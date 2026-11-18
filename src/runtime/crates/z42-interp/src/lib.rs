//! Independent interp execution backend, sharing the runtime core.
use z42::{components, exec, metadata, diagnostics, threads, object, zpkg};
pub mod interp;
pub use interp::*;
