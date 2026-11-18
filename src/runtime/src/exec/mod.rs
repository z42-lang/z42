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
pub mod exception;
pub mod symbol_resolution;
pub mod stack_guard;
pub mod ops;
#[cfg(feature = "native-interop")]
pub mod native;
#[cfg(feature = "native-interop")]
pub mod active_vm;
pub mod call;
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
