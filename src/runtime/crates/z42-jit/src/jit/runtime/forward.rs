//! Execute native-wrapper short circuits from compiled callers.
use crate::metadata::Function;
use crate::object::Value;
use crate::context::VmContext;
use super::frame::JitFrame;
use crate::jit::helpers::vm_ctx_ref;

/// Run the folded call of `wrapper` (fold `token`, `exec::call::forward::forward_of`) with `this` (vcall receiver, if any)
/// followed by the caller's `arg_regs`, storing the result into `regs[dst]` (`Null`
/// for a void builtin — what the wrapper's `ret` would have produced). Returns the JIT
/// helper status: 0 = ok, 1 = exception pending.
///
/// # Safety
/// `frame` / `ctx` as for every JIT helper; `arg_regs` index `frame.regs`; `wrapper`
/// is the forwarder (valid for the run, as every `FnEntry::func`).
pub(crate) unsafe fn call_forward(
    frame: &mut JitFrame, ctx: *const VmContext, dst: u32, wrapper: *const Function, token: u32,
    this: Option<Value>, arg_regs: &[u32],
) -> u8 {
    let vm = unsafe { vm_ctx_ref(ctx) };
    let wrapper = unsafe { &*wrapper };
    let module = || unsafe { crate::jit::helpers::jit_module(ctx) };
    match crate::exec::call::forward::run_folded(vm, wrapper, token, this, &frame.regs, arg_regs, module) {
        Ok(Some(v)) => { frame.regs[dst as usize] = v; 0 }
        Ok(None) => { frame.regs[dst as usize] = Value::Null; 0 }
        Err(f) => crate::jit::helpers::call::raised_into_exception(vm, f.raise(vm, module(), wrapper)),
    }
}
