#![allow(dangerous_implicit_autorefs)]
//! Direct call (`jit_call`) and corelib builtin dispatch (`jit_builtin`).

use crate::metadata::Value;

use super::super::frame::{FnEntry, JitFrame, JitModuleCtx};
use super::{set_exception, vm_ctx_ref, JitFn};

/// `jit_call` after formalize-jit-method-token Phase 2.C (2026-05-08):
/// hot path takes pre-resolved `MethodId` and indexes `fn_entries_by_id`
/// directly. On `UNRESOLVED` (cross-zpkg), falls back to name-based
/// HashMap lookup. Name pointer kept for diagnostics + fallback.
///
/// `caller_line` / `caller_col` (jit-stack-trace + span-column-propagate,
/// 2026-05-10) are the source position of this call site — codegen passes
/// both as constants. Stamped onto the caller's FrameInfo before descending
/// so a downstream throw's snapshot shows the precise call site.
/// `caller_col == 0` means unknown (zbc < 1.1) — formatter degrades to
/// `(file:line)`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_call(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32,
    method_id: u32,
    fn_name_ptr: *const u8, fn_name_len: usize,
    args_ptr: *const u32, argc: usize,
    ic_ptr: *const std::sync::atomic::AtomicU32,
    caller_line: u32,
    caller_col:  u32,
    caller_offset: u32, // add-offline-symbolication: linearized code offset
) -> u8 {
    use crate::metadata::tokens::UNRESOLVED;
    let ctx_ref   = &*ctx;
    let frame_ref = &mut *frame;

    // Resolve the callee to a slot id, then to its (lazily-compiled) FnEntry.
    // Three tiers, cheapest first:
    //   1. `method_id` resolved at codegen (intra-module) → by-id, lock-free.
    //   2. per-site IC hit (make-vm-loading-lazy) → cached id, by-id, lock-free.
    //   3. cold: resolve the name (registers a lazy slot if cross-zpkg), then
    //      cache the id in the IC so tiers 2 wins next time.
    // Borrow the FnEntry (don't clone): it lives in the read-only `ctx_ref`;
    // cloning copied two `Arc<str>` (name + file) per call re-cloned in
    // push_frame anyway.
    let entry_ref: Option<&FnEntry> = if method_id != UNRESOLVED {
        ctx_ref.resolve_fn_by_id_tiered(method_id as usize)
    } else {
        // Tier 2: per-site inline cache.
        let cached = if ic_ptr.is_null() {
            UNRESOLVED
        } else {
            (*ic_ptr).load(std::sync::atomic::Ordering::Relaxed)
        };
        if cached != UNRESOLVED {
            ctx_ref.resolve_fn_by_id_tiered(cached as usize)
        } else {
            // Tier 3: resolve by name (may register a synthetic lazy id), then
            // populate the IC. `resolve_id_by_name` returns None for a target
            // that is untranslatable or reachable only via the interpreter.
            let func_name = std::str::from_utf8(std::slice::from_raw_parts(fn_name_ptr, fn_name_len))
                .unwrap_or("<invalid>");
            match ctx_ref.resolve_id_by_name(func_name) {
                Some(id) => {
                    if !ic_ptr.is_null() {
                        (*ic_ptr).store(id, std::sync::atomic::Ordering::Relaxed);
                    }
                    ctx_ref.resolve_fn_by_id_tiered(id as usize)
                }
                None => None,
            }
        }
    };

    let entry: &FnEntry = match entry_ref {
        Some(e) => e,
        None => {
            // Cross-zpkg lazy-loader fallback: the callee is untranslatable or
            // lives in another zpkg not JIT-compiled into this module, so there
            // is no `FnEntry`. Resolve it via the VM context and run it through
            // the interpreter — mirrors interp `exec_call::call`'s
            // `try_lookup_function` path and `jit_vcall`'s lazy fallback.
            // Without this, a static cross-package call (e.g.
            // `Std.Toml.TomlValue.Parse`) aborts under `--mode jit` while
            // working under interp.
            let func_name = std::str::from_utf8(std::slice::from_raw_parts(fn_name_ptr, fn_name_len))
                .unwrap_or("<invalid>");
            return cross_zpkg_via_interp(
                frame_ref, ctx, dst, func_name, args_ptr, argc, caller_line, caller_col, caller_offset);
        }
    };

    // Fill the callee frame directly from the caller's registers — no
    // intermediate `Vec<Value>` alloc, args cloned once instead of twice.
    let arg_regs = std::slice::from_raw_parts(args_ptr, argc);
    let mut callee_frame = JitFrame::new_args_from(entry.max_reg, &frame_ref.regs, arg_regs);
    let jit_fn: JitFn = std::mem::transmute(entry.ptr);
    let vm_ctx = vm_ctx_ref(ctx);

    // jit-stack-trace + span-column-propagate: stamp caller's site pos + offset.
    vm_ctx.update_top_frame_pos(caller_line, caller_col, caller_offset);
    // 2026-05-10 unify-frame-chain: one push covering GC roots + trace.
    vm_ctx.push_frame(crate::exception::VmFrame::new(
        entry.name.clone(),
        entry.file.clone(),
        &callee_frame.regs as *const _,
        &callee_frame.env_arena as *const _,
    ));
    let result = jit_fn(&mut callee_frame, ctx);
    vm_ctx.pop_frame();
    if result != 0 { callee_frame.recycle(); return 1; }
    frame_ref.regs[dst as usize] = callee_frame.ret.take().unwrap_or(Value::Null);
    callee_frame.recycle();
    0
}

