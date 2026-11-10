//! z42 Tier 2 ergonomic Rust API for native interop.
//!
//! Sits on top of [`z42_abi`] and exposes user-friendly types and traits that
//! Rust crate authors implement to expose native types to z42 user code.
//! [`module`] provides module call/value helpers, error handling and synchronous
//! script callbacks. It contains no VM registry or module-specific implementation.
//!
//! # Quick start
//!
//! ```ignore
//! use z42_rs::prelude::*;
//!
//! #[derive(Default)]
//! pub struct Counter { value: i64 }
//!
//! #[z42::methods(module = "demo", name = "Counter")]
//! impl Counter {
//!     pub fn inc(&mut self) -> i64 { self.value += 1; self.value }
//!     pub fn get(&self) -> i64 { self.value }
//! }
//!
//! z42::module! { name: "demo", types: [Counter] }
//! ```
//!
//! After expansion the module exposes a `demo_register()` symbol the VM
//! invokes to register `Counter` via Tier 1 `z42_register_type`.
//!
//! Note: `z42-rs` requires `std`. The lower-level `z42-abi` mirror crate
//! stays `no_std` for embedded users.

#[cfg(feature = "types")]
pub mod native_helpers;
pub mod module;
#[cfg(feature = "types")]
pub mod traits;
#[cfg(feature = "types")]
pub mod types;

/// One-stop import for users implementing native types.
#[cfg(feature = "types")]
pub mod prelude {
    pub use crate::traits::{Visitor, Z42Traceable, Z42Type};
    pub use crate::types::{Descriptor, Z42Args, Z42Error, Z42TypeRef, Z42Value};
    pub use ::z42_macros::{methods, module, trait_impl, Z42Type as DeriveZ42Type};
}

// Re-export macros at crate root so users can `#[z42_rs::methods(...)]` /
// `#[derive(z42_rs::Z42Type)]` without the prelude import.
#[cfg(feature = "types")]
pub use ::z42_macros::{methods, module, trait_impl, Z42Type};

pub use z42_abi;
