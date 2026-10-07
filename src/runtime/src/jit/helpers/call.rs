#![allow(dangerous_implicit_autorefs)]
//! Direct call (`jit_call`) and corelib builtin dispatch (`jit_builtin`).

use crate::metadata::Value;

use super::super::frame::{FnEntry, JitFrame, JitModuleCtx};
use super::super::invoke::call_entry;
use super::{set_exception, vm_ctx_ref};

/// Direct call. The callee is named by its JIT id (= `FnId`, see
/// `JitModuleCtx`), cheapest source first:
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
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32,
    method_id: u32,
    fn_name_ptr: *const u8, fn_name_len: usize,
    args_ptr: *const u32, argc: usize,
    ic_ptr: *const std::sync::atomic::AtomicU32,
    caller_offset: u32, // packed code offset of the call site
) -> u8 {
    use crate::metadata::tokens::UNRESOLVED;
    let ctx_ref   = &*ctx;
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
        if let Some(entry) = ctx_ref.resolve_fn_by_id_tiered(token as usize) {
            return call_compiled(frame_ref, ctx, dst, entry, arg_regs, caller_offset);
        }
        if let Some(callee) = ctx_ref.fn_of(token as usize) {
            return call_interp(frame_ref, ctx, dst, callee, arg_regs, caller_offset);
        }
    }

    // Tier 3: bind by name (the first call through this site). Signature mismatch /
    // undefined function surface as catchable exceptions and are not cached.
    let vm = vm_ctx_ref(ctx);
    let module = &*ctx_ref.module;
    let name = std::str::from_utf8(std::slice::from_raw_parts(fn_name_ptr, fn_name_len))
        .unwrap_or("<invalid>");
    let site = if ic_ptr.is_null() { None } else { Some(&*ic_ptr) };
    let mut holder: Option<std::sync::Arc<crate::metadata::Function>> = None;
    let (callee, id) = match crate::interp::bind_callee(vm, module, name, argc, site, &mut holder) {
        Ok(bound) => bound,
        Err(exc) => { set_exception(vm, exc); return 1; }
    };
    if let Some(entry) = id.and_then(|id| ctx_ref.resolve_fn_by_id_tiered(id)) {
        return call_compiled(frame_ref, ctx, dst, entry, arg_regs, caller_offset);
    }
    call_interp(frame_ref, ctx, dst, callee, arg_regs, caller_offset)
}

/// Run a compiled callee: module-init and cctor barriers (keyed by the callee's own
/// name — nothing to decode), then the native call with the callee frame filled
/// straight from the caller's registers.
#[inline]
unsafe fn call_compiled(
    frame_ref: &mut JitFrame, ctx: *const JitModuleCtx, dst: u32,
    entry: &FnEntry, arg_regs: &[u32], caller_offset: u32,
) -> u8 {
    let vm = vm_ctx_ref(ctx);
    // add-static-constructors：调用该类型的静态方法是 C# 的类型初始化触发点之一。
    // fix-crosspkg-static-call-cctor：必须在解析**之后**——首次跨包调用的解析正是加载依赖包、
    // 登记其 cctor 的那一步。add-module-init-hook：包级初始化先于类型初始化。两道门都在
    // 函数内短路，稳态下各是一次 relaxed load。
    if vm.module_init_gate_open() || vm.any_cctor_pending() {
        if let Some(exc) = call_barriers(vm, ctx, &*entry.func, &entry.owner_init) {
            set_exception(vm, exc);
            return 1;
        }
    }
    // jit-stack-trace: stamp the caller's call-site offset. Before the forwarder
    // short-circuit too: with the wrapper's row gone, the caller's row is the top
    // of a trace captured inside the builtin.
    vm.set_top_frame_pc(caller_offset);
    // `[Native]` extern wrapper: dispatch its builtin straight from our registers
    // instead of building the wrapper's activation (see `jit::forward`).
    if let Some(id) = entry.forward {
        if arg_regs.len() == (*entry.func).param_count {
            return super::super::forward::call_forward(frame_ref, ctx, dst, id, None, arg_regs);
        }
    }
    let callee_frame = JitFrame::new_args_from(vm, entry.max_reg, &frame_ref.regs, arg_regs);
    call_entry(vm, ctx, entry, callee_frame).store_into(&mut frame_ref.regs, dst)
}

