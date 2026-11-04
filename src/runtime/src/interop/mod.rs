//! Interop-owned per-VM buffers and optional Tier 1 native ABI support.
//! Buffer pinning is available on every target. External type registration,
//! libffi dispatch and dynamic loading require `native-interop`.
mod state;
pub use state::{pin_owned_buffer, release_owned_buffer, pinned_owned_buffer_count};
#[cfg(feature = "native-interop")]
pub use state::{register_native_type, resolve_native_type, native_type_count};

#[cfg(feature = "native-interop")]
pub mod dispatch;
#[cfg(feature = "native-interop")]
pub mod error;
#[cfg(feature = "native-interop")]
pub mod exports;
#[cfg(feature = "native-interop")]
pub mod ext;
#[cfg(feature = "native-interop")]
pub mod loader;
#[cfg(feature = "native-interop")]
pub mod marshal;
#[cfg(feature = "native-interop")]
pub mod registry;

#[cfg(feature = "native-interop")]
pub use registry::{MethodEntry, RegisteredType};
#[cfg(all(test, feature = "native-interop"))]
mod registry_tests;
#[cfg(all(test, feature = "native-interop"))]
mod marshal_tests;
#[cfg(all(test, feature = "native-interop"))]
mod dispatch_tests;
#[cfg(all(test, feature = "native-interop"))]
mod ext_tests;
#[cfg(test)]
mod state_tests;

pub(crate) fn state_for_vm() -> state::State { state::State::default() }
