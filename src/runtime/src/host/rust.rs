//! `z42-host` — Tier 2 ergonomic Rust wrapper for the embedding API.
//!
//! Rust and C use the same session operations. This adapter provides typed errors,
//! `Drop`-based cleanup, and owned Rust output callbacks.
//!
//! Spec: docs/internals/src/runtime/embedding.md (§5 Tier 2 Rust API).
//!
//! ```no_run
//! use z42::host::rust::{Host, HostConfig, ExecMode};
//!
//! # fn run() -> Result<(), z42::host::rust::HostError> {
//! let cfg = HostConfig {
//!     exec_mode: ExecMode::Interp,
//!     stdout: Some(Box::new(|bytes| {
//!         std::io::Write::write_all(&mut std::io::stdout(), bytes).unwrap();
//!     })),
//!     search_paths: vec!["artifacts/intermediate/libraries/flat/release".into()],
//!     ..Default::default()
//! };
//! let host  = Host::new(cfg)?;
//! let bytes = std::fs::read("hello.zbc").unwrap();
//! let m     = host.load_zbc(&bytes)?;
//! let entry = host.resolve_entry(&m, "Embedding.Hello.Main")?;
//! host.invoke(&entry, &[])?;
//! # Ok(())
//! # }
//! ```
//!
//! The `Host` is a process singleton in v0.1; constructing two
//! concurrently returns `HostError::AlreadyInit`. Multi-instance lands
//! in a future iteration (see embedding.md §12 Deferred).

#[path = "rust_resolvers.rs"]
mod resolvers;
pub use resolvers::{MapResolver, SearchPathsResolver};
use std::cell::Cell;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use z42_abi::{
    Z42_VALUE_TAG_BOOL, Z42_VALUE_TAG_F64, Z42_VALUE_TAG_I64, Z42_VALUE_TAG_NULL, Z42Value,
};

use super::{config, error::Z42HostStatus, marshal, state};
use crate::corelib::io::{HostSinks, OutputSink};

pub use z42::host::resolver::ZpkgResolver;
pub use z42::pal::fs::Vfs;

// ── Public types ────────────────────────────────────────────────────────

/// Execution backend. `Default` lets the runtime / `.zbc` metadata
/// decide; explicit modes fail with `HostError::FeatureOff` if the
/// runtime was built without that feature (see
/// `docs/internals/src/runtime/cross-platform.md`).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub enum ExecMode {
    #[default]
    Default,
    Interp,
    Jit,
    Aot,
}

impl ExecMode {
    fn session_mode(self) -> config::Z42ExecMode {
        match self {
            Self::Default => config::Z42ExecMode::Default,
            Self::Interp => config::Z42ExecMode::Interp,
            Self::Jit => config::Z42ExecMode::Jit,
            Self::Aot => config::Z42ExecMode::Aot,
        }
    }
}

/// Configuration handed to [`Host::new`].
pub struct HostConfig {
    pub exec_mode: ExecMode,
    pub heap_initial: Option<usize>,
    pub heap_max: Option<usize>,
    pub stdout: Option<Box<dyn Fn(&[u8]) + Send + Sync + 'static>>,
    pub stderr: Option<Box<dyn Fn(&[u8]) + Send + Sync + 'static>>,
    pub search_paths: Vec<PathBuf>,
    /// Optional [`ZpkgResolver`]. Takes precedence over `search_paths`
    /// during `load_zbc` namespace resolution; runtime falls back to
    /// `search_paths` on miss. See
    /// [`docs/internals/src/runtime/embedding.md §11`].
    pub zpkg_resolver: Option<Arc<dyn ZpkgResolver>>,
    /// The file system the host's VMs load packages from and run on (`search_paths`
    /// are looked up on it too). `None` = the runtime's default: the OS file system,
    /// or a fresh in-memory one where the target has none. A host that mounts files
    /// into a `z42::pal::fs::MemoryFs` passes it here.
    pub vfs: Option<Arc<dyn Vfs>>,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            exec_mode: ExecMode::Default,
            heap_initial: None,
            heap_max: None,
            stdout: None,
            stderr: None,
            search_paths: Vec::new(),
            zpkg_resolver: None,
            vfs: None,
        }
    }
}

/// Read the keys a zpkg can be resolved by — its package name (e.g.
/// `"z42.core"`, by which the prelude is requested) plus every namespace
/// it declares in `NSPC`. Tier 3 facades use this to build a resolution
/// map directly from the packages they ship — no separate index file.
/// Mirrors the C ABI `z42_zpkg_read_namespaces`.
pub fn read_zpkg_namespaces(bytes: &[u8]) -> Result<Vec<String>, HostError> {
    z42::host::read_zpkg_namespaces(bytes)
        .map_err(|e| HostError::BadZbc(format!("read zpkg metadata: {e:#}")))
}

