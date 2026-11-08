//! Activation entry, frame guards, native routing and exception bookkeeping.

#![allow(unused_imports)]
use super::*;
use crate::vm::exec::ops::references::store_thru_ref;
use crate::vm::runtime::{Function, Module};
use crate::{
    metadata::{BranchTargets, Terminator},
    object::Value,
};
use crate::vm::context::VmContext;
use anyhow::{bail, Context, Result};
use std::collections::HashMap;

/// RAII guard ensuring push_frame / pop_frame stay strictly paired even
/// across `?` early-return or panic unwind from `exec_function`. After the pop
/// it hands the frame's register file back to the context's register pool — the
/// popped `VmFrame` no longer roots it, so the pool's `clear()` is safe.
pub(super) struct FrameGuard<'a> {
    pub(super) ctx: &'a VmContext,
    /// `&mut frame.regs` of the entered `Frame`, which outlives the guard (the
    /// guard is declared after it in `exec_function_body`).
    regs: *mut Vec<Value>,
}
impl Drop for FrameGuard<'_> {
    fn drop(&mut self) {
        self.ctx.pop_frame();
        // SAFETY: `regs` points into the live `Frame` (see the field); nothing
        // else borrows it while the guard drops. The frame keeps an empty Vec.
        let regs = std::mem::take(unsafe { &mut *self.regs });
        self.ctx.exec.reg_pool.give(regs);
    }
}

/// The interpreter's frame entry: push one `VmFrame` enrolling `frame` as GC
/// root + stack-trace row, then check the native stack budget. The returned
/// guard pops the frame on every exit path (`?` propagation, panic unwind,
/// normal return) — including this function's own stack-overflow `Err`, which
/// is reported with the frame already on the stack — and recycles its registers.
/// The frame's arena id is taken lazily (`Frame::frame_id`), not here; the
/// bottom frame's push also installs the context's engine guards
/// (`VmContext::push_frame`).
///
/// Must not reach a safepoint before the push: until then the callee's
/// arguments are reachable only from `frame` (see `exec_function_body`).
///
/// SAFETY of the raw pointers: `frame.regs` lives in the caller's `Frame` on
/// the Rust call stack, and `func` is borrowed by the caller for the whole
/// activation — both outlive the guard.
pub(super) fn enter_frame<'a>(ctx: &'a VmContext, func: &Function, frame: &mut Frame) -> Result<FrameGuard<'a>> {
    ctx.push_frame(crate::vm::exec::call::frames::VmFrame::new(
        func as *const Function,
        &frame.regs as *const Vec<Value>,
    ));
    let guard = FrameGuard { ctx, regs: &mut frame.regs as *mut Vec<Value> };
    // runtime-audit P0-4: stack overflow is fatal — checked once the frame is
    // on the call stack, so the report includes it.
    crate::vm::exec::stack_guard::check(ctx)?;
    Ok(guard)
}

pub(crate) fn exec_function(ctx: &VmContext, module: &Module, func: &Function, args: &[Value]) -> Result<ExecOutcome> {
    // fix-callee-entry-safepoint-drops-args : the callee-entry safepoint used
    // to be **here**, before the frame exists. `args` is a caller-side temporary — for the
    // `new` path it is the only reference to the freshly allocated receiver — so a
    // collection at this point swept values the mutator was still using. It now lives in
    // `exec_function_body`, immediately after the frame is enrolled as a GC root.
    // runtime-jit-tiering Phase 1.5.2 (mixed-mode invariant backstop): if `func`
    // is already JIT-compiled, run its native code instead of interpreting it.
    // `exec_function` is the SINGLE choke point every non-hot-Call/VCall interp
    // path funnels through — constructors, closures, `ToString` dispatch, the
    // non-IC / vtable / base-fallback vcall paths, cross-zpkg static calls, and
    // builtin callbacks. Diverting here guarantees Decision 5's invariant — "a
    // compiled function is never interp-executed" — for ALL of them at once
    // (present and future), which is the hard precondition for Phase 2 IR reclaim.
    // The idx-based per-site hooks (`try_native_static_call` /
    // `try_native_method_call`) remain the hot-path fast lane; the two
    // `exec_function_from_*regs` variants are only reached from those hooked paths
    // (cold callees only), so this name-based backstop completes the coverage.
    if let Some(outcome) = try_native_exec(ctx, func, args, &[]) {
        return outcome;
    }
    let frame = Frame::new(ctx, args, func.max_reg);
    exec_function_body(ctx, module, func, frame)
}

