//! Compile-time OS backend selection. No runtime dispatch or VM types.
#[cfg(all(unix, not(target_arch = "wasm32")))]
mod unix;
#[cfg(all(unix, not(target_arch = "wasm32")))]
pub(crate) use unix::*;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) use windows::*;
#[cfg(target_arch = "wasm32")]
mod wasm;
#[cfg(target_arch = "wasm32")]
pub(crate) use wasm::*;
#[cfg(not(any(unix, windows, target_arch = "wasm32")))]
mod fallback;
#[cfg(not(any(unix, windows, target_arch = "wasm32")))]
pub(crate) use fallback::*;
mod common;
