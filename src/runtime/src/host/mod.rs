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
//! use z42::host::{LoadOptions, LoadedProgram};
//! let mut request = LoadOptions::default();
//! let _ = LoadedProgram::load_bytes;
//! request.native.allow_runtime = false;
//! request.allow_profiling = false;
//! ```
//! ```compile_fail
//! use z42::vm::program::LoadedProgram;
//! ```
//! ```
//! use z42::host::resolver::{read_zpkg_namespaces, ZpkgResolver};
//! let _: Option<&dyn ZpkgResolver> = None;
//! let _ = read_zpkg_namespaces;
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
/// Package resolver contract and resolver-key discovery.
pub mod resolver {
    pub use crate::loader::program::{read_zpkg_namespaces, ZpkgResolver};
}
/// Process crash reporting for native and Rust hosts.
pub mod signal {
    pub use crate::composition::install_crash_report as install;
}
pub mod error;
// C ABI configuration, session, marshaling and entry points.
pub(crate) mod config;
pub(crate) mod ffi;
pub(crate) mod marshal;
pub(crate) mod state;
mod app;
pub use z42_zpkg::{ExecMode, image::PackageImage};
pub use config::{Z42ExecMode, Z42_HOST_ABI_VERSION, Z42HostConfig};
pub use ffi::{
    Z42Host, Z42Module, Z42Entry, z42_host_initialize, z42_host_shutdown,
    z42_host_load_zbc, z42_host_resolve_entry, z42_host_invoke,
    z42_host_set_stdout_sink, z42_host_set_stderr_sink, z42_host_last_error,
    z42_zpkg_read_namespaces, install_zpkg_resolver, install_vfs, z42_host_run_app,
};
pub use resolver::{read_zpkg_namespaces, ZpkgResolver};
pub use crate::vm::program::{default_mode, execution_available, LoadOptions, LoadedProgram};
pub use app::{run_app, RunOpts};
pub use crate::vm::app_state::{program_args, set_program_args};
#[cfg(test)] mod host_tests;
#[cfg(test)] mod app_tests;
#[cfg(test)] pub(crate) mod program_tests;


pub use crate::vm::VmExtensions;

pub use z42_abi::{Z42Value, Z42_VALUE_TAG_I64};

/// Ergonomic Rust embedding over the shared host services.
pub mod rust;


pub use crate::config::{RuntimeConfig, VmConfig};


/// Whether this VM already has a profiler provider installed.
pub fn profiler_loaded(ctx: &VmContext) -> bool {
    crate::diagnostics::profiler_loaded(ctx)
}

/// Resolve the configured profiler, validating its build/core identity first.
pub fn prepare_profiler(ctx: &VmContext) -> anyhow::Result<()> {
    crate::diagnostics::prepare_profiler(ctx)
}

/// Write a heap snapshot using the configured profiler.
pub fn write_heap_snapshot(ctx: &VmContext, path: &str) -> anyhow::Result<i64> {
    crate::diagnostics::write_heap_snapshot(ctx, path)
}
