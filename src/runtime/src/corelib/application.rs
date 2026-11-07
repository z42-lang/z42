//! Application policy supplied by the facade, without retaining a VM or managed values.
use std::{path::PathBuf, sync::Arc};
use crate::{pal::fs::Vfs, vm::context::VmContext};
use crate::vm::exec::call::builtin_abi::BuiltinError;

pub(crate) type RunIsolated = fn(&str, Option<&str>, Option<PathBuf>, &Arc<dyn Vfs>) -> anyhow::Result<()>;

#[derive(Clone, Copy)]
pub(crate) struct Services {
    pub(crate) run_isolated: RunIsolated,
    pub(crate) report_on_exit: fn(&VmContext),
}

impl Services {
    /// Reporting must precede destructor-free process termination.
    pub(crate) fn exit(self, ctx: &VmContext, code: i32) -> ! {
        (self.report_on_exit)(ctx);
        std::process::exit(code)
    }
}

pub(crate) fn state(ctx: &VmContext) -> Result<&Services, BuiltinError> {
    super::resource_registry::state(ctx)
}

#[cfg(test)]
#[path = "application_tests.rs"]
mod tests;
