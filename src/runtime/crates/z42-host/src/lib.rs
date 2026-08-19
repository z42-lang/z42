//! `z42-host` — Tier 2 ergonomic Rust wrapper for the embedding API.
//!
//! Tier 1 (the C ABI in `z42::host`) stays the stable contract;
//! Tier 2 trades a bit of overhead for `Result`-based error handling,
//! `Drop`-based cleanup, and `Box<dyn Fn(...)>` sink callbacks.
//!
//! Spec: docs/design/runtime/embedding.md (§5 Tier 2 Rust API).
//!
//! ```no_run
//! use z42_host::{Host, HostConfig, ExecMode};
//!
//! # fn run() -> Result<(), z42_host::HostError> {
//! let cfg = HostConfig {
//!     exec_mode: ExecMode::Interp,
//!     stdout: Some(Box::new(|bytes| {
//!         std::io::Write::write_all(&mut std::io::stdout(), bytes).unwrap();
//!     })),
//!     search_paths: vec!["artifacts/z42/libs".into()],
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

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::Arc;

use z42_abi::{Z42Value, Z42_VALUE_TAG_BOOL, Z42_VALUE_TAG_F64, Z42_VALUE_TAG_I64, Z42_VALUE_TAG_NULL};

use z42::host::config::{Z42HostConfig, Z42_HOST_ABI_VERSION};
use z42::host::error::Z42HostStatus;
use z42::host::{
    install_zpkg_resolver, z42_host_initialize, z42_host_invoke, z42_host_last_error,
    z42_host_load_zbc, z42_host_resolve_entry, z42_host_set_stderr_sink, z42_host_shutdown,
    Z42Host,
};

pub use z42::host::resolver::ZpkgResolver;

// ── Public types ────────────────────────────────────────────────────────

/// Execution backend. `Default` lets the runtime / `.zbc` metadata
/// decide; explicit modes fail with `HostError::FeatureOff` if the
/// runtime was built without that feature (see
/// `docs/design/runtime/cross-platform.md`).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub enum ExecMode {
    #[default]
    Default,
    Interp,
    Jit,
    Aot,
}

impl ExecMode {
    fn as_raw(self) -> i32 {
        match self {
            ExecMode::Default => 0,
            ExecMode::Interp => 1,
            ExecMode::Jit => 2,
            ExecMode::Aot => 3,
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
    /// [`docs/design/runtime/embedding.md §11`].
    pub zpkg_resolver: Option<Arc<dyn ZpkgResolver>>,
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
        }
    }
}

// ── Built-in resolvers ──────────────────────────────────────────────────

/// `HashMap`-backed eager resolver. The host pre-populates all known
/// zpkgs at startup; `resolve` is a trivial map lookup. Ideal for
/// mobile / WASM where stdlib bundles are loaded once and then served
/// from memory.
///
/// ```no_run
/// # use z42_host::{MapResolver};
/// let mut r = MapResolver::new();
/// r.insert("z42.core", std::fs::read("z42.core.zpkg").unwrap());
/// ```
pub struct MapResolver {
    map: std::sync::RwLock<std::collections::HashMap<String, Vec<u8>>>,
}

