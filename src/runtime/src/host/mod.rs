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
//! C handle consumers import the owning host API rather than implementation files:
//! ```
//! use z42::host::{Z42Host, Z42Module, Z42Entry};
//! let _: (*mut Z42Host, *mut Z42Module, *mut Z42Entry) =
//!     (std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
//! ```
//! ```compile_fail
//! use z42::host::entry::Z42Entry;
//! ```
//! ```compile_fail
//! use z42::host::VmContext;
//! ```
//! ```compile_fail
//! use z42::host::module::Z42Module;
//! ```
pub use crate::{object::Value, pal::fs::Vfs, metadata::Module};
// VmContext stays reachable at `z42::vm` only: hosts express functional
// requests; the low-level context is a VM-internal facility.
use crate::vm::VmContext;
pub mod resolver;
pub(crate) mod program;
pub mod signal;
pub mod error;
/// C embedding adapter (`z42_host` ABI): config, session, marshaling and the
/// `extern "C"` entry points. Reached only through this module's re-exports.
/// C embedding adapter files (`z42_host` ABI): config validation, the
/// process session, marshaling and the `extern "C"` entry points live flat
/// here; the Rust embedding surface shares the root.
pub(crate) mod config;
pub(crate) mod ffi;
pub(crate) mod marshal;
pub(crate) mod ops;
pub(crate) mod state;
mod app;
mod load;
pub(crate) mod app_state;
pub(crate) mod services;
pub use z42_zpkg::{ExecMode, image::PackageImage};
pub use config::{Z42ExecMode, Z42_HOST_ABI_VERSION, Z42HostConfig};
pub use ffi::{
    Z42Host, Z42Module, Z42Entry, z42_host_initialize, z42_host_shutdown,
    z42_host_load_zbc, z42_host_resolve_entry, z42_host_invoke,
    z42_host_set_stdout_sink, z42_host_set_stderr_sink, z42_host_last_error,
    z42_zpkg_read_namespaces, install_zpkg_resolver, install_vfs, z42_host_run_app,
};
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


/// Optional analysis services exposed through the embedding facade.
pub fn profiler_loaded(ctx: &VmContext) -> bool { ctx.core().diagnostics().sampler.is_loaded() }
/// Resolve the configured profiler, validating its build/core identity first.
pub fn prepare_profiler(ctx: &VmContext) -> anyhow::Result<()> {
    crate::components::api::profiler_service(ctx).map(|_| ())
}
pub fn write_heap_snapshot(ctx: &VmContext, path: &str) -> anyhow::Result<i64> {
    crate::components::api::profiler_service(ctx)?.write_heap_snapshot(ctx, path)
}