/// add-generic-activator: resolve type-parameter markers in a generic call site's `mta`
/// against the CALLER activation. A bare method-level param of the enclosing generic method
/// (`Foo<T>() { Bar<T>() }`) is the forwarding marker `"$mta:N"` → `caller.method_type_args[N]`;
/// a class-level one (`class H<T> { M() { Bar<T>(); } }`) is `"$cta:N@Owner"` → the caller's
/// receiver; either may sit inside a constructed argument (`Bar<List<T>>()`). Same binding as
/// the caller's own `o is T` / `typeof(T)` (`type_param_env`). Only called when the site carries
/// at least one marker (see [`has_forwarded_mta`]), so the common concrete path allocates nothing.
pub(in crate::interp) fn resolve_forwarded_mta(ctx: &VmContext, caller: &Frame, mta: &[String]) -> Vec<String> {
    let env = caller.type_param_env();
    mta.iter().map(|s| env.substitute(ctx, s)).collect()
}

/// Does a call site's `method_type_args` carry type-parameter markers to bind first?
#[inline]
pub(in crate::interp) fn has_forwarded_mta(mta: &[String]) -> bool {
    mta.iter().any(|s| crate::metadata::index::type_param_markers::has_markers(s))
}

/// add-reflective-invoke: like [`exec_function`] but threads method-level generic
/// `method_type_args` into the callee frame, so a reflectively-invoked *constructed*
/// generic method (`MethodInfo.MakeGenericMethod(..).Invoke(..)`) materializes
/// `typeof(T)`/`new T()`/`default(T)` via the M1 `frame.method_type_args` slot —
/// identical to a direct `Foo<T>()` call. An empty slice is behaviourally identical
/// to `exec_function` (same JIT backstop + frame), so non-generic reflective invokes
/// keep their prior path. The native entry receives the same bound type arguments.
pub(crate) fn exec_function_with_type_args(
    ctx: &VmContext,
    module: &Module,
    func: &Function,
    args: &[Value],
    method_type_args: &[String],
) -> Result<ExecOutcome> {
    // fix-callee-entry-safepoint-drops-args: safepoint moved into `exec_function_body` —
    // see `exec_function`.
    if let Some(outcome) = try_native_exec(ctx, func, args, method_type_args) {
        return outcome;
    }
    let mut frame = Frame::new(ctx, args, func.max_reg);
    if !method_type_args.is_empty() {
        frame.method_type_args = method_type_args.into();
    }
    exec_function_body(ctx, module, func, frame)
}

/// runtime-jit-tiering Phase 1.5.2: mixed-mode divert used as the universal
/// backstop at `exec_function`, keyed by the function's JIT id (no hashing). Returns `None` (→ interpret) when the
/// VM has no JIT installed (interp-only run), when an argument is a `Ref` (a stack address
/// can't cross into native code — see `crate::interp::ops::exec_call::try_native_static_call`; a
/// compiled fn never has ref params anyway, so this is defensive), or when the
/// function is not compiled yet (or has no JIT id). On a
/// compiled hit it runs the native code and marshals the result into an
/// `ExecOutcome`, mirroring `try_native_static_call`.
#[cfg(feature = "jit")]
pub(super) fn try_native_exec(ctx: &VmContext, func: &Function, args: &[Value], method_type_args: &[String]) -> Option<Result<ExecOutcome>> {
    let tier = ctx.tier()?;
    if args.iter().any(|a| matches!(a, Value::Ref { .. })) { return None; }
    tier.invoke(ctx, crate::vm::exec::engine::TierTarget::Existing(func),
        crate::vm::exec::call::args::CallArgs::Values(args), method_type_args)
}