impl MapResolver {
    pub fn new() -> Self {
        Self {
            map: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    pub fn insert(&self, namespace: &str, bytes: Vec<u8>) {
        if let Ok(mut g) = self.map.write() {
            g.insert(namespace.to_string(), bytes);
        }
    }

    pub fn with(namespace: &str, bytes: Vec<u8>) -> Arc<Self> {
        let r = Self::new();
        r.insert(namespace, bytes);
        Arc::new(r)
    }
}

impl Default for MapResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl ZpkgResolver for MapResolver {
    fn resolve(&self, namespace: &str) -> Option<Vec<u8>> {
        self.map.read().ok()?.get(namespace).cloned()
    }
}

/// Filesystem-based resolver. Scans the configured directories for
/// `.zpkg` files declaring the requested namespace (mirrors the legacy
/// `search_paths` behaviour). Useful for desktop apps that ship the
/// stdlib alongside the binary and want explicit resolver chaining.
pub struct SearchPathsResolver {
    paths: Vec<PathBuf>,
}

impl SearchPathsResolver {
    pub fn new(paths: Vec<PathBuf>) -> Self {
        Self { paths }
    }
}

impl ZpkgResolver for SearchPathsResolver {
    fn resolve(&self, namespace: &str) -> Option<Vec<u8>> {
        let zpkgs = z42::metadata::resolve_namespace(namespace, &[], &self.paths).ok()?;
        for zpkg_path in zpkgs {
            if let Ok(bytes) = std::fs::read(&zpkg_path) {
                return Some(bytes);
            }
        }
        None
    }
}

/// Read the keys a zpkg can be resolved by — its package name (e.g.
/// `"z42.core"`, by which the prelude is requested) plus every namespace
/// it declares in `NSPC`. Tier 3 facades use this to build a resolution
/// map directly from the packages they ship — no separate index file.
/// Mirrors the C ABI `z42_zpkg_read_namespaces`.
pub fn read_zpkg_namespaces(bytes: &[u8]) -> Result<Vec<String>, HostError> {
    let info = z42::metadata::zbc_reader::read_zpkg_meta(bytes)
        .map_err(|e| HostError::BadZbc(format!("read zpkg metadata: {e:#}")))?;
    let mut keys = Vec::with_capacity(info.namespaces.len() + 1);
    if !info.name.is_empty() {
        keys.push(info.name);
    }
    keys.extend(info.namespaces);
    Ok(keys)
}

/// One-shot: load a z42 app (`.zbc` / `.zpkg`) at `file` and run its entry
/// (`entry` overrides the artifact's baked-in entry hint; `None` uses it).
///
/// add-embedded-app-run: shares z42vm's own app-run core (`z42::app::run`), so
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
    mode: Option<z42::metadata::ExecMode>,
    args: Vec<String>,
) -> Result<(), HostError> {
    let opts = z42::app::RunOpts {
        mode: mode.unwrap_or_else(z42::app::default_mode),
        libs_dir,
        program_args: args,
        print_stats: false,
    };
    z42::app::run(file, entry, opts).map_err(|e| HostError::VmException(format!("{e:#}")))
}

/// Errors surfaced by every `z42-host` API. `message` carries the
/// runtime's diagnostic from `z42_host_last_error`.
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
        Self(Z42Value {
            tag: Z42_VALUE_TAG_NULL,
            reserved: 0,
            payload: 0,
        })
    }
    pub fn i64(v: i64) -> Self {
        Self(Z42Value {
            tag: Z42_VALUE_TAG_I64,
            reserved: 0,
            payload: v as u64,
        })
    }
    pub fn f64(v: f64) -> Self {
        Self(Z42Value {
            tag: Z42_VALUE_TAG_F64,
            reserved: 0,
            payload: v.to_bits(),
        })
    }
    pub fn bool(v: bool) -> Self {
        Self(Z42Value {
            tag: Z42_VALUE_TAG_BOOL,
            reserved: 0,
            payload: if v { 1 } else { 0 },
        })
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

// ── Sink trampoline ─────────────────────────────────────────────────────

/// Wrapper kept alive by `Host` so the sink trampoline can dereference
/// `user_data` safely. `Arc` because the same closure is held by both
/// the trampoline (via raw pointer) and the `Host` (via this `Arc`).
struct SinkBox(Arc<dyn Fn(&[u8]) + Send + Sync + 'static>);

unsafe extern "C" fn sink_trampoline(
    bytes: *const c_char,
    length: usize,
    user_data: *mut c_void,
) {
    if user_data.is_null() {
        return;
    }
    let sb = unsafe { &*(user_data as *const SinkBox) };
    let slice = if bytes.is_null() || length == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(bytes as *const u8, length) }
    };
    (sb.0)(slice);
}

// ── Host / Module / Entry ───────────────────────────────────────────────

