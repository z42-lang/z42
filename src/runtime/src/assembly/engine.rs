//! Facade assembly: VM contracts do not select or construct concrete backends.
use crate::vm::exec::engine::EngineRegistry;

pub(crate) fn registry() -> EngineRegistry {
    #[cfg(test)]
    let baseline = Some(&crate::interp::engine::ENGINE as &'static dyn crate::vm::exec::engine::Engine);
    #[cfg(not(test))]
    let baseline = None;
    let registry = EngineRegistry::new(baseline);
    #[cfg(all(test, feature = "jit", z42_jit_supported))]
    { let _ = registry.register_jit(crate::jit::execution_engine(), crate::config::runtime_config().jit_config()); }
    registry
}
