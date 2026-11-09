//! REPL script registration and portable adapter. The terminal/history backend is optional.
pub mod registration;
#[cfg(feature = "backend")]
mod editor;
#[cfg(feature = "backend")]
pub use editor::*;
#[cfg(feature = "backend")]
pub(crate) use editor::{call_complete, call_key_edit, highlight};
pub use registration::z42_repl_module_v1;