/// Direct-call fallback when the target has no JIT machine-code `FnEntry`.
/// Two cases land here, both mirroring `interp::exec_call::call`'s resolution
/// order:
///   1. the callee lives in the eagerly-merged main `module` but was not
///      JIT-compiled (so it's absent from `fn_entries`) — resolve it via
///      `module.func_index` and run it on the interpreter;
///   2. the callee lives in a dependency zpkg only reachable through the lazy
///      loader (`try_lookup_function`) — load + interp it.
/// Either way the callee runs interpreted and the result is spliced back into
/// the JIT caller's frame. Without case 1, a static cross-package call (e.g.
/// `Std.Toml.TomlValue.Parse`) aborts under `--mode jit` while working under
/// interp, because the lazy loader doesn't own already-merged functions.
unsafe fn cross_zpkg_via_interp(
    frame_ref: &mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, func_name: &str,
    args_ptr: *const u32, argc: usize,
    caller_line: u32, caller_col: u32, caller_offset: u32,
) -> u8 {
    let vm_ctx = vm_ctx_ref(ctx);
    let module = &*(*ctx).module;

    let arg_regs = std::slice::from_raw_parts(args_ptr, argc);
    let args: Vec<Value> = arg_regs.iter().map(|&r| frame_ref.regs[r as usize].clone()).collect();
    // jit-stack-trace: stamp the caller's site + offset before descending.
    vm_ctx.update_top_frame_pos(caller_line, caller_col, caller_offset);

    // Case 1: function present in the merged main module (interp's hot path).
    let outcome = if let Some(callee) = module.func_index.get(func_name)
        .and_then(|&idx| module.functions.get(idx))
    {
        crate::interp::exec_function(vm_ctx, module, callee, &args)
    // Case 2: cross-zpkg target reachable only through the lazy loader.
    } else if let Some(lazy_fn) = vm_ctx.try_lookup_function(func_name) {
        crate::interp::exec_function(vm_ctx, module, lazy_fn.as_ref(), &args)
    } else {
        set_exception(vm_ctx, Value::Str(format!("undefined function `{}`", func_name).into()));
        return 1;
    };

    match outcome {
        Ok(crate::interp::ExecOutcome::Returned(ret)) => {
            frame_ref.regs[dst as usize] = ret.unwrap_or(Value::Null);
            0
        }
        Ok(crate::interp::ExecOutcome::Thrown(val)) => { set_exception(vm_ctx, val); 1 }
        Err(e) => { set_exception(vm_ctx, Value::Str(e.to_string().into())); 1 }
    }
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
    args_ptr: *const u32, argc: usize,
) -> u8 {
    let frame_ref = &mut *frame;
    let arg_regs  = std::slice::from_raw_parts(args_ptr, argc);
    let args: Vec<Value> = arg_regs.iter().map(|&r| frame_ref.regs[r as usize].clone()).collect();

    let id = crate::metadata::tokens::BuiltinId(builtin_id);
    let vm = vm_ctx_ref(ctx);
    match crate::corelib::exec_builtin_by_id(vm, id, &args) {
        Ok(v)  => { frame_ref.regs[dst as usize] = v; 0 }
        Err(e) => {
            // A callback builtin (reflection `MethodInfo.Invoke`) that ran z42
            // code which threw stashed the ORIGINAL exception value — propagate
            // it with its real type, not wrapped into Std.Exception (parity with
            // interp `exec_call::builtin`).
            if let Some(thrown) = vm.take_pending_thrown() {
                set_exception(vm, thrown);
                return 1;
            }
            // make-corelib-errors-catchable parity (this path was interp-only;
            // jit_builtin previously set a raw `Value::Str`). Wrap the builtin
            // error in a `Std.Exception` so JIT-compiled code can catch it with
            // `catch (Exception e)` — a raw string never matches the catch type.
            // Falls back to the raw string if `Std.Exception` isn't loaded.
            let module = &*(*ctx).module;
            let exc = match crate::exception::make_stdlib_exception(
                vm, module, "Std.Exception", e.to_string(),
            ) {
                Ok(exc) => exc,
                Err(_)  => Value::Str(e.to_string().into()),
            };
            set_exception(vm, exc);
            1
        }
    }
}
