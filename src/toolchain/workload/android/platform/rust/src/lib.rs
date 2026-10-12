//! `z42-platform-android` — Tier 3 Android Rust binding.
//!
//! Produces `libz42_platform_android.so` (one per ABI via cargo-ndk).
//! The cdylib re-exports every `z42_host_*` symbol so the C JNI bridge
//! (`z42vm/src/main/cpp/z42vm_jni.c`) can dlopen it and call straight
//! through to the runtime's embedding ABI.
//!
//! Spec: docs/spec/archive/2026-05-12-add-platform-android/

// Re-export the entire host C ABI surface. `#[no_mangle]` on the
// original `extern "C"` declarations keeps the linker symbols intact;
// pulling the items into this crate's symbol table is enough.
pub use z42::host::*;
