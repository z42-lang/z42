#![allow(dangerous_implicit_autorefs)]
//! Virtual dispatch (`jit_vcall`): the JIT's **invoke** side.
//!
//! unify-vcall-resolution (2026-09-03): target resolution (boxed primitive / boxed
//! struct / primitive-as-struct / object vtable + hierarchy walk, candidate spellings,
//! PIC install) lives in `interp::vcall_resolve` and is shared with the interpreter —
//! this helper only decides how to run the resolved target: a compiled `FnEntry` when
//! the JIT has one (tiered), otherwise the interpreter on a receiver-filled frame.

use crate::metadata::resolver::VCallIC;
use crate::metadata::{Function, Value};
use crate::interp::vcall_resolve::{resolve_vcall, vcall_ic_hit, VCallTarget};

use super::super::frame::{FnEntry, JitFrame, JitModuleCtx};
use super::{set_exception, vm_ctx_ref, JitFn};

/// `jit_vcall` after formalize-jit-method-token Phase 2.E (2026-05-08):
/// the per-site `VCallIC` is threaded in (stable raw pointer baked into
/// machine code by codegen). IC hit goes straight to
/// `fn_entries_by_id[cached_fn_idx]`; miss runs the shared resolver, which
/// writes the resolved (TypeId, vtable slot, MethodId) triple back to the IC.
///
/// `ic_ptr` may be null when the resolver hasn't run (only happens in
/// tests bypassing `Vm::run`); helper degrades gracefully to slow path.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_vcall(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, obj: u32, method_ptr: *const u8, method_len: usize,
    args_ptr: *const u32, argc: usize,
    ic_ptr: *const VCallIC,
    caller_line: u32,   // 2026-05-10 jit-stack-trace
    caller_col:  u32,   // 2026-05-10 span-column-propagate
    caller_offset: u32, // add-offline-symbolication: linearized code offset
) -> u8 {
    let ctx_ref   = &*ctx;
    let module    = &*ctx_ref.module;
    let frame_ref = &mut *frame;
    let vm_ctx    = vm_ctx_ref(ctx);

    // jit-stack-trace: stamp caller's call-site line + offset once at entry; each
    // invoke path below pushes the callee frame info before running.
    vm_ctx.update_top_frame_pos(caller_line, caller_col, caller_offset);

    let obj_val = frame_ref.regs[obj as usize].clone();
    let arg_regs = std::slice::from_raw_parts(args_ptr, argc);
    let ic: Option<&VCallIC> = if ic_ptr.is_null() { None } else { Some(&*ic_ptr) };

    // ── PIC hit: by-id tiered entry, no name decode (lean-jit-vcall-hit-path) ─────────
    // A cold / untranslatable cached target yields `None` and falls through to the slow
    // path, whose Local arm interps it.
    if let Some(idx) = vcall_ic_hit(ic, &obj_val) {
        if let Some(entry) = ctx_ref.resolve_fn_by_id_tiered(idx) {
            // Move `obj_val` in — this branch always returns.
            return invoke_entry(frame_ref, ctx, dst, entry, obj_val, arg_regs);
        }
    }

    // ── PIC miss: decode the name now (only the slow path needs it), resolve, invoke ──
    let method = std::str::from_utf8(std::slice::from_raw_parts(method_ptr, method_len))
        .unwrap_or("<invalid>");
    let resolved = match resolve_vcall(vm_ctx, module, &obj_val, method, argc, ic) {
        Ok(r) => r,
        Err(e) => { set_exception(vm_ctx, Value::Str(e.to_string().into())); return 1; }
    };
    match resolved.target {
        VCallTarget::Immediate(v) => { frame_ref.regs[dst as usize] = v; 0 }
        // Module-local: compiled (or compiles at the tier threshold) → native; cold /
        // untranslatable (interp-only opcode such as `LoadLocalAddr`) → interp, mirroring
        // `jit_call`'s cross-zpkg-via-interp fallback.
        VCallTarget::Local(idx) => match ctx_ref.resolve_fn_by_id_tiered(idx) {
            Some(entry) => invoke_entry(frame_ref, ctx, dst, entry, resolved.this, arg_regs),
            None => match module.functions.get(idx) {
                Some(f) => invoke_interp(frame_ref, ctx, dst, f, resolved.this, arg_regs),
                None => {
                    set_exception(vm_ctx, Value::Str(
                        format!("VCall: resolved function index {} out of range", idx).into()));
                    1
                }
            },
        },
        // Cross-zpkg / lazily-loaded: give the JIT a chance to register + compile a lazy
        // slot for it (by name, tiered); otherwise interp the loaded function.
        VCallTarget::Lazy(f) => match ctx_ref.resolve_fn_by_name_tiered(&f.name) {
            Some(entry) => invoke_entry(frame_ref, ctx, dst, entry, resolved.this, arg_regs),
            None => invoke_interp(frame_ref, ctx, dst, f.as_ref(), resolved.this, arg_regs),
        },
    }
}

/// Run a compiled entry: receiver in reg 0, args read straight from the caller's
/// registers (no `Vec<Value>`), one `VmFrame` push covering GC roots + stack trace.
unsafe fn invoke_entry(
    frame_ref: &mut JitFrame, ctx: *const JitModuleCtx, dst: u32,
    entry: &FnEntry, this: Value, arg_regs: &[u32],
) -> u8 {
    let mut callee = JitFrame::new_method_args_from(entry.max_reg, this, &frame_ref.regs, arg_regs);
    let jit_fn: JitFn = std::mem::transmute(entry.ptr);
    let vm_ctx = vm_ctx_ref(ctx);
    vm_ctx.push_frame(crate::exception::VmFrame::new(
        entry.name.clone(), entry.file.clone(),
        &callee.regs as *const _, &callee.env_arena as *const _));
    let r = jit_fn(&mut callee, ctx);
    vm_ctx.pop_frame();
    if r != 0 { callee.recycle(); return 1; }
    frame_ref.regs[dst as usize] = callee.ret.take().unwrap_or(Value::Null);
    callee.recycle();
    0
}

/// Run a function on the interpreter with `this` in reg 0 and args filled from the
/// caller's registers (same frame builder the interpreter's own vcall uses).
unsafe fn invoke_interp(
    frame_ref: &mut JitFrame, ctx: *const JitModuleCtx, dst: u32,
    func: &Function, this: Value, arg_regs: &[u32],
) -> u8 {
    let ctx_ref = &*ctx;
    let module  = &*ctx_ref.module;
    let vm_ctx  = vm_ctx_ref(ctx);
    match crate::interp::exec_function_from_receiver_regs(
        vm_ctx, module, func, &this, &frame_ref.regs, arg_regs, &[])
    {
        Ok(crate::interp::ExecOutcome::Returned(ret)) => {
            frame_ref.regs[dst as usize] = ret.unwrap_or(Value::Null); 0
        }
        Ok(crate::interp::ExecOutcome::Thrown(val)) => { set_exception(vm_ctx, val); 1 }
        Err(e) => { set_exception(vm_ctx, Value::Str(e.to_string().into())); 1 }
    }
}
