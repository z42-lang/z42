//! Execution-owned shared VM state; concrete backends are selected by the facade.
use super::engine::EngineRegistry;

pub(crate) struct ExecState {
    pub(crate) engines: EngineRegistry,
    pub(crate) builtins: &'static [super::call::builtin_abi::BuiltinDesc],
    pub(crate) object_protocol: super::object_protocol::ObjectProtocol,
}

impl ExecState {
    pub(crate) fn new(
        engines: EngineRegistry,
        builtins: &'static [super::call::builtin_abi::BuiltinDesc],
        object_protocol: super::object_protocol::ObjectProtocol,
    ) -> Self {
        Self { engines, builtins, object_protocol }
    }
}
