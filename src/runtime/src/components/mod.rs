//! Component protocol, provider loading and per-VM lifetime management.
pub mod api;
mod catalog;
mod instance;
mod options;
mod identity;
mod linked;
pub(crate) mod backends;
pub(crate) mod profiling;

pub use catalog::Factory;
pub use instance::{ComponentLease, VmComponents};
pub use options::{ComponentOptions, component_interface, component_ids};
pub use identity::{core_identity, CORE_ABI};
pub use linked::register_static_component;
pub use backends::{BackendOptions, BackendSource, jit_supported, jit_available, interp_available};
#[cfg(any(feature = "native-interop", feature = "component-loading"))]
pub(crate) use backends::native_paths;

#[cfg(test)] mod api_tests;
#[cfg(test)] mod component_tests;
#[cfg(test)] mod backend_tests;

#[cfg(test)] mod profiling_tests;
