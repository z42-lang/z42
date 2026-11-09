//! Shared instruction execution semantics.
pub mod exception;
pub mod stack_guard;
pub mod ops;
pub mod ambient;
#[cfg(feature = "native-interop")]
pub mod native;
#[cfg(feature = "native-interop")]
pub mod active_vm;
pub mod call;
pub mod stack;
pub mod engine;
pub mod object_protocol;

pub mod init;

mod state;
pub use state::ExecState;

mod thread_state;
pub use thread_state::ThreadExecState;
