//! Activation records and storage shared by all execution engines.
pub mod frames;
pub(crate) mod frame_stack;
pub(crate) mod reg_pool;
pub(crate) mod stack_alloc;
pub(crate) mod struct_arena;
pub(crate) mod transient_arena;
pub mod outcome;
pub(crate) mod forward;
pub(crate) mod type_param_env;
pub(crate) mod args;
pub(crate) mod indirect_return;
pub mod builtin_abi;
pub(crate) mod engine_guards;
pub mod builtins;
