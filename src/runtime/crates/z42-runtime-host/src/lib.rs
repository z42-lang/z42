//! Native embedding facade for a shared-core runtime pack. The VM implementation
//! stays in z42-runtime-shared; C entry points are the same host implementation.
#[cfg(feature = "dynamic-component")]
pub use ::runtime::host::*;
