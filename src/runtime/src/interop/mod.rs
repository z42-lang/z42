//! Interop-owned per-VM buffers and optional Tier 1 native ABI support.
//! Buffer pinning is available on every target. External type registration,
//! libffi dispatch and dynamic loading require `native-interop`.
mod state;
pub use state::{pin_owned_buffer, release_owned_buffer, pinned_owned_buffer_count};
#[cfg(feature = "native-interop")]
pub use state::{register_native_type, resolve_native_type, native_type_count};

#[cfg(feature = "native-interop")]
pub mod ext;
#[cfg(feature = "native-interop")]
mod tier1;

#[cfg(feature = "native-interop")]
pub use tier1::{dispatch, error, exports, loader, marshal, registry};
#[cfg(feature = "native-interop")]
pub use registry::{MethodEntry, RegisteredType};
#[cfg(feature = "native-interop")]
pub(crate) use tier1::call::call_registered;
#[cfg(test)]
mod state_tests;

pub(crate) fn state_for_vm() -> state::State { state::State::default() }
