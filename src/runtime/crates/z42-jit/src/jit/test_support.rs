//! Shared assembly for JIT tests.
use crate::components::bridge::{register_baseline_engine, test_engine_registered, VmContext};
use crate::metadata::Module;
use super::compile::state::JitRuntime;
use crate::exec::JitConfig;

pub(super) fn install<'a>(vm: &'a VmContext, module: &Module, config: JitConfig) -> anyhow::Result<&'a JitRuntime> {
    ensure_baseline(vm)?;
    super::compile::state::install(vm, module, config)
}

pub(super) fn ensure_baseline(vm: &VmContext) -> anyhow::Result<()> {
    if !test_engine_registered(vm, z42::zpkg::ExecMode::Interp) {
        // SAFETY: the test links the interpreter into the process.
        unsafe { register_baseline_engine(vm, &crate::interp::engine::ENGINE)? };
    }
    Ok(())
}
