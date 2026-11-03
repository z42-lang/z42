//! Facade assembly: VM contracts do not select or construct concrete backends.
use crate::vm::exec::engine::EngineRegistry;

pub(crate) fn registry() -> EngineRegistry {
    #[cfg(feature = "jit")]
    let tier_run = Some(crate::jit::run as crate::vm::exec::engine::TierRun);
    #[cfg(not(feature = "jit"))]
    let tier_run = None;
    EngineRegistry::new(Some(&crate::interp::engine::ENGINE), tier_run)
}
