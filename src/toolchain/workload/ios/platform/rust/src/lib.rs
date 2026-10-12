//! `z42-platform-ios` — Tier 3 iOS Rust binding.
//!
//! This crate exists for one purpose: produce a `staticlib` that re-exports
//! every `z42_host_*` symbol so Swift can call them through the
//! `Z42VMC` clang module. There is no logic here; all real code lives in
//! [`z42::host`] and [`z42_host`].
//!
//! Spec: docs/spec/archive/2026-05-12-add-platform-ios/

// Re-export the entire host C ABI surface. `#[no_mangle]` on the original
// `extern "C"` declarations means linker symbols stay intact; we just need
// the rlib graph to pull them in.
pub use z42::host::*;
