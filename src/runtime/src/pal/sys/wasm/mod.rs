//! Wasm backend for the PAL concern interfaces.
pub(crate) mod fs;
pub(crate) mod system;
pub(crate) use super::common::mem;
pub(crate) mod random;
pub(crate) mod thread;
pub(crate) mod time;
pub(crate) use super::common::process;
pub(crate) mod net;
pub(crate) use super::common::signal;
