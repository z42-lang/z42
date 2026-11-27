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
//!
//! The pause lock and polling countdown are internal to the thread module.
//! ```compile_fail
//! fn phase(ctx: &z42::vm::VmContext) {
//!     let _ = &ctx.core().threads().gc_phase;
//! }
//! ```
//! ```compile_fail
//! fn countdown(ctx: &z42::vm::VmContext) {
//!     let _ = &ctx.threads().safepoint_skip;
//! }
//! ```
//!
//! ```compile_fail
//! use z42::threads::roots::CoreRoots;
//! ```
mod gc;
pub(crate) use gc::{attach_mutator, detach_mutator, wire_gc_pressure};
mod roots;
pub(crate) use roots::install_vm_roots;
pub mod safepoint;

mod state;
pub use state::{ThreadState, ThreadLocalState};
pub(crate) use state::SAFEPOINT_SKIP_OFFSET;

mod monitor;
