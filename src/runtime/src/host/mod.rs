//! Shared Rust host services and C embedding adapters.
//! Component management stays private to the VM. Ordinary hosts load and invoke programs.
//!
//! ```compile_fail
//! use z42::host::component_ids;
//! ```
//! ```compile_fail
//! use z42::host::component_interface;
//! ```
//! ```compile_fail
//! use z42::host::ComponentLease;
//! ```
//! ```compile_fail
//! use z42::host::ComponentOptions;
//! ```
//! ```compile_fail
//! use z42::host::BackendOptions;
//! ```
//! ```compile_fail
//! use z42::host::BackendSource;
//! ```
//! ```
//! let mut request = z42::host::LoadOptions::default();
//! request.native.allow_runtime = false;
//! request.allow_profiling = false;
//! ```
pub use crate::{object::Value, pal::fs::Vfs, vm::{VmContext, VmCore}, metadata::Module};
pub mod config;
pub mod resolver;
pub(crate) mod program;
pub mod signal;
pub mod error;
pub mod entry;
pub mod module;
pub mod marshal;
pub mod ops;
pub mod state;
mod ffi;
mod app;
mod load;
pub(crate) mod app_state;
pub(crate) mod services;
pub use z42_zpkg::{ExecMode, image::PackageImage};
pub use ffi::*;
pub use resolver::{read_zpkg_namespaces, ZpkgResolver};
pub use program::{LoadOptions, LoadedProgram};
pub use app::{default_mode, execution_available, run_app, RunOpts};
pub use app_state::{program_args, set_program_args};
pub(crate) use app::report_on_exit;
#[cfg(test)] mod host_tests;
#[cfg(test)] mod app_tests;
#[cfg(test)] pub(crate) mod program_tests;


pub use crate::vm::VmExtensions;

pub use z42_abi::{Z42Value, Z42_VALUE_TAG_I64};

/// Ergonomic Rust embedding over the shared host services.
pub mod rust;


pub use crate::config::{RuntimeConfig, VmConfig};



mod profiling;
pub use profiling::{profiler_loaded, prepare_profiler, write_heap_snapshot};
