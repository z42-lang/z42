//! Execution-owned shared VM state; concrete backends are selected by the facade.
use super::engine::EngineRegistry;

pub(crate) struct ExecState {
    pub(crate) engines: EngineRegistry,
}

impl ExecState {
    pub(crate) fn new(engines: EngineRegistry) -> Self { Self { engines } }
}
