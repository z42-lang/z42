//! Native type registration, C ABI exports and libffi dispatch.

pub(crate) mod call;
pub mod dispatch;
pub mod error;
pub mod exports;
pub mod loader;
pub mod marshal;
pub mod registry;

#[cfg(test)]
mod dispatch_tests;
#[cfg(test)]
mod marshal_tests;
#[cfg(test)]
mod registry_tests;
