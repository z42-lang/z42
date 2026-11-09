//! VM-owned linked definitions, identities and execution caches.
pub mod ids;
pub mod types;
pub mod resolve;
pub mod dispatch;
mod function;
mod module;
mod sites;
pub use function::{Function, OwnerInitCell};
pub use module::Module;

mod fusion;
pub use fusion::{CmpOp, SuperInstr};

#[cfg(test)]
mod runtime_tests;

pub mod reflection_types;
mod state;
pub use state::{RuntimeState, ThreadRuntimeState};

pub mod statics;
pub mod builtins;
