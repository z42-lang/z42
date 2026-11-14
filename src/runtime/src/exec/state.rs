//! Execution-owned shared VM state; concrete backends are selected by the facade.
use super::engine::EngineRegistry;

pub struct ExecState {
    pub stack_alloc: super::call::stack_alloc::StackAllocPolicy,
    pub engines: EngineRegistry,
    pub builtins: &'static [super::call::builtin_abi::BuiltinDesc],
}

impl ExecState {
    pub fn new(
        engines: EngineRegistry,
        builtins: &'static [super::call::builtin_abi::BuiltinDesc],
        stack_alloc: super::call::stack_alloc::StackAllocPolicy,
    ) -> Self {
        Self { engines, builtins, stack_alloc }
    }
}
