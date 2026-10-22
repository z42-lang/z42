//! Thread-local allocation buffer (TLAB) — chunk-exclusive per-thread GC
//! allocation (add-gc-tlab, stage 2).
//!
//! # Why
//!
//! `ArcMagrGC` is one shared heap; every mutator thread allocates into it. The
//! pre-TLAB hot path took the per-region `Mutex` on **every** `new`, so N
//! parallel mutators serialized on that lock — parallel compilation went
//! *slower* with more threads. A TLAB gives each thread exclusive write
//! ownership of a whole region *chunk* (design D1 "chunk 独占"): the thread
//! bump-fills it lock-free, and only touches the shared region lock once per
//! *chunk* (borrow) and once per *retire* (merge the filled prefix back).
//!
//! # Ownership model (thread-local, not on VmContext)
//!
//! The [`Tlab`] lives in a `thread_local!` cell, so it is inherently
//! per-OS-thread and every access point runs **on the owning thread**:
//! - allocation fast path (`acquire_var_block`),
//! - retire-on-park at a GC safepoint (the mutator parks *itself*),
//! - retire at `VmContext::drop` (runs on the owner thread).
//!
//! This sidesteps threading a `&VmContext` through the `MagrGC::alloc_*` trait
//! boundary (which only has `&self`), and keeps `VmContext` `Send` (a `Tlab`
//! holds raw chunk pointers and is `!Send`, but a `thread_local!` never crosses
//! threads so that is fine).
//!
//! # Heap binding
//!
//! A `Tlab`'s live claims belong to exactly one heap (identified by
//! [`MagrGC::heap_epoch`]). `heap_epoch == 0` means "unbound" (no live claims).
//! The fast path binds an empty TLAB to the current heap; if a TLAB still holds
//! claims for a *different* heap (only reachable in multi-heap cargo tests that
//! don't drop their `VmContext` between heaps), the allocation falls back to the
//! ambient locked path rather than mixing regions. `VmContext::drop` retires the
//! TLAB, returning it to the unbound state for the next context on this thread.

use std::cell::UnsafeCell;

use super::var_region::{VarChunkClaim, VarHoleStash};

/// Per-thread allocation buffer: a borrowed chunk claim on the heap's variable-length region
/// (`region_var`: objects, arrays, strings, closures) plus the dead slots taken for reuse.
pub(crate) struct Tlab {
    /// Active claim on `region_var` (`Value::Object` / `BoxedStruct` / `Array` / `Str` /
    /// `Closure`).
    pub(crate) var: Option<VarChunkClaim>,
    /// **M12**: dead slots of `region_var` taken for reuse, per size class
    /// (`var_region::holes`). Inline so the fast path's class test is one load away; its per-class
    /// table is allocated at the first refill.
    pub(crate) holes: VarHoleStash,
    /// Heap epoch the current claims belong to; `0` = unbound (no live claims).
    pub(crate) heap_epoch: u64,
}

impl Tlab {
    const fn empty() -> Self {
        Self { var: None, holes: VarHoleStash::new(), heap_epoch: 0 }
    }

    /// True when the TLAB holds no live claims (safe to (re)bind to any heap).
    #[inline]
    pub(crate) fn is_unbound(&self) -> bool {
        self.var.is_none() && !self.holes.holds_slots()
    }
}

/// The claims a thread set aside when it parked for a blocking native call
/// ([`crate::gc::NativeParkGuard`]): they stay borrowed (the thread may continue filling
/// them after the call) unless a collection begins meanwhile — then the collector retires
/// them on the parked thread's behalf, exactly as the thread would have at a safepoint park.
pub(crate) struct ParkedTlab {
    pub(crate) var: Option<VarChunkClaim>,
    /// M12: the dead slots this thread took (counted live + young at take time). Retired with
    /// the claims (`VarRegion::retire_holes`: unused ones back on the free list), so a collection
    /// never runs while a parked thread holds slots off the free list.
    pub(crate) holes: VarHoleStash,
    pub(crate) heap_epoch: u64,
}

