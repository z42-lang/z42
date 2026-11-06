//! Optimized-tier registration. Only this backend constructs native activations.
use anyhow::Result;
use crate::vm::exec::call::{args::CallArgs, outcome::ExecOutcome};
use crate::vm::exec::engine::{OsrState, TierBackend, TierTarget};
use crate::vm::runtime::Function;
use crate::vm_context::VmContext;
use super::{frame::JitFrame, runtime::JitRuntime};

impl TierBackend for JitRuntime {
    fn osr_threshold(&self) -> u32 { self.osr_threshold }
    fn invoke(&self, ctx: &VmContext, target: TierTarget<'_>, args: CallArgs<'_>, method_type_args: &[String])
        -> Option<Result<ExecOutcome>> {
        let entry = match target {
            TierTarget::Site(id) => self.resolve_fn_by_id_tiered(ctx, id)?,
            TierTarget::Existing(func) => {
                let dthr = self.interp_tierup;
                let id = self.id_of_func(func)?;
                if dthr == 0 { self.peek_fn_by_id(id)? }
                else { self.resolve_fn_by_id_thr(ctx, id, dthr)? }
            }
        };
        let (max_reg, ptr, callee_fn) = (entry.max_reg, entry.ptr, entry.func);
        ctx.counters().jit_native_from_interp.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut callee = match args {
            CallArgs::Values(values) => JitFrame::new(ctx, max_reg, values),
            CallArgs::Registers { regs, indices } => JitFrame::new_args_from(ctx, max_reg, regs, indices),
            CallArgs::Receiver { receiver, regs, indices } =>
                JitFrame::new_method_args_from(ctx, max_reg, receiver, regs, indices),
        };
        callee.method_type_args = method_type_args;
        let outcome = unsafe { super::invoke::enter_compiled(ctx, ptr, callee_fn, callee) };
        Some(Ok(outcome.into_exec(ctx)))
    }
    #[cold]
    #[inline(never)]
    fn osr(&self, ctx: &VmContext, func: &Function, header: usize, state: OsrState<'_>)
        -> Option<Result<ExecOutcome>> {
        if state.has_ref_writebacks { return None; }
        let id = self.id_of_func(func)?;
        let entry = self.resolve_osr_entry(ctx, id, header)?;
        ctx.counters().jit_native_from_interp.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut osr = JitFrame::from_interp_regs(ctx, state.regs, entry.max_reg);
        osr.frame_id = state.frame_id;
        osr.method_type_args = state.method_type_args;
        // Keep both existing VmFrame records: the interpreter guard still owns
        // its roots until this native continuation returns to that caller.
        let outcome = unsafe { super::invoke::enter_compiled(ctx, entry.ptr, entry.func, osr) };
        Some(Ok(outcome.into_exec(ctx)))
    }
}
