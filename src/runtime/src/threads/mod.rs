//! Mutator coordination and the VM side of GC services.
mod gc;
pub mod roots;
pub mod safepoint;

mod state;
pub use state::{ThreadState, ThreadLocalState};

mod monitor;
