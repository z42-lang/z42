//! User-lock contention probes (add-concurrency-probes, script-profiling P1b).
//!
//! Blocking user-lock acquires (`Std.Threading.Mutex.Lock` / `RwLock.Read` /
//! `.Write`, in [`super::sync`]) route through these helpers. Under the
//! `profile-contention` cargo feature each does a `try_*` first: a miss means
//! the lock was already held → bump `VmCore.lock_contentions` and time the
//! blocking wait into `VmCore.lock_wait_us`. Without the feature the helper is a
//! plain forwarding call the optimizer inlines away, so the default build's
//! acquire path is unchanged (zero cost).
//!
//! Split into its own file (rather than inline in `sync.rs`) to keep `sync.rs`
//! from growing past the 500-line file limit.

use crate::metadata::Value;
use crate::vm_context::VmContext;
use std::sync::Arc;

#[cfg(feature = "profile-contention")]
pub(super) fn contended_lock<'a>(
    ctx: &VmContext,
    arc: &'a Arc<parking_lot::Mutex<Value>>,
) -> parking_lot::MutexGuard<'a, Value> {
    if let Some(g) = arc.try_lock() {
        return g;
    }
    record_contention(ctx, || arc.lock())
}

#[cfg(not(feature = "profile-contention"))]
#[inline(always)]
pub(super) fn contended_lock<'a>(
    _ctx: &VmContext,
    arc: &'a Arc<parking_lot::Mutex<Value>>,
) -> parking_lot::MutexGuard<'a, Value> {
    arc.lock()
}

#[cfg(feature = "profile-contention")]
pub(super) fn contended_read<'a>(
    ctx: &VmContext,
    arc: &'a Arc<parking_lot::RwLock<Value>>,
) -> parking_lot::RwLockReadGuard<'a, Value> {
    if let Some(g) = arc.try_read() {
        return g;
    }
    record_contention(ctx, || arc.read())
}

#[cfg(not(feature = "profile-contention"))]
#[inline(always)]
pub(super) fn contended_read<'a>(
    _ctx: &VmContext,
    arc: &'a Arc<parking_lot::RwLock<Value>>,
) -> parking_lot::RwLockReadGuard<'a, Value> {
    arc.read()
}

#[cfg(feature = "profile-contention")]
pub(super) fn contended_write<'a>(
    ctx: &VmContext,
    arc: &'a Arc<parking_lot::RwLock<Value>>,
) -> parking_lot::RwLockWriteGuard<'a, Value> {
    if let Some(g) = arc.try_write() {
        return g;
    }
    record_contention(ctx, || arc.write())
}

#[cfg(not(feature = "profile-contention"))]
#[inline(always)]
pub(super) fn contended_write<'a>(
    _ctx: &VmContext,
    arc: &'a Arc<parking_lot::RwLock<Value>>,
) -> parking_lot::RwLockWriteGuard<'a, Value> {
    arc.write()
}

/// Count one contended acquire and time how long the blocking `acquire` closure
/// takes. Shared by the Mutex / RwLock contended paths above.
#[cfg(feature = "profile-contention")]
fn record_contention<G>(ctx: &VmContext, acquire: impl FnOnce() -> G) -> G {
    use std::sync::atomic::Ordering::Relaxed;
    ctx.core.lock_contentions.fetch_add(1, Relaxed);
    let t = std::time::Instant::now();
    let g = acquire();
    ctx.core.lock_wait_us.fetch_add(t.elapsed().as_micros() as u64, Relaxed);
    g
}