#[cfg(not(feature = "jit"))]
#[inline]
pub(super) fn try_native_exec(_ctx: &VmContext, _func: &Function, _args: &[Value], _method_type_args: &[String]) -> Option<Result<ExecOutcome>> {
    None
}

/// add-osr-loop-tiering: on a hot loop back-edge, hand the running interp
/// activation over to native code (On-Stack Replacement). Called at every backward
/// branch; bumps the per-activation back-edge counter and, **exactly when** it
/// reaches `osr_threshold`, compiles (or reuses) an OSR entry at `loop_header` and
/// resumes there with the live register state. Returns `Some(outcome)` if OSR took
/// over (the function ran to completion natively), `None` to keep interpreting.
///
/// A frame with `ref` params is never handed off (its exit copy-out would be
/// skipped), so returning here without the interpreter's normal exit path is
/// correct. `frame.regs` is cloned into the OSR frame; block `0..K` results the
/// interpreter already computed live there.
///
/// Runs on every loop back-edge, so the counting is inlined into the
/// interpreter loop and only the hand-off itself stays out of line.
#[cfg(feature = "jit")]
#[inline(always)]
pub(super) fn try_osr(ctx: &VmContext, frame: &mut Frame, func: &Function, loop_header: usize)
    -> Option<Result<ExecOutcome>>
{
    let tier = ctx.tier()?;
    frame.back_edge_count = frame.back_edge_count.wrapping_add(1);
    if frame.back_edge_count != tier.osr_threshold { return None; }
    tier.osr(ctx, func, loop_header, crate::vm::exec::engine::OsrState {
        regs: &frame.regs, frame_id: frame.frame_id_if_taken(),
        method_type_args: &frame.method_type_args,
        has_ref_writebacks: !frame.ref_writebacks.is_empty(),
    })
}

#[cfg(not(feature = "jit"))]
#[inline]
pub(super) fn try_osr(_ctx: &VmContext, _frame: &mut Frame, _func: &Function, _loop_header: usize)
    -> Option<Result<ExecOutcome>> { None }

/// perf-vm-iteration Phase 1 (Decision 3): hot direct-call entry that fills the
/// callee register file **directly** from the caller's registers + argument
/// indices — no intermediate `collect_args` `Vec<Value>` alloc, and each arg is
/// cloned **once** (caller reg → callee reg) instead of twice (caller reg →
/// args Vec → callee reg). Mirrors the JIT's `JitFrame::new_args_from`
/// (jit/helpers/call.rs). Used by the non-virtual `Call` path (crate::interp::ops::exec_call::call),
/// which passes plain register indices with no receiver prepend.
pub(crate) fn exec_function_from_regs(
    ctx: &VmContext, module: &Module, func: &Function,
    caller_regs: &[Value], arg_indices: &[u32],
    // add-generic-methods: resolved FQ type-arg names for a generic call (empty
    // for non-generic). Stored on the callee frame for MethodTypeArg/MethodDefault.
    method_type_args: &[String],
) -> Result<ExecOutcome> {
    // fix-callee-entry-safepoint-drops-args: safepoint moved into `exec_function_body`.
    let mut frame = Frame::new_from_regs(ctx, caller_regs, arg_indices, func.max_reg)?;
    if !method_type_args.is_empty() { frame.method_type_args = method_type_args.into(); }
    exec_function_body(ctx, module, func, frame)
}

