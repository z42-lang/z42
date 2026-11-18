//! Shared instruction execution semantics.
//!
//! Consumers name the execution contract, not its implementation files:
//! ```
//! use z42::exec::{EngineRegistry, Initialization, TierBackend};
//! let _ = EngineRegistry::new(None);
//! ```
//! ```compile_fail
//! use z42::exec::engine::EngineRegistry;
//! ```
//! ```compile_fail
//! use z42::exec::init::Initialization;
//! ```
//! The activation vocabulary is reached through the exec root re-exports:
//! ```
//! use z42::exec::frames::VmFrame;
//! let _ = std::mem::size_of::<*const VmFrame>();
//! ```
//! ```compile_fail
//! use z42::exec::call::frames::VmFrame;
//! ```
pub mod exception;
pub mod symbol_resolution;
pub mod stack_guard;
pub mod ops;
#[cfg(feature = "native-interop")]
pub mod native;
#[cfg(feature = "native-interop")]
pub mod active_vm;
/// Engine-shared activation and call machinery. The stable vocabulary is
/// re-exported at the exec root (frames / frame_stack / reg_pool / arenas /
/// args / outcome / forwarding / builtin ABI and dispatch); `call` itself and
/// its remaining implementation files (engine_guards) stay crate-internal.
pub(crate) mod call;
pub use call::{
    args, builtin_abi, builtins, forward, frame_stack, frames, indirect_return,
    outcome, reg_pool, stack_alloc, struct_arena, transient_arena, type_param_env,
};
pub mod stack;
mod engine;
pub use engine::{Engine, EngineRegistry, JitConfig, OsrState, TierApi, TierBackend, TierTarget,
    invoke, invoke_args};

mod init;
pub use init::{Initialization, make_type_init_exception};
#[cfg(test)]
pub(crate) use init::{claim as claim_initialization, claim_with as claim_initialization_with};

mod state;
pub use state::ExecState;

mod thread_state;
pub use thread_state::ThreadExecState;
