//! Shared assembly for JIT tests.
use crate::components::bridge::VmContext;
use crate::metadata::{Function, Module};
use crate::object::Value;
use super::compile::state::JitRuntime;
use crate::exec::JitConfig;
use crate::exec::outcome::ExecOutcome;

pub(super) fn install<'a>(vm: &'a VmContext, module: &Module, config: JitConfig) -> anyhow::Result<&'a JitRuntime> {
    ensure_baseline(vm)?;
    super::compile::state::install(vm, module, config)
}

pub(super) use z42_test_engines::ensure_baseline;

pub(super) fn invoke_baseline(
    vm: &VmContext,
    module: &Module,
    function: &Function,
    args: &[Value],
) -> anyhow::Result<ExecOutcome> {
    ensure_baseline(vm)?;
    crate::exec::invoke(vm, module, function, args)
}
