//! Facade assembly: VM contracts do not select or construct concrete backends.
use crate::vm::exec::engine::EngineRegistry;

pub(crate) fn registry() -> EngineRegistry {
    #[cfg(feature = "jit")]
    let tier_run = Some(run_jit as crate::vm::exec::engine::TierRun);
    #[cfg(not(feature = "jit"))]
    let tier_run = None;
    EngineRegistry::new(Some(&crate::interp::engine::ENGINE), tier_run)
}

#[cfg(feature = "jit")]
fn run_jit(ctx: &crate::vm::context::VmContext, module: &crate::vm::runtime::Module, entry: &str) -> anyhow::Result<()> {
    crate::jit::run(ctx, module, entry, crate::config::runtime_config().jit_config())
}
