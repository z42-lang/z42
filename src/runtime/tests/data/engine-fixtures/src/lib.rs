//! Runtime test assembly of a linked baseline provider.
use z42::components::{
    abi::INTERP_COMPONENT,
    bridge::{test_bind_execution, test_engine_registered, VmContext},
};
use z42::zpkg::ExecMode;

pub fn ensure_baseline(vm: &VmContext) -> anyhow::Result<()> {
    if test_engine_registered(vm, ExecMode::Interp) {
        return Ok(());
    }
    // SAFETY: the interpreter factory and its engine are linked into the test binary.
    unsafe { test_bind_execution(vm, (INTERP_COMPONENT, z42_interp::component::z42_interp_init), None) }
}
