//! Component protocol, provider loading and per-VM lifetime management.
pub(crate) mod api;
mod catalog;
mod instance;
mod options;
mod linked;
mod backends;
mod profiling;

pub use instance::{ComponentLease, VmComponents};
pub use options::{ComponentOptions, component_interface, component_ids};
pub use linked::register_static_component;
pub use backends::{BackendOptions, BackendSource};

mod execution_roles;
#[cfg(test)] mod capability_api_tests;
#[cfg(test)] mod component_tests;
#[cfg(test)] mod backend_tests;

#[cfg(test)] mod profiling_tests;
