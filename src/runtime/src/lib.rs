//! Unified runtime library. Optional execution backends live in independent crates.
extern crate self as z42;
extern crate self as z42_runtime;
pub use z42_zpkg as zpkg;
pub mod vm;
pub mod object;
pub mod gc;
pub mod pal;
pub mod corelib;
pub mod profiler;
pub mod interop;
pub mod host;
pub mod config;
pub(crate) mod assembly;
pub use vm::exec::ops::object as objops;
mod identity;
pub use identity::{core_identity, CORE_ABI};
pub use vm::components;
#[cfg(test)]
#[path = "../crates/z42-interp/src/interp/mod.rs"]
pub mod interp;
#[cfg(all(test, feature = "jit", z42_jit_supported))]
#[path = "../crates/z42-jit/src/jit/mod.rs"]
pub mod jit;
