//! Independent jit execution backend, sharing the runtime core.
pub use z42::{context, exec, loader, metadata, diagnostics, threads, object, gc, pal, objops, zpkg};
#[cfg(all(feature = "jit", any(target_arch = "x86_64", target_arch = "aarch64"), any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub mod jit;
#[cfg(all(feature = "jit", any(target_arch = "x86_64", target_arch = "aarch64"), any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub use jit::*;

pub use z42::components::{core_identity, CORE_ABI};

pub use z42::components::api;