/// perf-vm-iteration Phase 1 (Decision 3): virtual-call hot-path entry. Fills
/// `regs[0] = receiver`, `regs[1+i] = caller_regs[arg_indices[i]]` directly —
/// no `vec![receiver]` / `collect_args` Vecs, each value cloned once. Used by
/// the `exec_vcall` object/primitive IC fast path.
pub(crate) fn exec_function_from_receiver_regs(
    ctx: &VmContext, module: &Module, func: &Function,
    receiver: &Value, caller_regs: &[Value], arg_indices: &[u32],
    method_type_args: &[String],   // add-generic-methods: see exec_function_from_regs
) -> Result<ExecOutcome> {
    // fix-callee-entry-safepoint-drops-args: safepoint moved into `exec_function_body`.
    let mut frame = Frame::new_from_receiver_regs(ctx, receiver, caller_regs, arg_indices, func.max_reg)?;
    if !method_type_args.is_empty() { frame.method_type_args = method_type_args.into(); }
    exec_function_body(ctx, module, func, frame)
}
/// Spec impl-ref-out-in-runtime: copy-out for `ref`/`out` params. Iterate
/// `frame.ref_writebacks`; for each `(reg, original_ref_kind)`, take the
/// callee's final value of that reg and store it through the original Ref
/// to the caller's lvalue. Runs before every function-exit return path
/// (normal return + uncaught throw).
/// Phase 2 D3+D6 : increment `exceptions_thrown` counter and
/// fire `RuntimeEvent::ExceptionThrown`. Reads class name + message from
/// the thrown value if it's an Exception subclass; otherwise stamps both
/// as `"<non-exception-value>"`. Message truncated to 256 chars to keep
/// the event firehose bounded.
pub(super) fn fire_exception_thrown(ctx: &VmContext, module: &crate::vm::runtime::Module, val: &crate::object::Value) {
    use std::sync::atomic::Ordering;
    ctx.counters().exceptions_thrown.fetch_add(1, Ordering::Relaxed);
    if !ctx.has_runtime_observers() { return; }
    let (class, mut message) = exception_class_and_message(ctx, val, module);
    if message.len() > 256 {
        message.truncate(256);
        message.push_str("…");
    }
    ctx.fire_runtime_event(&crate::vm::diagnostics::observer::RuntimeEvent::ExceptionThrown { class, message });
}

/// Phase 2 D3+D6 sibling: increment `exceptions_caught` + fire
/// `RuntimeEvent::ExceptionCaught`. `frames_unwound` = 0 for same-frame
/// catch; 1 for callee-thrown + caller-caught; >1 for deeper unwind.
pub(super) fn fire_exception_caught(
    ctx: &VmContext, module: &crate::vm::runtime::Module,
    val: &crate::object::Value, frames_unwound: u32,
) {
    use std::sync::atomic::Ordering;
    ctx.counters().exceptions_caught.fetch_add(1, Ordering::Relaxed);
    if !ctx.has_runtime_observers() { return; }
    let (class, _) = exception_class_and_message(ctx, val, module);
    ctx.fire_runtime_event(&crate::vm::diagnostics::observer::RuntimeEvent::ExceptionCaught { class, frames_unwound });
}

pub(super) fn exception_class_and_message(
    ctx: &VmContext, val: &crate::object::Value, module: &crate::vm::runtime::Module,
) -> (String, String) {
    use crate::object::Value;
    let class = match val {
        Value::Object(rc) => rc.type_desc().name.clone(),
        _ => "<non-exception-value>".to_string(),
    };
    let message = crate::vm::exec::exception::read_message(val, ctx, module).unwrap_or_default();
    (class, message)
}

/// Runs on every function exit; almost no frame has `ref` params, so the check
/// is inlined and the loop stays out of line.
#[inline(always)]
pub(super) fn run_ref_writebacks(frame: &Frame, ctx: &VmContext) -> Result<()> {
    if frame.ref_writebacks.is_empty() { return Ok(()); }
    run_ref_writebacks_slow(frame, ctx)
}

