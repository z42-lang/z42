//! Mutator coordination and the VM side of GC services.
mod gc;
pub(crate) mod roots;
pub mod safepoint;

mod state;
pub(crate) use state::ThreadState;
