//! VM-owned linked definitions, identities and execution caches.
mod function;
mod module;
mod sites;
pub use function::{Function, OwnerInitCell};
pub use module::Module;

mod fusion;
pub use fusion::{CmpOp, SuperInstr};

#[cfg(test)]
mod runtime_tests;
