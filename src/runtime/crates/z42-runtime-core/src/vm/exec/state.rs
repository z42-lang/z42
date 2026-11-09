//! Execution-owned shared VM state; concrete backends are selected by the facade.
use super::engine::EngineRegistry;

pub struct ExecState {
    pub stack_alloc: super::call::stack_alloc::StackAllocPolicy,
    pub engines: EngineRegistry,
    pub builtins: &'static [super::call::builtin_abi::BuiltinDesc],
    pub object_protocol: super::object_protocol::ObjectProtocol,
}

impl ExecState {
    pub fn new(
        engines: EngineRegistry,
        builtins: &'static [super::call::builtin_abi::BuiltinDesc],
        object_protocol: super::object_protocol::ObjectProtocol,
        stack_alloc: super::call::stack_alloc::StackAllocPolicy,
    ) -> Self {
        Self { engines, builtins, object_protocol, stack_alloc }
    }
}
