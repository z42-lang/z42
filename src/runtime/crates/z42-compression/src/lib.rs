//! Compression module registration, with an optional full algorithm backend.
pub mod native;
#[cfg(feature = "backend")]
mod backend;
#[cfg(feature = "backend")]
pub use backend::*;
pub use native::{z42_compression_module_v1, z42_compression_get_symbol};
