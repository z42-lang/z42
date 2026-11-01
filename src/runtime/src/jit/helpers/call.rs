#![allow(dangerous_implicit_autorefs)]
//! Direct call (`jit_call`) and corelib builtin dispatch (`jit_builtin`).

use crate::metadata::Value;

use super::super::frame::{FnEntry, JitFrame};
use crate::vm_context::VmContext;
use super::super::invoke::call_entry;
use super::{set_exception, vm_ctx_ref};

/// Direct call. The callee is named by its JIT id (= `FnId`, see
/// `jit::runtime`), cheapest source first:
///   1. `method_id` baked at translate time (the site's token was already bound);
///   2. the site's `method_tokens` cell (`ic_ptr`), bound since — by either backend;
///   3. cold: bind by name (`interp::bind_callee` — may load the defining package,
///      checks the signature, stores the token), the only place the name is decoded.
///
/// The id then resolves to a compiled `FnEntry` (counted toward its tier-up), or —
/// cold or untranslatable — the callee runs on the interpreter straight from the
/// caller's registers. Barrier order on every path: resolve → signature →
/// module init → cctor → execute (the same order as interp `exec_call::call`).
///
/// `caller_offset` is this call site's packed code offset
/// (`Function::linear_offset`, a codegen constant). Stamped onto the caller's
/// `VmFrame::pc` before descending so a downstream throw's snapshot shows the
/// call site.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_call(
    frame: *mut JitFrame, ctx: *const VmContext,
    dst: u32,
    method_id: u32,
    fn_name_ptr: *const u8, fn_name_len: usize,
    args_ptr: *const u32, argc: usize,
    ic_ptr: *const std::sync::atomic::AtomicU32,
    caller_offset: u32, // packed code offset of the call site
) -> u8 {
    call_impl(frame, ctx, dst, method_id, fn_name_ptr, fn_name_len, args_ptr, argc, ic_ptr, caller_offset, &[])
}

/// `jit_call` for a method-level generic call site. `mta_ptr/len` is the site's
/// `method_type_args` (FQ type-name strings; a compile-time constant of the site, baked by
/// codegen — it points into the instruction the Module owns). It is threaded into the
/// callee frame only when the callee runs on the interpreter; a **compiled** callee never
/// observes it (`translate::unsupported`'s invariant: a function that could read its type
/// args does not compile), so the native path is the plain `jit_call` one.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_call_mta(
    frame: *mut JitFrame, ctx: *const VmContext,
    dst: u32,
    method_id: u32,
    fn_name_ptr: *const u8, fn_name_len: usize,
    args_ptr: *const u32, argc: usize,
    ic_ptr: *const std::sync::atomic::AtomicU32,
    caller_offset: u32,
    mta_ptr: *const String, mta_len: usize,
) -> u8 {
    let mta = std::slice::from_raw_parts(mta_ptr, mta_len);
    // Class-level type params forwarded by the site (`$cta:N@Owner`) bind from the receiver
    // (reg 0); a method-level one keeps the caller out of the JIT (`forwards_caller_mta`).
    let bound;
    let mta = match super::object::bind_forwarded_mta(frame, ctx, mta) {
        Some(v) => { bound = v; &bound[..] }
        None => mta,
    };
    call_impl(frame, ctx, dst, method_id, fn_name_ptr, fn_name_len, args_ptr, argc, ic_ptr, caller_offset, mta)
}

