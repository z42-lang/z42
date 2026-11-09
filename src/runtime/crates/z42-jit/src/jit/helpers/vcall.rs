#![allow(dangerous_implicit_autorefs)]
//! Virtual dispatch (`jit_vcall`): the JIT's **invoke** side.
//!
//! unify-vcall-resolution : target resolution (boxed primitive / boxed
//! struct / primitive-as-struct / object vtable + hierarchy walk, candidate spellings,
//! PIC install) lives in `exec::ops::vcall` and is shared with the interpreter —
//! this helper only decides how to run the resolved target: a compiled `FnEntry` when
//! the JIT has one (tiered), otherwise the interpreter on a receiver-filled frame.

use crate::metadata::dispatch::ic::VCallIC;
use crate::metadata::Function;
use crate::object::Value;
use crate::exec::ops::vcall::resolve_vcall;
use crate::metadata::dispatch::vcall::vcall_ic_hit;
use crate::exec::ops::vcall::VCallTarget;

use crate::jit::runtime::frame::{FnEntry, JitFrame};
use crate::context::VmContext;
use crate::jit::runtime::invoke::call_entry;
use super::{set_exception, vm_ctx_ref};

/// `jit_vcall` after formalize-jit-method-token Phase 2.E :
/// the per-site `VCallIC` is threaded in (stable raw pointer baked into
/// machine code by codegen). IC hit goes straight to the cached callee's
/// JIT slot by `FnId` (tiered) — module-local and lazily loaded callees alike
/// (P1-2 PR 4); miss runs the shared resolver, which writes the resolved
/// (TypeId, FnId) pair back to the IC.
///
/// `ic_ptr` may be null when the resolver hasn't run (only happens in
/// tests bypassing `host::LoadedProgram::run_entry`); helper degrades gracefully to slow path.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_vcall(
    frame: *mut JitFrame, ctx: *const VmContext,
    dst: u32, obj: u32, method_ptr: *const u8, method_len: usize,
    args_ptr: *const u32, argc: usize,
    ic_ptr: *const VCallIC,
    caller_offset: u32, // packed code offset of the call site (jit-stack-trace)
) -> u8 {
    vcall_impl(frame, ctx, dst, obj, method_ptr, method_len, args_ptr, argc, ic_ptr, caller_offset, &[])
}

/// `jit_vcall` for a method-level generic call site (`e.Accept<R, A>(...)`): `mta_ptr/len`
/// is the site's `method_type_args`. The callee receives the bound arguments
/// in either tier, as with `jit_call_mta`.
/// Resolution and the PIC are the same as the non-generic site: generics are erased,
/// one `FnId` serves every instantiation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_vcall_mta(
    frame: *mut JitFrame, ctx: *const VmContext,
    dst: u32, obj: u32, method_ptr: *const u8, method_len: usize,
    args_ptr: *const u32, argc: usize,
    ic_ptr: *const VCallIC,
    caller_offset: u32,
    mta_ptr: *const String, mta_len: usize,
) -> u8 {
    let mta = std::slice::from_raw_parts(mta_ptr, mta_len);
    // Keep any temporary bound arguments alive for the callee activation.
    let bound;
    let mta = match super::object::bind_forwarded_mta(frame, ctx, mta) {
        Some(v) => { bound = v; &bound[..] }
        None => mta,
    };
    vcall_impl(frame, ctx, dst, obj, method_ptr, method_len, args_ptr, argc, ic_ptr, caller_offset, mta)
}

