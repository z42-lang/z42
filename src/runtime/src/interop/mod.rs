//! Interop runtime — Tier 1 C ABI implementation (calling external C libraries).
//!
//! See `docs/design/language/interop.md` and spec C2 (`impl-tier1-c-abi`). This
//! module wires together:
//!
//! - [`registry`] — `RegisteredType` + per-VM type table
//! - [`marshal`] — `Z42Value` ↔ `Value` conversion (blittable subset)
//! - [`dispatch`] — libffi cif construction + `extern "C"` invocation
//! - [`loader`] — `dlopen` + library-handle lifetime management
//! - [`error`] — thread-local last-error slot for `z42_last_error()`
//! - [`exports`] — `#[unsafe(no_mangle)]` `z42_*` ABI entry points
//!
//! Only `CallNative` (0x53) is wired to real dispatch in this spec; the
//! other three C1-scaffold opcodes (`CallNativeVtable`, `PinPtr`,
//! `UnpinPtr`) keep their trap behaviour until specs C4/C5 land.

pub mod dispatch;
pub mod error;
pub mod exports;
pub mod ext;
pub mod loader;
pub mod marshal;
pub mod registry;

#[cfg(test)]
mod registry_tests;
#[cfg(test)]
mod marshal_tests;
#[cfg(test)]
mod dispatch_tests;
#[cfg(test)]
mod ext_tests;

pub use registry::{MethodEntry, RegisteredType};
