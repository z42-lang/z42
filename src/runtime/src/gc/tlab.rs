//! Thread-local allocation buffer (TLAB) — chunk-exclusive per-thread GC
//! allocation (add-gc-tlab, 2026-08-29, stage 2).
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
//! - allocation fast path ([`ArcMagrGC::finish_alloc`] / `alloc_array_obj`),
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

use std::cell::{Cell, RefCell};

use super::region::ChunkClaim;
use super::var_region::VarChunkClaim;
use crate::metadata::types::ArrayObj;
use crate::metadata::ScriptObject;

/// Per-thread allocation buffer: one borrowed chunk claim per region the fast path
/// serves — the two fixed-size regions (object + array) and, since stage 3, the
/// variable-length region (`region_var`: strings / closures / array backings).
pub(crate) struct Tlab {
    /// Active claim on `region_object` (`Value::Object` / boxed struct).
    pub(crate) obj: Option<ChunkClaim<ScriptObject>>,
    /// Active claim on `region_array` (`Value::Array`).
    pub(crate) arr: Option<ChunkClaim<ArrayObj>>,
    /// **stage 3**: active claim on `region_var` (`Value::Str` / `Closure` / var blocks).
    pub(crate) var: Option<VarChunkClaim>,
    /// Heap epoch the current claims belong to; `0` = unbound (no live claims).
    pub(crate) heap_epoch: u64,
}

impl Tlab {
    const fn empty() -> Self {
        Self { obj: None, arr: None, var: None, heap_epoch: 0 }
    }

    /// True when the TLAB holds no live claims (safe to (re)bind to any heap).
    #[inline]
    pub(crate) fn is_unbound(&self) -> bool {
        self.obj.is_none() && self.arr.is_none() && self.var.is_none()
    }
}

thread_local! {
    /// This thread's TLAB. Lazily bound on first allocation; retired (→ unbound)
    /// at each GC safepoint park and at `VmContext::drop`.
    static TLAB: RefCell<Tlab> = const { RefCell::new(Tlab::empty()) };

    /// **add-gc-tlab (stage 2)**: per-thread arm count. The TLAB fast path is
    /// used only while `> 0` — armed for a thread's lifetime by
    /// `VmContext::new*` and disarmed at `VmContext::drop` (nesting count, so
    /// sequential/nested contexts on one thread balance). Threads WITHOUT a
    /// `VmContext` (cargo GC unit tests that drive `ArcMagrGC` directly, ambient
    /// `Str::new` before any VM, etc.) stay unarmed and keep the pre-TLAB
    /// locked allocation path — so the region-internal GC unit tests, which
    /// observe liveness immediately after `alloc` with no intervening
    /// retire/safepoint, see the same behavior as before.
    static ARMED: Cell<u32> = const { Cell::new(0) };
}

/// **add-gc-tlab (stage 2)**: arm this thread for TLAB allocation (called by
/// `VmContext::new*`). Balanced by [`disarm`].
#[inline]
pub(crate) fn arm() {
    ARMED.with(|c| c.set(c.get().saturating_add(1)));
}

/// **add-gc-tlab (stage 2)**: disarm this thread (called by `VmContext::drop`
/// after retiring the TLAB). Saturating so an unbalanced call can't underflow.
#[inline]
pub(crate) fn disarm() {
    ARMED.with(|c| c.set(c.get().saturating_sub(1)));
}

/// True when this thread has an active `VmContext` and should use the TLAB.
#[inline]
pub(crate) fn is_armed() -> bool {
    ARMED.with(|c| c.get() > 0)
}

/// Run `f` with a mutable borrow of the current thread's TLAB. The borrow is
/// non-reentrant: the allocation fast path never re-enters allocation while it
/// holds this borrow (object construction + `borrow_chunk`/`retire_chunk` do not
/// allocate GC objects), so the `RefCell` cannot double-borrow.
#[inline]
pub(crate) fn with_current_tlab<R>(f: impl FnOnce(&mut Tlab) -> R) -> R {
    TLAB.with(|cell| f(&mut cell.borrow_mut()))
}