/// One-shot: load a z42 app (`.zbc` / `.zpkg`) at `file` and run its entry
/// (`entry` overrides the artifact's baked-in entry hint; `None` uses it).
///
/// add-embedded-app-run: shares z42vm's own app-run core (`z42::host::run_app`), so
/// desktop self-contained + wasm / iOS / Android + any user cross-platform app
/// run identically embedded. `libs_dir` is the stdlib `libs/` dir (deps resolve
/// from the app's own directory first, then here). `mode = None` → build default
/// (JIT if compiled in, else Interp). Returns `Ok(())` when the program ran
/// (a z42-level failure surfaces as the program's own output / a thrown
/// exception → `Err(VmException)`), so callers reading structured results (e.g.
/// the test-agent's JSON) use that output, not an exit code.
pub fn run_app(
    file: &str,
    entry: Option<&str>,
    libs_dir: Option<PathBuf>,
    mode: Option<z42::host::ExecMode>,
    args: Vec<String>,
) -> Result<(), HostError> {
    crate::composition::install_app_config(file);
    let vfs = z42::pal::fs::default_vfs();
    let opts = z42::host::RunOpts {
        mode: mode.unwrap_or_else(z42::host::default_mode),
        search_dirs: crate::composition::resolve_search_dirs(
            file,
            libs_dir.as_deref(),
            &*vfs,
            crate::config::runtime_config(),
        ),
        vfs,
        resolver: None,
        program_args: args,
        print_stats: false,
        stats_json: false,
        exit_after_run: false,
    };
    z42::host::run_app(file, entry, opts).map_err(|e| match z42::exec::stack_guard::take_report() {
        Some(report) => HostError::Fatal(report),
        None => HostError::VmException(format!("{e:#}")),
    })
}

/// Errors surfaced by the Rust host API, with the runtime diagnostic.
#[derive(Debug, Clone)]
pub enum HostError {
    AlreadyInit(String),
    NotInit(String),
    BadConfig(String),
    FeatureOff(String),
    BadZbc(String),
    Verification(String),
    EntryNotFound(String),
    ArgMismatch(String),
    VmException(String),
    /// Fatal VM error (stack overflow); the VM must be shut down.
    Fatal(String),
    Internal(String),
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (tag, msg) = match self {
            Self::AlreadyInit(m) => ("AlreadyInit", m),
            Self::NotInit(m) => ("NotInit", m),
            Self::BadConfig(m) => ("BadConfig", m),
            Self::FeatureOff(m) => ("FeatureOff", m),
            Self::BadZbc(m) => ("BadZbc", m),
            Self::Verification(m) => ("Verification", m),
            Self::EntryNotFound(m) => ("EntryNotFound", m),
            Self::ArgMismatch(m) => ("ArgMismatch", m),
            Self::VmException(m) => ("VmException", m),
            Self::Fatal(m) => ("Fatal", m),
            Self::Internal(m) => ("Internal", m),
        };
        write!(f, "{tag}: {msg}")
    }
}

impl std::error::Error for HostError {}

/// A primitive value crossing the host ABI. H2 only supports null + i64
/// + f64 + bool; strings / objects / arrays land in H3.
#[derive(Debug, Copy, Clone)]
pub struct Value(Z42Value);

impl Value {
    pub fn null() -> Self {
        Self(super::marshal::raw_value(Z42_VALUE_TAG_NULL, 0))
    }
    pub fn i64(v: i64) -> Self {
        Self(super::marshal::raw_value(Z42_VALUE_TAG_I64, v as u64))
    }
    pub fn f64(v: f64) -> Self {
        Self(super::marshal::raw_value(Z42_VALUE_TAG_F64, v.to_bits()))
    }
    pub fn bool(v: bool) -> Self {
        Self(super::marshal::raw_value(
            Z42_VALUE_TAG_BOOL,
            if v { 1 } else { 0 },
        ))
    }

    pub fn is_null(self) -> bool {
        self.0.tag == Z42_VALUE_TAG_NULL
    }
    pub fn as_i64(self) -> Option<i64> {
        if self.0.tag == Z42_VALUE_TAG_I64 {
            Some(self.0.payload as i64)
        } else {
            None
        }
    }
    pub fn as_f64(self) -> Option<f64> {
        if self.0.tag == Z42_VALUE_TAG_F64 {
            Some(f64::from_bits(self.0.payload))
        } else {
            None
        }
    }
    pub fn as_bool(self) -> Option<bool> {
        if self.0.tag == Z42_VALUE_TAG_BOOL {
            Some(self.0.payload != 0)
        } else {
            None
        }
    }

    pub fn raw(self) -> Z42Value {
        self.0
    }
}

