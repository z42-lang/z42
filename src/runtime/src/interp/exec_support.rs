//! exec_function 族入口、FrameGuard、OSR / 原生分流、异常事件与 handler 查找、ref 写回。refactor-split-interp-mod（2026-09-03）：自 1155 行的 `interp/mod.rs` 逐行搬出，
//! mod.rs 只留模块表与执行主循环 `exec_function_body`；本模块经 mod.rs 的 `pub(crate) use` 全量再导出，
//! 兄弟模块的 `super::X` 路径不变。

#![allow(unused_imports)]
use super::*;
use crate::metadata::{BranchTargets, Function, Module, Terminator, Value};
use crate::vm_context::VmContext;
use anyhow::{bail, Context, Result};
use std::collections::HashMap;

/// RAII guard ensuring push_frame / pop_frame stay strictly paired even
/// across `?` early-return or panic unwind from `exec_function`.
///
/// 2026-05-10 unify-frame-chain collapsed the previous trio of pops
/// (regs / env_arena / call_frame) into a single `pop_frame()` matching
/// the new single-row VmFrame model.
pub(super) struct FrameGuard<'a> {
    pub(super) ctx: &'a VmContext,
}
impl Drop for FrameGuard<'_> {
    fn drop(&mut self) {
        self.ctx.pop_frame();
    }
}

pub(crate) fn exec_function(ctx: &VmContext, module: &Module, func: &Function, args: &[Value]) -> Result<ExecOutcome> {
    // add-gc-safepoint (2026-05-20): every newly-entered z42 function
    // immediately respects a pending GC request. A worker thread spawned
    // mid-collect parks here before touching any roots.
    crate::gc::safepoint::check_safepoint(ctx);
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
    if let Some(outcome) = try_native_exec(ctx, func, args) {
        return outcome;
    }
    let frame = Frame::new(args, func.max_reg);
    exec_function_body(ctx, module, func, frame)
}

/// add-generic-activator: resolve method-type-arg *forwarding* markers in `mta`
/// against the CALLER frame's already-concrete `method_type_args`. A marker
/// `"$mta:N"` (emitted when a generic call's type-arg is a bare method-level type
/// param of the enclosing method — `Foo<T>() { Bar<T>() }`) is replaced by
/// `caller.method_type_args[N]`. Non-marker entries pass through. Only called when
/// the caller carries at least one marker (see the `starts_with("$mta:")` guard at
/// the call sites), so the common concrete-type-arg path allocates nothing.
pub(super) fn resolve_forwarded_mta(caller: &Frame, mta: &[String]) -> Vec<String> {
    mta.iter()
        .map(|s| {
            if let Some(n) = s.strip_prefix("$mta:") {
                if let Ok(i) = n.parse::<usize>() {
                    if let Some(v) = caller.method_type_args.get(i) {
                        return v.clone();
                    }
                }
            }
            s.clone()
        })
        .collect()
}

/// add-reflective-invoke: like [`exec_function`] but threads method-level generic
/// `method_type_args` into the callee frame, so a reflectively-invoked *constructed*
/// generic method (`MethodInfo.MakeGenericMethod(..).Invoke(..)`) materializes
/// `typeof(T)`/`new T()`/`default(T)` via the M1 `frame.method_type_args` slot —
/// identical to a direct `Foo<T>()` call. An empty slice is behaviourally identical
/// to `exec_function` (same JIT backstop + frame), so non-generic reflective invokes
/// keep their exact prior path. Generic methods are never JIT-compiled (M1's
/// `jit_unsupported_reason`), so the `try_native_exec` fast lane is only consulted
/// for the empty (non-generic) case.
pub(crate) fn exec_function_with_type_args(
    ctx: &VmContext,
    module: &Module,
    func: &Function,
    args: &[Value],
    method_type_args: &[String],
) -> Result<ExecOutcome> {
    crate::gc::safepoint::check_safepoint(ctx);
    if method_type_args.is_empty() {
        if let Some(outcome) = try_native_exec(ctx, func, args) {
            return outcome;
        }
    }
    let mut frame = Frame::new(args, func.max_reg);
    if !method_type_args.is_empty() {
        frame.method_type_args = method_type_args.into();
    }
    exec_function_body(ctx, module, func, frame)
}

