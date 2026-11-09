//! One physical VM core for a same-build dynamic runtime pack.
/// Force every dynamic participant to depend on this shared image.
pub fn link_anchor() -> *const core::ffi::c_void { z42_runtime_core::core_identity() }
