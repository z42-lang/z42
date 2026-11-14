//! Independent interp execution backend, sharing the runtime core.
use z42::{context, exec, metadata, diagnostics, threads, object, objops, zpkg};
pub mod interp;
pub use interp::*;