/// Single-instance VM handle. Drops invoke `z42_host_shutdown`.
pub struct Host {
    handle: *mut Z42Host,
    /// Heap-allocated sink wrappers retained for the lifetime of the
    /// host so the trampoline's `user_data` pointer stays valid.
    _stdout_sink: Option<Box<SinkBox>>,
    _stderr_sink: Option<Box<SinkBox>>,
}

// All real synchronization happens inside the runtime's RwLock; the
// handle itself is just a sentinel pointer. Marking `Host` as Send
// matches the "thread-serial host" contract in embedding.md §7.
unsafe impl Send for Host {}

/// Loaded `.zbc` (with merged dependencies). Cheap to clone via the
/// underlying handle, but we keep it `!Clone` to make ownership obvious.
pub struct Module {
    handle: *mut z42::host::module::Z42Module,
}

unsafe impl Send for Module {}

/// Resolved entry (function / static method).
pub struct Entry {
    handle: *mut z42::host::entry::Z42Entry,
}

unsafe impl Send for Entry {}

impl Host {
    pub fn new(cfg: HostConfig) -> Result<Self, HostError> {
        // Hold paths + path-string vec alive across the FFI call so the
        // search_paths array still points at valid memory.
        let path_cstrings: Vec<CString> = cfg
            .search_paths
            .iter()
            .map(|p| {
                CString::new(p.to_string_lossy().as_bytes())
                    .map_err(|_| HostError::BadConfig("search_path contains NUL".into()))
            })
            .collect::<Result<_, _>>()?;
        let mut path_ptrs: Vec<*const c_char> =
            path_cstrings.iter().map(|cs| cs.as_ptr()).collect();
        path_ptrs.push(ptr::null());

        let stdout_box = cfg.stdout.map(|f| Box::new(SinkBox(Arc::from(f))));
        let stderr_box = cfg.stderr.map(|f| Box::new(SinkBox(Arc::from(f))));

        let stdout_sink = stdout_box.as_ref().map(|_| sink_trampoline as _);
        let stderr_sink = stderr_box.as_ref().map(|_| sink_trampoline as _);
        // Tier 1's HostConfig only exposes one user_data; pick stdout's
        // when present, else stderr's. Each callback dereferences its
        // own SinkBox by way of the install_*_sink calls below.
        let stdout_user_data = stdout_box
            .as_ref()
            .map(|b| (&**b) as *const SinkBox as *mut c_void)
            .unwrap_or(ptr::null_mut());

        let raw_cfg = Z42HostConfig {
            abi_version: Z42_HOST_ABI_VERSION,
            reserved: 0,
            exec_mode: cfg.exec_mode.as_raw(),
            heap_initial_bytes: cfg.heap_initial.unwrap_or(0),
            heap_max_bytes: cfg.heap_max.unwrap_or(0),
            stdout_sink,
            stderr_sink,
            sink_user_data: stdout_user_data,
            search_paths: if cfg.search_paths.is_empty() {
                ptr::null()
            } else {
                path_ptrs.as_ptr()
            },
            // Tier 2 doesn't round-trip Rust resolvers through C — we
            // install the Arc directly post-init below.
            zpkg_resolver: None,
            zpkg_resolver_user_data: ptr::null_mut(),
        };

        let mut handle: *mut Z42Host = ptr::null_mut();
        let status = unsafe { z42_host_initialize(&raw_cfg, &mut handle) };
        if status != Z42HostStatus::Ok {
            return Err(translate_status(status, "z42_host_initialize"));
        }

        // After initialize succeeds, override the per-sink user_data so
        // each trampoline gets its own SinkBox pointer (Tier 1 supports
        // independent stdout / stderr user_data via the dedicated
        // setters).
        if let Some(b) = stderr_box.as_ref() {
            let ud = (&**b) as *const SinkBox as *mut c_void;
            let s = unsafe { z42_host_set_stderr_sink(handle, Some(sink_trampoline), ud) };
            if s != Z42HostStatus::Ok {
                let _ = unsafe { z42_host_shutdown(handle) };
                return Err(translate_status(s, "z42_host_set_stderr_sink"));
            }
        }

        // Plug in the Rust ZpkgResolver (if any) directly into runtime
        // state, bypassing the C callback adapter for zero-overhead
        // dispatch.
        if let Some(resolver) = cfg.zpkg_resolver {
            let s = install_zpkg_resolver(resolver);
            if s != Z42HostStatus::Ok {
                let _ = unsafe { z42_host_shutdown(handle) };
                return Err(translate_status(s, "install_zpkg_resolver"));
            }
        }

        Ok(Self {
            handle,
            _stdout_sink: stdout_box,
            _stderr_sink: stderr_box,
        })
    }

