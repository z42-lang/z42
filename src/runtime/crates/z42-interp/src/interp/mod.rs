//! Register-based bytecode interpreter backend.
//! Execution control, instruction adapters and register frames are owned separately.

pub mod engine;
pub(crate) mod frame;
pub(crate) mod ops;
pub(crate) mod run;

pub use crate::object::value::format::value_to_str;
pub use run::{run, run_returning, run_outcome, init_static_fields, run_with_static_init};

pub mod component;