#[inline(always)]
#[allow(clippy::too_many_arguments)]
unsafe fn call_impl(
    frame: *mut JitFrame, ctx: *const VmContext,
    dst: u32,
    method_id: u32,
    fn_name_ptr: *const u8, fn_name_len: usize,
    args_ptr: *const u32, argc: usize,
    ic_ptr: *const std::sync::atomic::AtomicU32,
    caller_offset: u32,
    mta: &[String],
) -> u8 {
    use crate::metadata::tokens::UNRESOLVED;
    let rt        = super::jit_rt(ctx);
    let frame_ref = &mut *frame;
    let arg_regs  = std::slice::from_raw_parts(args_ptr, argc);

    let token = if method_id != UNRESOLVED {
        method_id
    } else if ic_ptr.is_null() {
        UNRESOLVED
    } else {
        (*ic_ptr).load(std::sync::atomic::Ordering::Relaxed)
    };
    if token != UNRESOLVED {
        // A bound token already passed its signature check (resolver / first bind).
        if let Some(entry) = rt.resolve_fn_by_id_tiered(&*ctx, token as usize) {
            return call_compiled(frame_ref, ctx, dst, entry, arg_regs, caller_offset, mta);
        }
        if let Some(callee) = rt.fn_of(token as usize) {
            return call_interp(frame_ref, ctx, dst, callee, arg_regs, caller_offset, mta);
        }
    }

    // Tier 3: bind by name (the first call through this site). Signature mismatch /
    // undefined function surface as catchable exceptions and are not cached.
    let vm = vm_ctx_ref(ctx);
    let module = super::jit_module(ctx);
    let name = std::str::from_utf8(std::slice::from_raw_parts(fn_name_ptr, fn_name_len))
        .unwrap_or("<invalid>");
    let site = if ic_ptr.is_null() { None } else { Some(&*ic_ptr) };
    let mut holder: Option<std::sync::Arc<crate::vm::runtime::Function>> = None;
    let (callee, id) = match crate::interp::bind_callee(vm, module, name, argc, site, &mut holder) {
        Ok(bound) => bound,
        Err(exc) => { set_exception(vm, exc); return 1; }
    };
    if let Some(entry) = id.and_then(|id| rt.resolve_fn_by_id_tiered(&*ctx, id)) {
        return call_compiled(frame_ref, ctx, dst, entry, arg_regs, caller_offset, mta);
    }
    call_interp(frame_ref, ctx, dst, callee, arg_regs, caller_offset, mta)
}

/// Run a compiled callee: module-init and cctor barriers (keyed by the callee's own
/// name — nothing to decode), then the native call with the callee frame filled
/// straight from the caller's registers. `mta` (a generic site's type args) is not
/// passed on — a compiled callee cannot read it — it only keeps a `[Native]` wrapper
/// from folding, as the interpreter's `try_fold` does for generic sites.
#[inline]
unsafe fn call_compiled(
    frame_ref: &mut JitFrame, ctx: *const VmContext, dst: u32,
    entry: &FnEntry, arg_regs: &[u32], caller_offset: u32, mta: &[String],
) -> u8 {
    let vm = vm_ctx_ref(ctx);
    // add-static-constructors：调用该类型的静态方法是 C# 的类型初始化触发点之一。
    // fix-crosspkg-static-call-cctor：必须在解析**之后**——首次跨包调用的解析正是加载依赖包、
    // 登记其 cctor 的那一步。add-module-init-hook：包级初始化先于类型初始化。两道门都在
    // 函数内短路，稳态下各是一次 relaxed load。
    // Settle fast path (the call-barrier twin of the static-field gate,
    // `statics.rs`): once a full pass proves the barrier will keep passing
    // (`callee_barrier_settled`), the entry records the epoch — read BEFORE the
    // barrier ran, so an event while it ran leaves the record stale (re-checked
    // next call), never wrong. The gate is then one acquire + one relaxed load.
    if !entry.init_ready_eq(vm.core.cctors.barrier_epoch()) {
        let epoch = vm.core.cctors.barrier_epoch_acquire();
        if vm.module_init_gate_open() || vm.any_cctor_pending() {
            if let Some(exc) = call_barriers(vm, ctx, &*entry.func, &entry.owner_init) {
                set_exception(vm, exc);
                return 1;
            }
        }
        if vm.callee_barrier_settled(&entry.owner_init) {
            entry.mark_init_ready(epoch);
        }
    }
    // jit-stack-trace: stamp the caller's call-site offset. Before the forwarder
    // short-circuit too: with the wrapper's row gone, the caller's row is the top
    // of a trace captured inside the builtin.
    vm.set_top_frame_pc(caller_offset);
    // `[Native]` extern wrapper: dispatch its builtin straight from our registers
    // instead of building the wrapper's activation (see `jit::forward`).
    if let Some(id) = entry.forward {
        if mta.is_empty() && arg_regs.len() == (*entry.func).param_count {
            return super::super::forward::call_forward(frame_ref, ctx, dst, entry.func, id, None, arg_regs);
        }
    }
    let callee_frame = JitFrame::new_args_from(vm, entry.max_reg, &frame_ref.regs, arg_regs);
    call_entry(vm, entry, callee_frame).store_into(&mut frame_ref.regs, dst)
}

