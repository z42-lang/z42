//! Mutator coordination and the VM side of GC services.
//!
//! The context registry is private to `threads`.
//! ```compile_fail
//! use z42::threads::VmContextPtr;
//! ```
//! ```compile_fail
//! fn registry(threads: &z42::threads::ThreadState) {
//!     let _ = &threads.vm_contexts;
//! }
//! ```
mod gc;
pub mod roots;
pub mod safepoint;

mod state;
pub use state::{ThreadState, ThreadLocalState};

mod monitor;
