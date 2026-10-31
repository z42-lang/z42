//! Fallback backend for the PAL concern interfaces.
pub(crate) mod fs;
pub(crate) mod system;
pub(crate) use super::common::mem;
pub(crate) use super::common::random;
pub(crate) use super::common::time;
pub(crate) mod thread;
pub(crate) use super::common::process;
pub(crate) mod net;
pub(crate) use super::common::signal;