#[inline(always)]
#[allow(clippy::too_many_arguments)]
unsafe fn vcall_impl(
    frame: *mut JitFrame, ctx: *const VmContext,
    dst: u32, obj: u32, method_ptr: *const u8, method_len: usize,
    args_ptr: *const u32, argc: usize,
    ic_ptr: *const VCallIC,
    caller_offset: u32,
    mta: &[String],
) -> u8 {
    let rt        = super::jit_rt(ctx);
    let module    = super::jit_module(ctx);
    let frame_ref = &mut *frame;
    let vm_ctx    = vm_ctx_ref(ctx);

    // jit-stack-trace: stamp the caller's call-site offset once at entry; each
    // invoke path below pushes the callee frame before running.
    vm_ctx.set_top_frame_pc(caller_offset);

    let obj_val = frame_ref.regs[obj as usize].clone();
    let arg_regs = std::slice::from_raw_parts(args_ptr, argc);
    let ic: Option<&VCallIC> = if ic_ptr.is_null() { None } else { Some(&*ic_ptr) };

    // ── PIC hit: by-id tiered entry, no name decode (lean-jit-vcall-hit-path) ─────────
    // The payload is the callee's `FnId` = its JIT slot id (P1-2 PR 4). Compiled (or
    // compiling at the tier threshold) → native; cold / untranslatable → interp the same
    // function by id (`fn_of`, lock-free) — no re-resolution, and the call is counted
    // once. `fn_of` is `None` only while the id's registration is not yet visible to this
    // thread (see `vcall_ic_hit`); the slow path then binds by name.
    if let Some(id) = vcall_ic_hit(ic, &obj_val) {
        if let Some(func) = rt.fn_of(id) {
            // The check is debug-only; keep its argument (a UTF-8 validation of the
            // method name) out of release builds, where it ran on every IC hit.
            #[cfg(debug_assertions)]
            crate::exec::ops::vcall::assert_pic_target(
                vm_ctx, module, &obj_val,
                std::str::from_utf8(std::slice::from_raw_parts(method_ptr, method_len)).unwrap_or("?"),
                func);
            if let Some(value) = read_resolved_field(vm_ctx, func, &obj_val, arg_regs, mta, ic) {
                frame_ref.regs[dst as usize] = value;
                return 0;
            }
            // Move `obj_val` in — this branch always returns.
            return match rt.resolve_fn_by_id_tiered(&*ctx, id) {
                Some(entry) => invoke_entry(frame_ref, ctx, dst, entry, obj_val, arg_regs, mta),
                None => invoke_interp(frame_ref, ctx, dst, func, obj_val, arg_regs, mta),
            };
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
        // fix-call-arity-skew: signature mismatch under the site's key → catchable throw.
        VCallTarget::Thrown(exc) => { set_exception(vm_ctx, exc); 1 }
        // Module-local or lazily loaded: compiled (or compiles at the tier threshold) →
        // native; cold / untranslatable (interp-only opcode such as `LoadLocalAddr`) / no
        // `FnId` → interp, mirroring `jit_call`'s cross-zpkg-via-interp fallback.
        VCallTarget::Local { func, id } => {
            // Some resolution fallbacks deliberately leave the dispatch PIC empty.
            let reader_ic = ic.filter(|_| id.is_some() && vcall_ic_hit(ic, &obj_val) == id);
            if let Some(value) = read_resolved_field(vm_ctx, func, &resolved.this, arg_regs, mta, reader_ic) {
                frame_ref.regs[dst as usize] = value;
                return 0;
            }
            match id.and_then(|id| rt.resolve_fn_by_id_tiered(&*ctx, id)) {
                Some(entry) => invoke_entry(frame_ref, ctx, dst, entry, resolved.this, arg_regs, mta),
                None => invoke_interp(frame_ref, ctx, dst, func, resolved.this, arg_regs, mta),
            }
        }
    }
}

// The PIC has already selected the target. A field reader needs no JIT tier lookup.
#[inline]
fn read_resolved_field(
    ctx: &VmContext, func: &Function, this: &Value, args: &[u32], mta: &[String], ic: Option<&VCallIC>,
) -> Option<Value> {
    if !args.is_empty() || !mta.is_empty() { return None; }
    crate::exec::call::forward::try_field_read_cached(ctx, func, this, ic.map(|site| &site.field_reader))
}

/// Run a compiled entry: receiver in reg 0, args read straight from the caller's
/// registers (no `Vec<Value>`), one `VmFrame` push covering GC roots + stack trace.
/// Preserve the bound method type arguments for the callee activation.
unsafe fn invoke_entry(
    frame_ref: &mut JitFrame, ctx: *const VmContext, dst: u32,
    entry: &FnEntry, this: Value, arg_regs: &[u32], mta: &[String],
) -> u8 {
    // `[Native]` extern wrapper (e.g. `string.Length`): run its builtin directly (`jit::compile::lookup`).
    if let Some(id) = entry.forward {
        if mta.is_empty() && arg_regs.len() + 1 == unsafe { (*entry.func).param_count } {
            return unsafe { crate::jit::runtime::forward::call_forward(frame_ref, ctx, dst, entry.func, id, Some(this), arg_regs) };
        }
    }
    let vm = vm_ctx_ref(ctx);
    let mut callee = JitFrame::new_method_args_from(vm, entry.max_reg, this, &frame_ref.regs, arg_regs);
    callee.method_type_args = mta;
    call_entry(vm, entry, callee).store_into(&mut frame_ref.regs, dst)
}

/// Run a function on the interpreter with `this` in reg 0 and args filled from the
/// caller's registers (same frame builder the interpreter's own vcall uses); a generic
/// site's `mta` goes into the callee frame's `method_type_args`.
unsafe fn invoke_interp(
    frame_ref: &mut JitFrame, ctx: *const VmContext, dst: u32,
    func: &Function, this: Value, arg_regs: &[u32], mta: &[String],
) -> u8 {
    let module  = super::jit_module(ctx);
    let vm_ctx  = vm_ctx_ref(ctx);
    // A cold `[Native]` wrapper folds too (`exec::call::forward`); not on a generic site.
    if mta.is_empty() && arg_regs.len() + 1 == func.param_count {
        if let Some(id) = crate::exec::call::forward::forward_of(vm_ctx, module, func) {
            return unsafe { crate::jit::runtime::forward::call_forward(frame_ref, ctx, dst, func, id, Some(this), arg_regs) };
        }
    }
    match crate::exec::engine::invoke_args(vm_ctx, module, func, crate::exec::call::args::CallArgs::Receiver { receiver: this, regs: &frame_ref.regs, indices: arg_regs }, mta)
    {
        Ok(crate::exec::call::outcome::ExecOutcome::Returned(ret)) => {
            frame_ref.regs[dst as usize] = ret.unwrap_or(Value::Null); 0
        }
        Ok(crate::exec::call::outcome::ExecOutcome::Thrown(val)) => { set_exception(vm_ctx, val); 1 }
        Err(e) => { set_exception(vm_ctx, Value::Str(e.to_string().into())); 1 }
    }
}
