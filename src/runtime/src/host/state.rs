//! Process-singleton VM state for the Embedding API.
//!
//! Spec: docs/internals/src/runtime/embedding.md §7 + docs/spec/archive/2026-05-10-add-embedding-api/design.md D1/D5.
//!
//! v0.1 holds one optional `HostState` behind an `RwLock`. `initialize`
//! takes the write lock and refuses if state is already `Some`; `shutdown`
//! takes the write lock and replaces with `None`. Multi-instance support
//! is in Deferred — the current shape leaves the public API unchanged.

use std::path::PathBuf;
use std::sync::{OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::metadata::Module;
use crate::vm_context::VmContext;

use super::config::ResolvedConfig;

/// Loaded user module bound to its own `VmContext`. v0.1 keeps one
/// context per loaded `.zbc` so concurrent loads don't cross-contaminate
/// static state. Future work may pool these once multi-instance lands
/// (see embedding.md §12 Deferred).
///
/// The merged module is owned by `ctx` (`VmContext::with_module`, built by
/// `boot::boot_context` — the same boot steps as `app::run`); read it via [`Self::module`].
pub(crate) struct HostModule {
    pub ctx: std::pin::Pin<Box<VmContext>>,
    /// fix-host-static-init: outcome of running the merged packages' `__static_init__`,
    /// done once on the first invoke. A failure is sticky — every later invoke reports
    /// the same error instead of running against half-initialized statics.
    pub static_init: OnceLock<Result<(), String>>,
}

impl HostModule {
    pub(crate) fn module(&self) -> &Module {
        self.ctx
            .module()
            .expect("HostModule.ctx is always built with a module (boot::boot_context)")
    }
}

/// Resolved entry handle. Indexes into `HostModule::module().functions`.
pub(crate) struct HostEntry {
    pub module_idx: usize,
    pub fn_idx: usize,
}

/// Probe result for `z42.core.zpkg`. Captures the discovered path so the
/// loader can re-read the zpkg into a fresh `Module` each time
/// `load_zbc` is called (the runtime `Module` is not `Clone` because
/// it owns indices that aliasing would invalidate; reload from disk is
/// the simplest correct strategy for v0.1).
///
/// `None` when no `search_paths` entry contains `z42.core.zpkg` —
/// callers that load a `.zbc` referencing corelib types will surface a
/// runtime error at first reference, which is documented in §10.
pub(crate) struct HostCorelib {
    /// Absolute path to the discovered `z42.core.zpkg`.
    pub zpkg_path: PathBuf,
    /// Filename(s) recorded as initially-loaded zpkgs so
    /// `LazyLoader::install_with_deps` skips them on lookup.
    pub initially_loaded: Vec<String>,
    /// Directory the zpkg was found in (handed to the lazy loader so it
    /// can locate further dependencies).
    pub libs_dir: PathBuf,
}

/// Shape of an in-process VM. Grows as host iterations land features:
/// H1 holds only `config`; H2 adds `modules` / `entries` / `corelib`;
/// future iterations may pool VmContexts per design.
pub(crate) struct HostState {
    /// Resolved configuration retained for diagnostics and the future
    /// hot-reload / multi-instance work that will need to inspect the
    /// originally-supplied options.
    #[allow(dead_code)]
    pub config: ResolvedConfig,
    pub modules: Vec<HostModule>,
    pub entries: Vec<HostEntry>,
    pub corelib: Option<HostCorelib>,
}

// `ResolvedConfig` carries `Z42WriteSink` (function pointer) and a raw
// user-data address stored as `usize`. Function pointers are `Send`/`Sync`
// in Rust; the user_data address has no Rust ownership. Documented in
// embedding.md §7: hosts must guarantee thread safety on their side.
unsafe impl Send for HostState {}
unsafe impl Sync for HostState {}

static HOST: RwLock<Option<HostState>> = RwLock::new(None);

/// Sentinel pointer returned for `Z42HostRef` while the singleton is
/// alive. Non-null and never dereferenced — see `is_valid_handle`.
pub(crate) const HOST_SENTINEL: usize = 0x1;

/// Outcome of `try_initialize`.
pub(crate) enum InitOutcome {
    Initialized,
    AlreadyInit,
    LockPoisoned,
}

/// Atomically transition Uninitialized → Initialized. The provided
/// `corelib` is the result of probing `config.search_paths` for
/// `z42.core.zpkg` (the caller does the I/O so this function stays
/// purely about state transitions).
pub(crate) fn try_initialize(
    config: ResolvedConfig,
    corelib: Option<HostCorelib>,
) -> InitOutcome {
    let mut guard = match HOST.write() {
        Ok(g) => g,
        Err(_) => return InitOutcome::LockPoisoned,
    };
    if guard.is_some() {
        return InitOutcome::AlreadyInit;
    }
    *guard = Some(HostState {
        config,
        modules: Vec::new(),
        entries: Vec::new(),
        corelib,
    });
    InitOutcome::Initialized
}

/// Outcome of `try_shutdown`.
pub(crate) enum ShutdownOutcome {
    Done,
    NotInit,
    LockPoisoned,
}

/// Atomically transition Initialized → Uninitialized.
pub(crate) fn try_shutdown() -> ShutdownOutcome {
    let mut guard = match HOST.write() {
        Ok(g) => g,
        Err(_) => return ShutdownOutcome::LockPoisoned,
    };
    if guard.is_none() {
        return ShutdownOutcome::NotInit;
    }
    *guard = None;
    ShutdownOutcome::Done
}

/// Whether the singleton is currently initialized.
pub(crate) fn is_initialized() -> bool {
    matches!(HOST.read(), Ok(g) if g.is_some())
}

/// `true` if `handle` matches the singleton sentinel and the VM is live.
pub(crate) fn is_valid_handle(handle_ptr: *const ()) -> bool {
    !handle_ptr.is_null() && handle_ptr as usize == HOST_SENTINEL && is_initialized()
}

/// Run a closure with a read guard on the host state. Returns `None` if
/// the VM is not initialized. Use this for non-mutating work that
/// touches `modules` / `entries` / `corelib`.
pub(crate) fn with_state_read<R>(
    f: impl FnOnce(&HostState) -> R,
) -> Option<R> {
    let guard: RwLockReadGuard<'_, Option<HostState>> = HOST.read().ok()?;
    guard.as_ref().map(f)
}

/// Run a closure with a write guard. Returns `None` when the VM is not
/// initialized; otherwise yields `Some(closure_result)`.
pub(crate) fn with_state_write<R>(
    f: impl FnOnce(&mut HostState) -> R,
) -> Option<R> {
    let mut guard: RwLockWriteGuard<'_, Option<HostState>> = HOST.write().ok()?;
    guard.as_mut().map(f)
}
