//! Compression signatures and optional algorithm implementation.
pub mod registration;
#[cfg(feature = "backend")]
mod c_api;
#[cfg(feature = "backend")]
pub use c_api::*;
#[cfg(feature = "backend")]
pub use registration::{z42_compression_module_v1, z42_compression_get_symbol};