/// Run a callee without a compiled entry (below the tier threshold, or
/// untranslatable) on the interpreter, its frame filled straight from the
/// caller's registers, and splice the result back into the JIT caller's frame.
#[inline(never)]
unsafe fn call_interp(
    frame_ref: &mut JitFrame, ctx: *const JitModuleCtx, dst: u32,
    callee: &crate::metadata::Function, arg_regs: &[u32], caller_offset: u32,
) -> u8 {
    let vm = vm_ctx_ref(ctx);
    let module = &*(*ctx).module;
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
    match crate::interp::exec_function_from_regs(vm, module, callee, &frame_ref.regs, arg_regs, &[]) {
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
    vm: &crate::vm_context::VmContext, ctx: *const JitModuleCtx,
    callee: &crate::metadata::Function, owner: &crate::metadata::bytecode::OwnerInitCell,
) -> Option<Value> {
    let module = &*(*ctx).module;
    if let Err(msg) = vm.ensure_module_inits(Some(&callee.name)) {
        return Some(crate::vm_context::cctor::make_type_init_exception(vm, module, &msg));
    }
    if let Err(msg) = vm.ensure_callee_owner_init(&callee.name, owner) {
        return Some(crate::vm_context::cctor::make_type_init_exception(vm, module, &msg));
    }
    None
}

/// `jit_builtin` after `formalize-jit-method-token` (2026-05-08): receives
/// pre-resolved `BuiltinId` directly (not name pointers). Resolver
/// guarantees every `Instruction::Builtin.name` resolves at module load
/// (closed set; panic on miss), so JIT codegen embeds the id as an i32
/// constant in the generated machine code. Helper indexes
/// `BUILTINS[id]` directly — zero hash on every call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_builtin(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32,
    builtin_id: u32,
    name_ptr: *const u8, name_len: usize,
    args_ptr: *const u32, argc: usize,
) -> u8 {
    let frame_ref = &mut *frame;
    let arg_regs  = std::slice::from_raw_parts(args_ptr, argc);
    // Inline storage: builtins take ≤ 4 args almost always — no heap Vec per call.
    let args: smallvec::SmallVec<[Value; 4]> = arg_regs.iter().map(|&r| frame_ref.regs[r as usize].clone()).collect();

    let vm = vm_ctx_ref(ctx);
    // fix-jit-builtin-ext-fallback: `UNRESOLVED` means the resolver could not bind this
    // site's name to a static or ext builtin at resolve time (e.g. a native-ext facade
    // resolved before its lib was loaded in this VM). Resolve by name now, which re-checks
    // the per-VM ext registry — mirrors interp `exec_call::builtin`'s name fallback. The
    // hot path (resolved id) still dispatches by index with no hashing.
    let result = if builtin_id != crate::metadata::tokens::UNRESOLVED {
        crate::corelib::exec_builtin_by_id(vm, crate::metadata::tokens::BuiltinId(builtin_id), &args)
    } else {
        let name = std::str::from_utf8(std::slice::from_raw_parts(name_ptr, name_len)).unwrap_or("<invalid>");
        crate::corelib::exec_builtin(vm, name, &args)
    };
    match result {
        // split-null-sentinel-channels ④：void builtin ⇒ 不写 `dst`（与 interp
        // `exec_call::builtin` 逐字同款 —— 两个后端必须同时改，否则就是
        // 「两个后端只有一个错」那种最难发现的形态）。
        Ok(Some(v)) => { frame_ref.regs[dst as usize] = v; 0 }
        Ok(None)    => 0,
        Err(e) => builtin_error_into_exception(vm, ctx, e),
    }
}

/// Turn a builtin's `Err` into the pending JIT exception and report `1`. Shared
/// by `jit_builtin` and the forwarder short-circuit (`jit::forward`).
///
/// # Safety
/// `ctx` must be a valid `JitModuleCtx` (as for every JIT helper).
pub(crate) unsafe fn builtin_error_into_exception(
    vm: &crate::vm_context::VmContext, ctx: *const JitModuleCtx, e: anyhow::Error,
) -> u8 {
    // A callback builtin (reflection `MethodInfo.Invoke`) that ran z42
    // code which threw stashed the ORIGINAL exception value — propagate
    // it with its real type, not wrapped into Std.Exception (parity with
    // interp `exec_call::builtin`).
    if let Some(thrown) = vm.take_pending_thrown() {
        set_exception(vm, thrown);
        return 1;
    }
    // Same exception as interp `exec_call::builtin` (one shared mapping): a typed
    // throw (null receiver → NullReferenceException) or a `Std.Exception`, so
    // JIT-compiled code can `catch` it. Raw string only when the class isn't loaded.
    let module = unsafe { &*(*ctx).module };
    let exc = crate::corelib::builtin_error_exception(vm, module, e)
        .unwrap_or_else(|e| Value::Str(e.to_string().into()));
    set_exception(vm, exc);
    1
}