/// Handle for the process-wide embedding session. Dropping it closes that session.
pub struct Host {
    token: usize,
    // A Host can move between threads; clients drive it from one thread at a time.
    _not_sync: PhantomData<Cell<()>>,
}

/// Loaded program in the embedding session.
pub struct Module {
    token: usize,
    _not_sync: PhantomData<Cell<()>>,
}

/// Resolved entry (function or static method).
pub struct Entry {
    token: usize,
    _not_sync: PhantomData<Cell<()>>,
}

impl Host {
    pub fn new(cfg: HostConfig) -> Result<Self, HostError> {
        let mode = cfg.exec_mode.session_mode();
        config::check_feature_available(mode).map_err(|_| {
            HostError::FeatureOff(format!("z42_host_initialize: exec_mode {mode:?} requires a feature disabled at compile time"))
        })?;
        if cfg.search_paths.len() >= 4096 {
            return Err(HostError::BadConfig(
                "search_paths array exceeded 4096 entries".into(),
            ));
        }
        let search_paths = cfg
            .search_paths
            .iter()
            .map(|path| {
                let path = path.to_string_lossy();
                if path.as_bytes().contains(&0) {
                    return Err(HostError::BadConfig("search_path contains NUL".into()));
                }
                Ok(path.into_owned())
            })
            .collect::<Result<_, _>>()?;

        let config = config::ResolvedConfig {
            exec_mode: mode,
            heap_initial_bytes: cfg.heap_initial.unwrap_or(0),
            heap_max_bytes: cfg.heap_max.unwrap_or(0),
            search_paths,
            zpkg_resolver: cfg.zpkg_resolver,
            vfs: cfg.vfs.unwrap_or_else(crate::pal::fs::default_vfs),
        };
        let sinks = HostSinks {
            stdout: cfg.stdout.map(|sink| OutputSink::Rust(Arc::from(sink))),
            stderr: cfg.stderr.map(|sink| OutputSink::Rust(Arc::from(sink))),
        };
        let token = session_call("z42_host_initialize", || state::initialize(config, sinks))?;
        Ok(Self {
            token,
            _not_sync: PhantomData,
        })
    }

    pub fn load_zbc(&self, bytes: &[u8]) -> Result<Module, HostError> {
        session_call("z42_host_load_zbc", || state::load(self.token, bytes)).map(|token| Module {
            token,
            _not_sync: PhantomData,
        })
    }

    pub fn load_zbc_path<P: AsRef<Path>>(&self, path: P) -> Result<Module, HostError> {
        let bytes = std::fs::read(&path)
            .map_err(|e| HostError::BadZbc(format!("read {}: {e}", path.as_ref().display())))?;
        self.load_zbc(&bytes)
    }

    pub fn resolve_entry(&self, module: &Module, fqn: &str) -> Result<Entry, HostError> {
        if fqn.as_bytes().contains(&0) {
            return Err(HostError::BadConfig("fqn contains NUL".into()));
        }
        session_call("z42_host_resolve_entry", || {
            state::resolve(self.token, module.token, fqn)
        })
        .map(|token| Entry {
            token,
            _not_sync: PhantomData,
        })
    }

    pub fn invoke(&self, entry: &Entry, args: &[Value]) -> Result<Value, HostError> {
        session_call("z42_host_invoke", || {
            marshal::invoke(self.token, entry.token, args.iter().map(|value| &value.0))
        })
        .map(Value)
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = session_call("z42_host_shutdown", || state::shutdown(self.token));
    }
}

fn session_call<T>(
    operation: &'static str,
    f: impl FnOnce() -> Result<T, state::Failure>,
) -> Result<T, HostError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
        .map_err(|_| HostError::Internal(format!("{operation}: internal panic")))?
        .map_err(|error| translate_failure(error, operation))
}

fn translate_failure(failure: state::Failure, ctx: &'static str) -> HostError {
    let detail = format!("{ctx}: {}", failure.message);
    match failure.status {
        Z42HostStatus::Ok => {
            HostError::Internal(format!("{ctx} returned OK but caller raised it as error"))
        }
        Z42HostStatus::AlreadyInit => HostError::AlreadyInit(detail),
        Z42HostStatus::NotInit => HostError::NotInit(detail),
        Z42HostStatus::BadConfig => HostError::BadConfig(detail),
        Z42HostStatus::FeatureOff => HostError::FeatureOff(detail),
        Z42HostStatus::BadZbc => HostError::BadZbc(detail),
        Z42HostStatus::Verification => HostError::Verification(detail),
        Z42HostStatus::EntryNotFound => HostError::EntryNotFound(detail),
        Z42HostStatus::ArgMismatch => HostError::ArgMismatch(detail),
        Z42HostStatus::VmException => HostError::VmException(detail),
        Z42HostStatus::Fatal => HostError::Fatal(detail),
        Z42HostStatus::Internal => HostError::Internal(detail),
    }
}