    pub fn load_zbc(&self, bytes: &[u8]) -> Result<Module, HostError> {
        let mut out: *mut z42::host::module::Z42Module = ptr::null_mut();
        let status = unsafe {
            z42_host_load_zbc(self.handle, bytes.as_ptr(), bytes.len(), &mut out)
        };
        if status != Z42HostStatus::Ok {
            return Err(translate_status(status, "z42_host_load_zbc"));
        }
        Ok(Module { handle: out })
    }

    pub fn load_zbc_path<P: AsRef<Path>>(&self, path: P) -> Result<Module, HostError> {
        let bytes = std::fs::read(&path).map_err(|e| {
            HostError::BadZbc(format!(
                "read {}: {e}",
                path.as_ref().display()
            ))
        })?;
        self.load_zbc(&bytes)
    }

    pub fn resolve_entry(&self, m: &Module, fqn: &str) -> Result<Entry, HostError> {
        let cstr = CString::new(fqn)
            .map_err(|_| HostError::BadConfig("fqn contains NUL".into()))?;
        let mut out: *mut z42::host::entry::Z42Entry = ptr::null_mut();
        let status = unsafe {
            z42_host_resolve_entry(self.handle, m.handle, cstr.as_ptr(), &mut out)
        };
        if status != Z42HostStatus::Ok {
            return Err(translate_status(status, "z42_host_resolve_entry"));
        }
        Ok(Entry { handle: out })
    }

    pub fn invoke(&self, entry: &Entry, args: &[Value]) -> Result<Value, HostError> {
        let raw_args: Vec<Z42Value> = args.iter().map(|v| v.0).collect();
        let args_ptr = if raw_args.is_empty() {
            ptr::null()
        } else {
            raw_args.as_ptr()
        };
        let mut result = Z42Value {
            tag: Z42_VALUE_TAG_NULL,
            reserved: 0,
            payload: 0,
        };
        let status = unsafe {
            z42_host_invoke(entry.handle, args_ptr, raw_args.len(), &mut result)
        };
        if status != Z42HostStatus::Ok {
            return Err(translate_status(status, "z42_host_invoke"));
        }
        Ok(Value(result))
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        // Best-effort shutdown; ignore status because there's nothing
        // useful to do if the runtime is already torn down.
        unsafe {
            let _ = z42_host_shutdown(self.handle);
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn last_error_message() -> String {
    let err = unsafe { z42_host_last_error(ptr::null_mut()) };
    if err.message.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(err.message) }
            .to_string_lossy()
            .into_owned()
    }
}

fn translate_status(status: Z42HostStatus, ctx: &'static str) -> HostError {
    let msg = last_error_message();
    let detail = if msg.is_empty() {
        ctx.to_string()
    } else {
        msg
    };
    match status {
        Z42HostStatus::Ok => HostError::Internal(format!("{ctx} returned OK but caller raised it as error")),
        Z42HostStatus::AlreadyInit => HostError::AlreadyInit(detail),
        Z42HostStatus::NotInit => HostError::NotInit(detail),
        Z42HostStatus::BadConfig => HostError::BadConfig(detail),
        Z42HostStatus::FeatureOff => HostError::FeatureOff(detail),
        Z42HostStatus::BadZbc => HostError::BadZbc(detail),
        Z42HostStatus::Verification => HostError::Verification(detail),
        Z42HostStatus::EntryNotFound => HostError::EntryNotFound(detail),
        Z42HostStatus::ArgMismatch => HostError::ArgMismatch(detail),
        Z42HostStatus::VmException => HostError::VmException(detail),
        Z42HostStatus::Internal => HostError::Internal(detail),
    }
}
