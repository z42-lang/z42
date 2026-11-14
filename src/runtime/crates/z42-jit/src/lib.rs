//! Independent jit execution backend, sharing the runtime core.
use z42::{context, exec, loader, metadata, diagnostics, threads, object, gc, objops, zpkg};
#[cfg(all(feature = "jit", any(target_arch = "x86_64", target_arch = "aarch64"), any(target_os = "linux", target_os = "macos", target_os = "windows", target_os = "android", all(target_os = "ios", target_abi = "sim"))))]
pub mod jit;
#[cfg(all(feature = "jit", any(target_arch = "x86_64", target_arch = "aarch64"), any(target_os = "linux", target_os = "macos", target_os = "windows", target_os = "android", all(target_os = "ios", target_abi = "sim"))))]
pub use jit::*;
