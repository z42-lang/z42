//! Shared native implementations and unsupported-platform sentinels.
#[cfg(not(unix))]
pub(crate) mod mem;
#[cfg(feature = "net")]
pub(crate) mod net;
#[cfg(not(any(unix, windows)))]
pub(crate) mod process;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod random;
#[cfg(not(unix))]
pub(crate) mod signal;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod system;
#[cfg(not(unix))]
pub(crate) mod thread;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod time;
