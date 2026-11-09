//! Independent jit execution backend, sharing the runtime core.
pub use z42_runtime_core::{vm, object, gc, pal, objops, zpkg};
#[cfg(all(any(target_arch = "x86_64", target_arch = "aarch64"), any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub mod jit;
#[cfg(all(any(target_arch = "x86_64", target_arch = "aarch64"), any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub use jit::*;

pub use z42_runtime_core::{core_identity, CORE_ABI};

pub use z42_component_api as api;