/// runtime-jit-tiering Phase 1.5.2: name-based mixed-mode divert used as the
/// universal backstop at `exec_function`. Returns `None` (→ interpret) when no JIT
/// ctx is published (interp-only run), when an argument is a `Ref` (a stack address
/// can't cross into native code — see `super::exec_call::try_native_static_call`; a
/// compiled fn never has ref params anyway, so this is defensive), or when the
/// function is cold / untranslatable (`resolve_fn_by_name_tiered` → None). On a
/// compiled hit it runs the native code and marshals the result into an
/// `ExecOutcome`, mirroring `try_native_static_call`.
#[cfg(feature = "jit")]
pub(super) fn try_native_exec(ctx: &VmContext, func: &Function, args: &[Value]) -> Option<Result<ExecOutcome>> {
    let p = ctx.jit_ctx_ptr();
    if p == 0 { return None; }
    if args.iter().any(|a| matches!(a, Value::Ref { .. })) { return None; }
    let jit_ctx = p as *const crate::jit::frame::JitModuleCtx;
    // SAFETY: `jit_ctx` is valid for the whole `JitModule::run_fn` (set/cleared in
    // lockstep with `vm_ctx`). Copy the small entry fields out before the native
    // call so no borrow of `*jit_ctx` is held across it.
    // Phase 1.5.2: peek (already-compiled?) — NOT the tiered resolve. The divert
    // only ROUTES already-hot functions to native; tier-up counting belongs to the
    // primary call sites. The tiered resolve here double-counted a cold callee
    // (jit_call's counter, then this fallback's) — halving the effective threshold.
    let (max_reg, ptr, name, file) = {
        let entry = unsafe { (*jit_ctx).resolve_fn_by_name_peek(&func.name) }?;
        (entry.max_reg, entry.ptr, entry.name.clone(), entry.file.clone())
    };
    ctx.counters().jit_native_from_interp.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut callee = crate::jit::frame::JitFrame::new(max_reg, args);
    let jit_fn: crate::jit::helpers::JitFn = unsafe { std::mem::transmute(ptr) };
    ctx.push_frame(crate::exception::VmFrame::new(
        name, file, &callee.regs as *const _, &callee.env_arena as *const _));
    let r = unsafe { jit_fn(&mut callee, jit_ctx) };
    ctx.pop_frame();
    if r != 0 {
        callee.recycle();
        return Some(Ok(ExecOutcome::Thrown(ctx.take_exception().unwrap_or(Value::Null))));
    }
    let ret = callee.ret.take();
    callee.recycle();
    Some(Ok(ExecOutcome::Returned(ret)))
}

#[cfg(not(feature = "jit"))]
#[inline]
pub(super) fn try_native_exec(_ctx: &VmContext, _func: &Function, _args: &[Value]) -> Option<Result<ExecOutcome>> {
    None
}