/// Run a callee without a compiled entry (below the tier threshold, or
/// untranslatable) on the interpreter, its frame filled straight from the
/// caller's registers, and splice the result back into the JIT caller's frame.
#[inline(never)]
unsafe fn call_interp(
    frame_ref: &mut JitFrame, ctx: *const VmContext, dst: u32,
    callee: &crate::vm::runtime::Function, arg_regs: &[u32], caller_offset: u32, mta: &[String],
) -> u8 {
    let vm = vm_ctx_ref(ctx);
    let module = super::jit_module(ctx);
    // jit-stack-trace: stamp the caller's call-site offset before descending.
    vm.set_top_frame_pc(caller_offset);
    // runtime-ambiguous-use-site：与 interp `exec_call` 对称（两后端同判据）。
    // 常态 = 一次 relaxed 原子读，进程内没发生过碰撞时恒 false。
    if let Some(exc) = crate::vm_context::symres::ambiguous_function_exception(vm, module, &callee.name) {
        set_exception(vm, exc);
        return 1;
    }
    if let Some(exc) = call_barriers(vm, ctx, callee, &callee.owner_init) {
        set_exception(vm, exc);
        return 1;
    }
    // A cold `[Native]` wrapper (no compiled entry yet) folds too (`interp::forward`);
    // not for a generic site, as the interpreter's `try_fold`.
    if mta.is_empty() && arg_regs.len() == callee.param_count {
        if let Some(id) = crate::interp::forward::forward_of(vm, module, callee) {
            return super::super::forward::call_forward(frame_ref, ctx, dst, callee, id, None, arg_regs);
        }
    }
    match crate::interp::exec_function_from_regs(vm, module, callee, &frame_ref.regs, arg_regs, mta) {
        Ok(crate::interp::ExecOutcome::Returned(ret)) => {
            frame_ref.regs[dst as usize] = ret.unwrap_or(Value::Null);
            0
        }
        Ok(crate::interp::ExecOutcome::Thrown(val)) => { set_exception(vm, val); 1 }
        Err(e) => { set_exception(vm, Value::Str(e.to_string().into())); 1 }
    }
}

/// Package-init then type-init barrier for a static call of `callee`; `Some` = the
/// `TypeInitializationException` to throw.
unsafe fn call_barriers(
    vm: &crate::vm_context::VmContext, ctx: *const VmContext,
    callee: &crate::vm::runtime::Function, owner: &crate::vm::runtime::OwnerInitCell,
) -> Option<Value> {
    let module = super::jit_module(ctx);
    if let Err(msg) = vm.ensure_module_inits(Some(&callee.name)) {
        return Some(crate::vm_context::cctor::make_type_init_exception(vm, module, &msg));
    }
    if let Err(msg) = vm.ensure_callee_owner_init(&callee.name, owner) {
        return Some(crate::vm_context::cctor::make_type_init_exception(vm, module, &msg));
    }
    None
}

/// `jit_builtin`: the site's pre-resolved `BuiltinId` (or `UNRESOLVED` + the name, to
/// resolve now — which re-checks the ext registry, as the interpreter does). The
/// arguments are read in place from this frame's registers (`corelib::invoke_builtin`);
/// a void builtin leaves `dst` alone (same as interp `exec_call::builtin`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_builtin(
    frame: *mut JitFrame, ctx: *const VmContext,
    dst: u32,
    builtin_id: u32,
    name_ptr: *const u8, name_len: usize,
    args_ptr: *const u32, argc: usize,
) -> u8 {
    let frame_ref = &mut *frame;
    let arg_regs  = std::slice::from_raw_parts(args_ptr, argc);
    let vm = vm_ctx_ref(ctx);
    let name = if builtin_id != crate::metadata::tokens::UNRESOLVED { "" } else {
        std::str::from_utf8(std::slice::from_raw_parts(name_ptr, name_len)).unwrap_or("<invalid>")
    };
    match crate::corelib::invoke_builtin(vm, builtin_id, name, None, &frame_ref.regs, arg_regs) {
        Ok(Some(v)) => { frame_ref.regs[dst as usize] = v; 0 }
        Ok(None)    => 0,
        Err(e) => {
            let module = super::jit_module(ctx);
            let who = if name.is_empty() { crate::corelib::builtin_name(builtin_id, "?") } else { name };
            raised_into_exception(vm, crate::corelib::abi::error::raise(vm, module, who, *e, &[]))
        }
    }
}

/// A failed builtin call → the JIT's exception channel (pending exception + status 1).
/// The thrown value comes from the shared `abi::error::raise` (same class, message and
/// trace as the interpreter). A fatal error — or an exception class that is not loaded —
/// leaves the error text: while the fatal flag is set the catch dispatch declines every
/// handler, so it unwinds to the entry, which reports it.
pub(crate) fn raised_into_exception(
    vm: &crate::vm_context::VmContext, r: crate::corelib::abi::error::Raised,
) -> u8 {
    use crate::corelib::abi::error::Raised;
    let exc = match r {
        Raised::Throw(v) => v,
        Raised::Fatal(e) | Raised::Raw(e) => Value::Str(e.to_string().into()),
    };
    set_exception(vm, exc);
    1
}
