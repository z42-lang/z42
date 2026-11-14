//! Component protocol, provider loading and per-VM lifetime management.
pub mod abi;
#[doc(hidden)]
pub mod bridge;
pub(crate) mod api;
mod catalog;
mod instance;
mod options;
mod linked;
mod backends;
mod profiling;


mod execution_roles;
#[cfg(test)] mod capability_api_tests;
#[cfg(test)] mod component_tests;
#[cfg(test)] mod backend_tests;

#[cfg(test)] mod profiling_tests;
