//! Unified runtime library. Optional execution backends live in independent crates.
//!
//! The executable uses a process C entry; CLI Rust implementation stays private:
//! ```compile_fail
//! use z42::host::cli::run_from;
//! ```
//! ```compile_fail
//! use z42::cli::run_from;
//! ```
//! ```compile_fail
//! use z42::host::entry_api::RuntimeEntry;
//! ```
extern crate self as z42;
pub use z42_zpkg as zpkg;
pub mod vm;
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
pub mod config;
pub use exec::ops::object as objops;
/// Same-build component contracts; VM management remains crate-private.
#[doc(hidden)]
pub mod components;
#[cfg(test)]
#[path = "../crates/z42-interp/src/interp/mod.rs"]
pub mod interp;

#[cfg(all(feature = "shared-core", any(feature = "mimalloc-alloc", feature = "dhat-heap")))]
compile_error!("shared-core runtime packs require the system allocator; custom allocators belong to a final static host");


// Final Core linkage composition. Entries are borrowed only on first module use;
// recording a function pointer must not run the module's descriptor or VM callbacks.
pub(crate) const LINKED_NATIVE_MODULES: &[(&str, z42_abi::module::ModuleEntry)] = &[
    ("z42_repl", z42_repl::z42_repl_module_v1),
    #[cfg(feature = "bundled-compression")]
    ("z42_compression", z42_compression::z42_compression_module_v1),
];

#[cfg(feature = "cli")]
mod main_startup;

/// Process entry used by the dynamically loaded executable. Reads the original
/// OS argument vector; no Rust values or allocations cross this C boundary.
#[cfg(feature = "cli")]
#[no_mangle]
extern "C" fn z42_main_v1() -> i32 {
    match std::panic::catch_unwind(|| main_startup::run_from(std::env::args_os())) {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => { eprintln!("Error: {error:?}"); 1 }
        Err(_) => { eprintln!("z42vm: runtime panicked"); 1 }
    }
}
