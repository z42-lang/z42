//! Process-singleton VM state for the Embedding API.
//!
//! Spec: docs/internals/src/runtime/embedding.md §7 + 变更 add-embedding-api/design.md D1/D5.
//!
//! v0.1 holds one optional `HostState` behind an `RwLock`. `initialize`
//! takes the write lock and refuses if state is already `Some`; `shutdown`
//! takes the write lock and replaces with `None`. Multi-instance support
//! is in Deferred — the current shape leaves the public API unchanged.

use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};


use super::config::ResolvedConfig;

/// Resolved entry handle. Indexes into `super::LoadedProgram::module().functions`.
#[derive(Clone, Copy)]
pub(crate) struct HostEntry {
    pub module_idx: usize,
    pub fn_idx: usize,
}

/// Shape of an in-process VM. Grows as host iterations land features:
/// Loaded programs own their VM context and initialization state.
pub(crate) struct HostState {
    /// Resolved configuration retained for diagnostics and the future
    /// hot-reload / multi-instance work that will need to inspect the
    /// originally-supplied options.
    #[allow(dead_code)]
    pub config: ResolvedConfig,
    /// `Arc` so `z42_host_invoke` can take its module out from under the
    /// `HOST` lock and run user code without holding it.
    pub modules: Vec<std::sync::Arc<super::LoadedProgram>>,
    pub entries: Vec<HostEntry>,
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

/// Publish the validated session and its sinks under the same lifecycle lock.
/// `publish_sinks` only installs prepared callbacks; it must not call host code.
pub(crate) fn try_initialize(
    config: ResolvedConfig,
    publish_sinks: impl FnOnce(&ResolvedConfig),
) -> InitOutcome {
    let mut guard = match HOST.write() {
        Ok(g) => g,
        Err(_) => return InitOutcome::LockPoisoned,
    };
    if guard.is_some() {
        return InitOutcome::AlreadyInit;
    }
    publish_sinks(&config);
    *guard = Some(HostState {
        config,
        modules: Vec::new(),
        entries: Vec::new(),
    });
    InitOutcome::Initialized
}

/// Outcome of `try_shutdown`.
pub(crate) enum ShutdownOutcome {
    Done,
    NotInit,
    LockPoisoned,
}

/// Withdraw the session and its sinks under the lifecycle lock, then destroy
/// resources after releasing it. `clear_sinks` must not invoke host code.
pub(crate) fn try_shutdown(clear_sinks: impl FnOnce()) -> ShutdownOutcome {
    let mut guard = match HOST.write() {
        Ok(g) => g,
        Err(_) => return ShutdownOutcome::LockPoisoned,
    };
    if guard.is_none() {
        return ShutdownOutcome::NotInit;
    }
    clear_sinks();
    let retired = guard.take();
    drop(guard);
    // Component/resource destructors may re-enter the host.
    drop(retired);
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
/// touches `modules` / `entries`.
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