// SAFETY: the claims and the hole stash hold raw pointers into chunks the heap owns. A parked thread's claims
// move to the collector thread only while the owner is parked (it neither allocates nor
// touches them — `debug_assert_not_native_parked`), and back to the owner only once the
// phase is `Idle`; the parking handshake (`gc_phase` lock) orders the two sides' accesses.
unsafe impl Send for ParkedTlab {}

/// Per-`VmContext` holder of its [`ParkedTlab`] while the thread is native-parked
/// (`MagrGC::park_thread_tlab` / `unpark_thread_tlab` / `retire_parked_tlab`).
#[derive(Default)]
pub struct ParkedTlabSlot {
    inner: parking_lot::Mutex<Option<ParkedTlab>>,
}

impl ParkedTlabSlot {
    pub(crate) fn put(&self, p: ParkedTlab) -> Option<ParkedTlab> { self.inner.lock().replace(p) }
    pub(crate) fn take(&self) -> Option<ParkedTlab> { self.inner.lock().take() }
}

impl std::fmt::Debug for ParkedTlabSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ParkedTlabSlot")
    }
}

/// Per-thread TLAB cell: the arm count + the TLAB, behind one `UnsafeCell` so the
/// hot path does a **single** thread-local access and no runtime borrow check.
struct TlabCell {
    /// Arm count (see [`arm`]). The TLAB fast path is used only while `> 0`.
    armed: u32,
    tlab: Tlab,
}

thread_local! {
    /// This thread's TLAB cell. `UnsafeCell` (not `RefCell`) because the TLAB is
    /// **owner-thread-exclusive** and the allocation fast path is **non-reentrant**
    /// — object construction + `borrow_chunk`/`retire_chunk` never allocate a GC
    /// object while the `&mut Tlab` is held, so no aliasing `&mut` can form. This
    /// removes the per-alloc `RefCell` borrow-flag check.
    ///
    /// `armed` starts 0: threads WITHOUT a `VmContext` (cargo GC unit tests that
    /// drive `ArcMagrGC` directly, ambient `Str::new` before any VM) stay unarmed
    /// and keep the pre-TLAB locked path — so region-internal GC unit tests, which
    /// observe liveness right after `alloc` with no intervening retire/safepoint,
    /// behave exactly as before. `VmContext::new*` arms; `VmContext::drop` disarms.
    static TLAB: UnsafeCell<TlabCell> =
        const { UnsafeCell::new(TlabCell { armed: 0, tlab: Tlab::empty() }) };
}

/// **add-gc-tlab (stage 2)**: arm this thread for TLAB allocation (called by
/// `VmContext::new*`). Balanced by [`disarm`] (nesting count).
#[inline]
pub(crate) fn arm() {
    // SAFETY: single-threaded access to this thread's own cell; no `&mut Tlab`
    // is held across this call (arm runs at VmContext construction, not mid-alloc).
    TLAB.with(|c| unsafe { (*c.get()).armed = (*c.get()).armed.saturating_add(1) });
}

/// **add-gc-tlab (stage 2)**: disarm this thread (called by `VmContext::drop`
/// after retiring the TLAB). Saturating so an unbalanced call can't underflow.
#[inline]
pub(crate) fn disarm() {
    TLAB.with(|c| unsafe { (*c.get()).armed = (*c.get()).armed.saturating_sub(1) });
}

/// True when this thread has an active `VmContext` and should use the TLAB.
#[inline]
pub(crate) fn is_armed() -> bool {
    // SAFETY: plain read of this thread's own cell.
    TLAB.with(|c| unsafe { (*c.get()).armed > 0 })
}

/// Run `f` with a mutable reference to the current thread's TLAB. Non-reentrant
/// by contract (the alloc fast path never re-enters allocation while holding the
/// `&mut Tlab`), so the `UnsafeCell` deref is sound.
#[inline]
pub(crate) fn with_current_tlab<R>(f: impl FnOnce(&mut Tlab) -> R) -> R {
    // SAFETY: owner-thread-exclusive, non-reentrant (see the `TLAB` docs).
    TLAB.with(|c| f(unsafe { &mut (*c.get()).tlab }))
}
