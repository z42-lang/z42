//! Shared instruction execution semantics.
pub mod ops;
pub mod ambient;
#[cfg(feature = "native-interop")]
pub mod active_vm;
pub mod call;
pub mod stack;
pub(crate) mod engine;
pub(crate) mod object_protocol;

pub(crate) mod init;

mod state;
pub(crate) use state::ExecState;

mod thread_state;
pub(crate) use thread_state::ThreadExecState;