#[inline(never)]
fn run_ref_writebacks_slow(frame: &Frame, ctx: &VmContext) -> Result<()> {
    for (reg, kind) in &frame.ref_writebacks {
        let final_val = frame.regs.get(*reg as usize)
            .cloned()
            .unwrap_or(Value::Null);
        store_thru_ref(kind, final_val, ctx)?;
    }
    Ok(())
}

/// Find the index into `func.exception_table` of the first handler whose try
/// region covers `block_idx` AND whose declared `catch_type` matches the thrown
/// value's class (with subclass walk via the type registry).
///
/// catch-by-generic-type : catch_type semantics —
///   None       — wildcard (user wrote `catch { }` / `catch (e)`); always matches.
///   Some("*")  — synthetic finally fallthrough catchall (compiler-generated
///                when there is no user catch but a finally block exists).
///   Some(t)    — typed catch; matches when the thrown value is an instance of
///                class `t` or any of its subclasses (sibling lineages skipped).
///
/// Source-order is preserved: exception_table entries are written in catch-clause
/// order by FunctionEmitterStmts; this loop scans them in that order and returns
/// the first match — matching C# / Java first-source-match-wins semantics.
///
/// `thrown` is expected to be a `Value::Object` (z42 throw is restricted to
/// Exception-derived class instances); non-object throws fall through to the
/// untyped catches via the wildcard branches above.
pub(super) fn find_handler(
    ctx: &VmContext,
    func: &Function,
    block_idx: usize,
    block_map: &HashMap<String, usize>,
    type_registry: &rustc_hash::FxHashMap<String, std::sync::Arc<crate::object::TypeDesc>>,
    thrown: &Value,
    // Binds the type-parameter markers of a `catch (Err<T> e)` in a shared generic body.
    tp_env: crate::vm::exec::call::type_param_env::TypeParamEnv<'_>,
) -> Option<usize> {
    // A fatal VM error (stack overflow) unwinds past every handler.
    if crate::vm::exec::stack_guard::is_fatal(ctx) { return None; }
    // perf-vm-isa-cache: match on the thrown object's descriptor (cached by TypeId), no
    // per-throw `String` clone of its class name.
    // The thrown object's own type arguments take part, as in `is` (`crate::vm::exec::ops::cast::isa_object`):
    // `catch (Err<int>)` must not catch an `Err<string>` of an imported generic.
    let thrown_obj = match thrown {
        Value::Object(rc) => Some((rc.type_desc(), rc.type_args())),
        _                 => None,
    };

    for (i, entry) in func.exception_table().iter().enumerate() {
        // An entry whose labels don't resolve (the zbc reader names an
        // out-of-range `try_end` `block_{len}`) is skipped, not the end of the
        // search — later entries may still cover `block_idx`. Same as the JIT's
        // `find_handler_entries`.
        let Some(&start_idx) = block_map.get(&entry.try_start) else { continue };
        let Some(&end_idx)   = block_map.get(&entry.try_end)   else { continue };
        if !(block_idx >= start_idx && block_idx < end_idx) { continue; }

        match entry.catch_type.as_deref() {
            None      => return Some(i),                   // user untyped catch
            Some("*") => return Some(i),                   // synthetic finally fallthrough
            Some(target) => {
                if let Some((td, inst_args)) = thrown_obj {
                    let hit = if crate::metadata::index::type_param_markers::has_markers(target) {
                        let t = tp_env.test_target(ctx, target);
                        let key = crate::vm::runtime::ids::tokens::TypeKeyCell::new();
                        crate::vm::exec::ops::cast::isa_object(ctx, type_registry, td, inst_args, &t, &key)
                    } else {
                        crate::vm::exec::ops::cast::isa_object(ctx, type_registry, td, inst_args, target, func.catch_key(i))
                    };
                    if hit {
                        return Some(i);
                    }
                }
                // type mismatch — try next entry
            }
        }
    }
    None
}

#[cfg(test)]
#[path = "exec_support_tests.rs"]
mod exec_support_tests;
