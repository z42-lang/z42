//! Activation records and storage shared by all execution engines.
mod entry;
pub(crate) use entry::{invoke_entry, EntryError};

pub mod frames;
pub(super) mod frame_stack;
pub mod reg_pool;
pub mod stack_alloc;
pub(super) mod struct_arena;
pub(super) mod transient_arena;
pub mod outcome;
pub mod forward;
pub mod type_param_env;
pub mod args;
pub mod indirect_return;
pub mod builtin_abi;
pub mod engine_guards;
pub mod builtins;
