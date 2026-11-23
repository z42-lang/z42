//! Execute a program entry at the managed call boundary.
use super::outcome::ExecOutcome;
use crate::{metadata::Module, object::Value, vm::VmContext, zpkg::ExecMode};
use anyhow::{Error, anyhow};

pub(crate) enum EntryError {
    ArgMismatch(String),
    Uncaught(String),
    Fatal(String),
    Internal(Error),
}

impl EntryError {
    pub(crate) fn into_diagnostic(self) -> Error {
        match self {
            Self::ArgMismatch(message) | Self::Uncaught(message) | Self::Fatal(message) => {
                Error::msg(message)
            }
            Self::Internal(error) => error,
        }
    }
}

pub(crate) fn invoke_entry(
    ctx: &VmContext,
    module: &Module,
    mode: ExecMode,
    index: usize,
    args: &[Value],
    prepare: impl FnOnce(),
) -> Result<Option<Value>, EntryError> {
    let func = module
        .functions
        .get(index)
        .ok_or_else(|| EntryError::Internal(anyhow!("entry function index out of bounds")))?;
    if args.len() != func.param_count {
        return Err(EntryError::ArgMismatch(format!(
            "arg-count-mismatch: function `{}` expects {} arg(s), got {}",
            func.name,
            func.param_count,
            args.len()
        )));
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
        let report = crate::exec::stack_guard::report()
            .unwrap_or_else(|| format!("{}stack overflow", crate::exec::stack_guard::FATAL_PREFIX));
        return Err(EntryError::Fatal(report));
    }
    let outcome = outcome.map_err(EntryError::Internal)?;
    match outcome {
        ExecOutcome::Returned(value) => Ok(value),
        ExecOutcome::Thrown(value) => Err(EntryError::Uncaught(
            crate::exec::exception::format_uncaught(&value, ctx, module),
        )),
    }
}
