//! VM-owned linked definitions, identities and execution caches.
//!
//! Registry assembly and reflection payloads are crate-internal; the identity,
//! dispatch, resolution, statics and builtin catalogs are the stable surface.
//! ```compile_fail
//! use z42::metadata::types;
//! ```
pub mod ids;
pub(crate) mod types;
pub mod resolve;
pub mod dispatch;
mod function;
mod module;
mod sites;
pub use function::{Function, OwnerInitCell};
pub use module::Module;

mod fusion;
pub use fusion::{CmpOp, SuperInstr};

#[cfg(all(test, z42_test_facade))]
mod runtime_tests;

pub(crate) mod reflection_types;
mod state;
pub use state::{RuntimeState, ThreadRuntimeState};

pub mod statics;
pub mod builtins;
