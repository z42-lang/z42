//! Shared native implementations and unsupported-platform sentinels.
#[cfg(not(unix))]
pub mod mem;
#[cfg(feature = "net")]
pub mod net;
#[cfg(not(any(unix, windows)))]
pub mod process;
#[cfg(not(target_arch = "wasm32"))]
pub mod random;
#[cfg(not(unix))]
pub mod signal;
#[cfg(not(target_arch = "wasm32"))]
pub mod system;
#[cfg(not(unix))]
pub mod thread;
#[cfg(not(target_arch = "wasm32"))]
pub mod time;