/// add-osr-loop-tiering: on a hot loop back-edge, hand the running interp
/// activation over to native code (On-Stack Replacement). Called at every backward
/// branch; bumps the per-activation back-edge counter and, **exactly when** it
/// reaches `osr_threshold`, compiles (or reuses) an OSR entry at `loop_header` and
/// resumes there with the live register state. Returns `Some(outcome)` if OSR took
/// over (the function ran to completion natively), `None` to keep interpreting.
///
/// OSR only applies to translatable functions (guaranteed no `ref`/`out` params →
/// no `LoadLocalAddr` → no exit copy-out to skip), so returning here without the
/// interpreter's normal exit path is correct. `frame.regs` is cloned into the OSR
/// frame; block `0..K` results the interpreter already computed live there.
#[cfg(feature = "jit")]
pub(super) fn try_osr(ctx: &VmContext, frame: &mut Frame, func: &Function, loop_header: usize)
    -> Option<Result<ExecOutcome>>
{
    let p = ctx.jit_ctx_ptr();
    if p == 0 { return None; }                        // interp-only mode: no OSR
    frame.back_edge_count = frame.back_edge_count.wrapping_add(1);
    let jit_ctx = p as *const crate::jit::frame::JitModuleCtx;
    // SAFETY: jit_ctx is valid for the whole JitModule::run_fn (set/cleared in
    // lockstep with vm_ctx). Only touched through &-methods / Copy field reads.
    let threshold = unsafe { (*jit_ctx).osr_threshold };
    if frame.back_edge_count != threshold { return None; }   // fire exactly once
    // v1: OSR only merged functions — resolve this function's merged id by name.
    let id = unsafe { (*(*jit_ctx).module).func_index.get(&func.name).copied() }?;
    let entry = unsafe { (*jit_ctx).resolve_osr_entry(id, loop_header) }?; // owned FnEntry
    ctx.counters().jit_native_from_interp.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut osr = crate::jit::frame::JitFrame::from_interp_regs(&frame.regs, entry.max_reg);
    // add-struct-jit-value-path (P5): OSR continues the SAME logical activation, so
    // the native frame must inherit the interp frame's id — any StructRef the loop
    // already allocated (frame_id = interp's) must still deref after hand-off, and
    // new struct allocs in OSR code stay consistent with them.
    osr.frame_id = frame.frame_id;
    let jit_fn: crate::jit::helpers::JitFn = unsafe { std::mem::transmute(entry.ptr) };
    // NB v1 simplification: the interpreter's own VmFrame for this activation is
    // still on the stack; we push a second one for the OSR native frame. GC scans
    // both — the interp regs are clones of the OSR regs (same heap refs), so the
    // double-scan is conservatively correct. A crash trace shows the frame twice
    // (cosmetic). Popped here; the interp frame's guard pops on the `return` below.
    ctx.push_frame(crate::exception::VmFrame::new(
        entry.name, entry.file, &osr.regs as *const _, &osr.env_arena as *const _));
    let r = unsafe { jit_fn(&mut osr, jit_ctx) };
    ctx.pop_frame();
    if r != 0 {
        osr.recycle();
        return Some(Ok(ExecOutcome::Thrown(ctx.take_exception().unwrap_or(Value::Null))));
    }
    let ret = osr.ret.take();
    osr.recycle();
    Some(Ok(ExecOutcome::Returned(ret)))
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
/// (jit/helpers/call.rs). Used by the non-virtual `Call` path (super::exec_call::call),
/// which passes plain register indices with no receiver prepend.
pub(crate) fn exec_function_from_regs(
    ctx: &VmContext, module: &Module, func: &Function,
    caller_regs: &[Value], arg_indices: &[u32],
    // add-generic-methods: resolved FQ type-arg names for a generic call (empty
    // for non-generic). Stored on the callee frame for MethodTypeArg/MethodDefault.
    method_type_args: &[String],
) -> Result<ExecOutcome> {
    crate::gc::safepoint::check_safepoint(ctx);
    let mut frame = Frame::new_from_regs(caller_regs, arg_indices, func.max_reg)?;
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
    crate::gc::safepoint::check_safepoint(ctx);
    let mut frame = Frame::new_from_receiver_regs(receiver, caller_regs, arg_indices, func.max_reg)?;
    if !method_type_args.is_empty() { frame.method_type_args = method_type_args.into(); }
    exec_function_body(ctx, module, func, frame)
}
/// Spec impl-ref-out-in-runtime: copy-out for `ref`/`out` params. Iterate
/// `frame.ref_writebacks`; for each `(reg, original_ref_kind)`, take the
/// callee's final value of that reg and store it through the original Ref
/// to the caller's lvalue. Runs before every function-exit return path
/// (normal return + uncaught throw).
/// Phase 2 D3+D6 (2026-05-26): increment `exceptions_thrown` counter and
/// fire `RuntimeEvent::ExceptionThrown`. Reads class name + message from
/// the thrown value if it's an Exception subclass; otherwise stamps both
/// as `"<non-exception-value>"`. Message truncated to 256 chars to keep
/// the event firehose bounded.
pub(super) fn fire_exception_thrown(ctx: &VmContext, module: &crate::metadata::Module, val: &crate::metadata::Value) {
    use std::sync::atomic::Ordering;
    ctx.counters().exceptions_thrown.fetch_add(1, Ordering::Relaxed);
    let (class, mut message) = exception_class_and_message(val, module);
    if message.len() > 256 {
        message.truncate(256);
        message.push_str("…");
    }
    ctx.fire_runtime_event(&crate::observer::RuntimeEvent::ExceptionThrown { class, message });
}

/// Phase 2 D3+D6 sibling: increment `exceptions_caught` + fire
/// `RuntimeEvent::ExceptionCaught`. `frames_unwound` = 0 for same-frame
/// catch; 1 for callee-thrown + caller-caught; >1 for deeper unwind.
pub(super) fn fire_exception_caught(
    ctx: &VmContext, module: &crate::metadata::Module,
    val: &crate::metadata::Value, frames_unwound: u32,
) {
    use std::sync::atomic::Ordering;
    ctx.counters().exceptions_caught.fetch_add(1, Ordering::Relaxed);
    let (class, _) = exception_class_and_message(val, module);
    ctx.fire_runtime_event(&crate::observer::RuntimeEvent::ExceptionCaught { class, frames_unwound });
}

pub(super) fn exception_class_and_message(
    val: &crate::metadata::Value, module: &crate::metadata::Module,
) -> (String, String) {
    use crate::metadata::Value;
    let class = match val {
        Value::Object(rc) => rc.type_desc().name.clone(),
        _ => "<non-exception-value>".to_string(),
    };
    let message = crate::exception::read_message(val, module).unwrap_or_default();
    (class, message)
}

pub(super) fn run_ref_writebacks(frame: &Frame, ctx: &VmContext) -> Result<()> {
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
/// catch-by-generic-type (2026-05-06): catch_type semantics —
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
    type_registry: &rustc_hash::FxHashMap<String, std::sync::Arc<crate::metadata::TypeDesc>>,
    thrown: &Value,
) -> Option<usize> {
    // perf-vm-isa-cache: match on the thrown object's descriptor (identity-cached), no
    // per-throw `String` clone of its class name.
    let thrown_td: Option<&crate::metadata::TypeDesc> = match thrown {
        Value::Object(rc) => Some(rc.type_desc()),
        _                 => None,
    };

    for (i, entry) in func.exception_table().iter().enumerate() {
        let start_idx = *block_map.get(&entry.try_start)?;
        let end_idx   = *block_map.get(&entry.try_end)?;
        if !(block_idx >= start_idx && block_idx < end_idx) { continue; }

        match entry.catch_type.as_deref() {
            None      => return Some(i),                   // user untyped catch
            Some("*") => return Some(i),                   // synthetic finally fallthrough
            Some(target) => {
                if let Some(td) = thrown_td {
                    if super::dispatch::isa_td(ctx, type_registry, td, target) {
                        return Some(i);
                    }
                }
                // type mismatch — try next entry
            }
        }
    }
    None
}
