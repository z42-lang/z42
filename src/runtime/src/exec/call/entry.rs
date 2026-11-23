//! Execute a program entry at the managed call boundary.
use super::outcome::ExecOutcome;
use crate::{metadata::Module, object::Value, vm::VmContext, zpkg::ExecMode};
use anyhow::{anyhow, bail, Result};

pub(crate) fn invoke_entry(
    ctx: &VmContext,
    module: &Module,
    mode: ExecMode,
    index: usize,
    args: &[Value],
    prepare: impl FnOnce(),
) -> Result<Option<Value>> {
    let func = module
        .functions
        .get(index)
        .ok_or_else(|| anyhow!("entry function index out of bounds"))?;
    if args.len() != func.param_count {
        bail!(
            "arg-count-mismatch: function `{}` expects {} arg(s), got {}",
            func.name,
            func.param_count,
            args.len()
        );
    }
    #[cfg(feature = "native-interop")]
    let _vm_guard = crate::exec::active_vm::VmGuard::enter(ctx);
    let _heap_guard = crate::gc::ambient::HeapGuard::enter(ctx.heap());
    struct Roots<'a>(&'a crate::gc::Heap, Vec<crate::gc::RootHandle>);
    impl Drop for Roots<'_> {
        fn drop(&mut self) {
            for root in self.1.drain(..) {
                self.0.unpin_root(root);
            }
        }
    }
    let _roots = Roots(
        ctx.heap(),
        args.iter().map(|v| ctx.heap().pin_root(*v)).collect(),
    );
    prepare();
    let outcome = ctx
        .core()
        .exec()
        .engines
        .invoke_mode(mode, ctx, module, func, args);
    if crate::exec::stack_guard::is_fatal(ctx) {
        bail!(
            "{}",
            crate::exec::stack_guard::report().unwrap_or_else(|| format!(
                "{}stack overflow",
                crate::exec::stack_guard::FATAL_PREFIX
            ))
        );
    }
    let outcome = outcome?;
    match outcome {
        ExecOutcome::Returned(value) => Ok(value),
        ExecOutcome::Thrown(value) => bail!(
            "{}",
            crate::exec::exception::format_uncaught(&value, ctx, module)
        ),
    }
}
