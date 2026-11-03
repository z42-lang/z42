//! Application-owned arguments, shared with core-library consumers through a narrow service.
use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use std::sync::Arc;
use crate::vm_context::VmContext;

#[derive(Default)]
pub(crate) struct State {
    args: Mutex<Vec<String>>,
}

impl crate::corelib::fs::ProgramArgs for State {
    fn program_args(&self) -> Vec<String> { self.args.lock().clone() }
}

pub(crate) fn state_for_vm() -> Arc<State> { Arc::new(State::default()) }

fn state(ctx: &VmContext) -> Result<&State> {
    ctx.ext::<Arc<State>>().map(Arc::as_ref)
        .ok_or_else(|| anyhow!("application arguments are not installed for this VM"))
}

/// Replace this VM application's arguments. Workers sharing its core observe the same data.
pub fn set_program_args(ctx: &VmContext, args: Vec<String>) -> Result<()> {
    *state(ctx)?.args.lock() = args;
    Ok(())
}

/// Take an owned snapshot of this VM application's arguments.
pub fn program_args(ctx: &VmContext) -> Result<Vec<String>> {
    Ok(crate::corelib::fs::ProgramArgs::program_args(state(ctx)?))
}

#[cfg(test)]
#[path = "app_state_tests.rs"]
mod app_state_tests;
