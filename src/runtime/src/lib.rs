//! Unified runtime library. Optional execution backends live in independent crates.
extern crate self as z42;
pub use z42_zpkg as zpkg;
pub mod context;
pub mod exec;
pub mod loader;
pub mod metadata;
pub mod diagnostics;
pub mod threads;
pub mod object;
pub mod gc;
pub mod pal;
pub mod corelib;
#[cfg(test)]
#[path = "../crates/z42-profiler/src/profiler/mod.rs"]
pub mod profiler;
pub mod interop;
pub mod host;
/// CLI host adapter, excluded from non-CLI runtime builds.
///
/// The ordinary host surface does not export command-line implementations:
/// ```compile_fail
/// use z42::host::cli::run_from;
/// ```
/// ```compile_fail
/// use z42::host::entry_api::RuntimeEntry;
/// ```
#[cfg(feature = "cli")]
pub mod cli;
pub mod config;
pub use exec::ops::object as objops;
/// Same-build component contracts; VM management remains crate-private.
#[doc(hidden)]
pub mod components;
#[cfg(test)]
#[path = "../crates/z42-interp/src/interp/mod.rs"]
pub mod interp;
#[cfg(all(test, feature = "jit", z42_jit_supported))]
#[path = "../crates/z42-jit/src/jit/mod.rs"]
pub mod jit;

#[cfg(all(feature = "shared-core", any(feature = "mimalloc-alloc", feature = "dhat-heap")))]
compile_error!("shared-core runtime packs require the system allocator; custom allocators belong to a final static host");
