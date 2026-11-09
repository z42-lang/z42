//! REPL script registration and portable adapter. The terminal/history backend is optional.
pub mod native;
#[cfg(feature = "backend")]
mod backend;
#[cfg(feature = "backend")]
pub use backend::*;
#[cfg(feature = "backend")]
pub(crate) use backend::{call_complete, call_key_edit, highlight};
pub use native::z42_repl_module_v1;
