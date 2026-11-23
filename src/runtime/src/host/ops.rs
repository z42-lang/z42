//! C ABI operation adapters over the shared Rust host service.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;

use crate::corelib::io;
use crate::object::Value;

use super::config::ResolvedConfig;
use super::marshal::{value_to_z42_value, z42_value_to_value};
use super::state::HostEntry;

/// Validate an available prelude during C host initialization.
/// Missing corelib remains valid for self-contained modules. Actual preparation
/// and dependency selection are performed by the shared loader for each program.
pub(crate) fn probe_corelib(config: &ResolvedConfig) -> Result<()> {
    crate::loader::probe_prelude(&*config.vfs, &config.search_paths)
}

/// Load bytes through the same preparation service as file-based application runs.
pub(crate) fn build_host_module(config: &ResolvedConfig, bytes: &[u8], image_cache: &crate::loader::ImageCache) -> Result<crate::host::LoadedProgram> {
    crate::host::LoadedProgram::load_bytes_with_cache(bytes, crate::host::LoadOptions {
        native: Default::default(), allow_profiling: true,
        mode: match config.exec_mode {
            super::config::Z42ExecMode::Default => crate::host::default_mode(),
            super::config::Z42ExecMode::Interp => crate::zpkg::ExecMode::Interp,
            super::config::Z42ExecMode::Jit => crate::zpkg::ExecMode::Jit,
            super::config::Z42ExecMode::Aot => crate::zpkg::ExecMode::Aot,
        },
        vfs: Arc::clone(&config.vfs),
        search_dirs: config.search_paths.iter().map(PathBuf::from).collect(),
        resolver: config.zpkg_resolver.clone(),
        eager_dependencies: true,
    }, image_cache)
}

/// RAII guard that toggles the per-thread "host sink active" flag.
/// Activated on entry to `z42_host_invoke`, restored on Drop so panics
/// or early returns can't leave a dangling flag.
pub(crate) struct HostSinkGuard {
    prev: bool,
}

impl HostSinkGuard {
    pub(crate) fn enter() -> Self {
        let prev = io::host_sink_set_active(true);
        Self { prev }
    }
}

impl Drop for HostSinkGuard {
    fn drop(&mut self) {
        io::host_sink_set_active(self.prev);
    }
}

/// Invoke an entry point. Marshals args + return value through the
/// `Z42Value` ABI; activates host stdout / stderr dispatch for the
/// duration of the call.
///
/// Errors are returned as `anyhow::Error` so the `extern "C"` boundary
/// can classify them. The caller maps the special prefix
/// `"arg-count-mismatch:"` to `Z42_HOST_ERR_ARG_MISMATCH`; any z42
/// throw escaping the entry surfaces as `Z42_HOST_ERR_VM_EXCEPTION`
/// via `classify_invoke_error`.
pub(crate) fn invoke_impl(
    host_module: &crate::host::LoadedProgram,
    entry: &HostEntry,
    args_bytes: &[Value],
) -> Result<Option<Value>> {
    let _sink_guard = HostSinkGuard::enter();
    host_module.invoke_index(entry.fn_idx, args_bytes)
}

/// Marshal a slice of `Z42Value` into runtime `Value`s. Errors mirror
/// `Z42_HOST_ERR_ARG_MISMATCH` semantics (caller maps).
pub(crate) fn marshal_args(args: &[z42_abi::Z42Value]) -> Result<Vec<Value>> {
    args.iter().map(z42_value_to_value).collect()
}

/// Marshal a returned `Option<Value>` back to a `Z42Value` writable to
/// `out_result`. `None` (void return) maps to a NULL-tagged value.
pub(crate) fn marshal_return(value: Option<&Value>) -> Result<z42_abi::Z42Value> {
    value_to_z42_value(value)
}
