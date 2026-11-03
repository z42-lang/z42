//! Shared instruction execution semantics.
pub mod ops;
pub mod ambient;
pub mod call;
pub mod stack;
pub(crate) mod engine;

pub(crate) mod init;

mod state;
pub(crate) use state::ExecState;
