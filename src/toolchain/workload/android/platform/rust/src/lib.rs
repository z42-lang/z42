//! `z42-platform-android` — Tier 3 Android Rust binding.
//!
//! Produces `libz42_platform_android.so` (one per ABI via cargo-ndk).
//! The cdylib re-exports every `z42_host_*` symbol so the C JNI bridge
//! (`z42vm/src/main/cpp/z42vm_jni.c`) can dlopen it and call straight
//! through to the runtime's embedding ABI.
//!
//! Spec: 变更 add-platform-android/

// Re-export the entire host C ABI surface. `#[no_mangle]` on the
// original `extern "C"` declarations keeps the linker symbols intact;
// pulling the items into this crate's symbol table is enough.
pub use z42::host::*;

/// Register this embedding library's statically linked interpreter before using host APIs.
#[no_mangle]
pub extern "C" fn z42_linked_components_init() -> i32 {
    match unsafe { z42::host::register_static_component(z42::host::INTERP_COMPONENT, z42_interp::component::z42_interp_init) } {
        Ok(()) => 0,
        Err(_) => 1,
    }
}
