//! `z42-platform-ios` — Tier 3 iOS Rust binding.
//!
//! This crate exists for one purpose: produce a `staticlib` that re-exports
//! every `z42_host_*` symbol so Swift can call them through the
//! `Z42VMC` clang module. There is no logic here; all real code lives in
//! [`z42::host`] and [`z42_host`].
//!
//! Spec: 变更 add-platform-ios/

// Re-export the entire host C ABI surface. `#[no_mangle]` on the original
// `extern "C"` declarations means linker symbols stay intact; we just need
// the rlib graph to pull them in.
pub use z42::host::*;

/// Register this embedding library's statically linked interpreter before using host APIs.
#[no_mangle]
pub extern "C" fn z42_linked_components_init() -> i32 {
    match unsafe { z42::host::register_static_component(z42::host::INTERP_COMPONENT, z42_interp::component::z42_interp_init) } {
        Ok(()) => 0,
        Err(_) => 1,
    }
}
