//! Shared Rust host services and C embedding adapters.
mod components;
mod component_catalog;
pub use z42_component_api::*;
pub use components::{ComponentOptions, component_interface, component_ids};
pub use crate::vm::components::ComponentLease;
pub use crate::{object::Value, pal::fs::Vfs, vm::context::{VmContext, VmCore}, vm::runtime::Module};
pub mod config;
pub mod resolver;
pub mod runtime;
pub mod signal;
pub mod error;
pub mod entry;
pub mod module;
pub mod marshal;
pub mod ops;
pub mod state;
mod ffi;
mod app;
pub(crate) mod boot;
mod load;
pub(crate) mod app_state;
pub(crate) mod services;
pub use z42_zpkg::{ExecMode, image::PackageImage};
pub use ffi::*;
pub use resolver::{read_zpkg_namespaces, ZpkgResolver};
pub use runtime::{LoadOptions, LoadedProgram};
pub use app::{default_mode, run_app, RunOpts};
pub use app_state::{program_args, set_program_args};
pub(crate) use app::report_on_exit;
#[cfg(test)] mod host_tests;
#[cfg(test)] mod app_tests;
#[cfg(test)] mod runtime_tests;

#[cfg(test)] mod component_tests;

pub use crate::assembly::context::VmContextFactory;
pub use crate::assembly::loader::{load_artifact, load_artifact_from_bytes, build_block_indices, fold_availability, LazyLoaderFactory};
mod backends;
pub use backends::{BackendOptions, BackendSource, jit_supported, jit_available, interp_available};
#[cfg(test)]
mod backend_tests;

pub(crate) use backends::native_paths;
pub use crate::vm::ext::VmExtensions;

pub use z42_abi::{Z42Value, Z42_VALUE_TAG_I64};
