//! Shared assembly for JIT tests.
use crate::components::bridge::VmContext;
use crate::metadata::Module;
use super::compile::state::JitRuntime;
use crate::exec::JitConfig;

pub(super) fn install<'a>(vm: &'a VmContext, module: &Module, config: JitConfig) -> anyhow::Result<&'a JitRuntime> {
    if !vm.core().exec().engines.has_baseline() {
        vm.core().exec().engines.register_baseline(&crate::interp::engine::ENGINE)?;
    }
    super::compile::state::install(vm, module, config)
}
