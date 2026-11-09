//! Unified runtime library. Optional execution backends live in independent crates.
extern crate self as z42;
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

// The runtime owns allocation for both its Rust dylib consumers and static hosts.
// Backends must use this allocator rather than installing a separate allocator.
#[cfg(all(not(test), feature = "mimalloc-alloc", not(feature = "dhat-heap")))]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;
#[cfg(all(not(test), feature = "dhat-heap"))]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;
