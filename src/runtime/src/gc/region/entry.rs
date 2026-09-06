//! `RegionEntry` — one GC slot: the user value plus its GC metadata.
//!
//! shrink-object-footprint: split out of `region.rs` (1085 lines, and the
//! line-limit ratchet forbids an already-over-limit file from growing).
//! Re-exported from the parent, so `gc::region::RegionEntry` is unchanged.

use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU8, Ordering};

use parking_lot::Mutex;

use super::super::types::FinalizerFn;

/// Per-object slot inside a `Region<T>`. Holds the user data plus GC
/// metadata. Address stability: once a `RegionEntry` is initialized
/// inside a chunk, its `&self` reference remains valid until the
/// owning chunk's Box is dropped (which happens only when the Region
/// itself drops — never during normal sweep cycles).
pub struct RegionEntry<T> {
    /// User value. `Mutex` provides per-entry locking (preserves the
    /// multi-threading concurrency model). Access via
    /// `entry.value.lock()` from `GcRef::borrow` / `borrow_mut`.
    pub(crate) value: Mutex<T>,

    /// Mark bit (add-mark-sweep-collector + add-concurrent-gc). CAS
    /// from 0 to 1 by mark phase / write barrier. Sweep resets to 0
    /// on survivors. `Relaxed` ordering — visibility sync via the
    /// gc_phase Mutex / mark_queue Mutex established at sweep / drain
    /// boundaries.
    pub(crate) marked: AtomicU8,

    /// Tombstone flag. `true` while the slot holds a live user
    /// object; `false` after sweep reclaims it. `Acquire / Release`
    /// ordering pairs with `WeakGcRef::upgrade` reads + sweep writes
    /// (prevents reading half-tombstoned state).
    pub(crate) alive: AtomicBool,

    /// **add-generational-gc P0 (2026-05-22)**: generation age. 0 =
    /// young (fresh alloc); incremented at each minor GC the entry
    /// survives; >= `PROMOTION_THRESHOLD` means promoted to old gen.
    /// Lock-free atomic read for the write-barrier hot path
    /// (cross-gen detection). Promotion writes happen during STW
    /// minor sweep, so no race.
    pub(crate) gen_age: AtomicU8,

    /// Generation counter. Bumped on every tombstone. `GcRef` and
    /// `WeakGcRef` both record the generation at construction; access
    /// methods (`upgrade`, `borrow`) check the recorded generation
    /// matches the entry's current generation. Mismatch → entry was
    /// reclaimed + slot reused → return None / panic (per design D5).
    pub(crate) generation: AtomicU32,

    /// One-shot finalizer slot, as a **raw `Box<FinalizerFn>` pointer**
    /// (`null` = none). `swap(null)` gives the same fire-once `take()`
    /// semantics the sweep path relies on, atomically and lock-free.
    ///
    /// shrink-object-footprint P1: this was `Mutex<Option<FinalizerFn>>` =
    /// **24 bytes on every entry** (19% of a 128-byte `RegionEntry<ScriptObject>`)
    /// for a capability with **zero production registrations** — `grep
    /// register_finalizer` over `src/` hits only the trait, its impl, these
    /// accessors, and `arc_heap_tests/finalization.rs`. As an `AtomicPtr` it is
    /// 8 bytes, and only an entry that actually registers one pays the 16-byte
    /// box. Freed by this entry's `Drop`.
    pub(crate) finalizer: AtomicPtr<FinalizerFn>,

    /// **add-custom-allocator P2 (2026-05-22)**: self-location
    /// (chunk_idx, entry_idx) within the owning Region. Lets the
    /// `MagrGC::finalize_now` path tombstone + recycle this slot
    /// given only a `&RegionEntry<T>` (no separate handle needed).
    /// Set by `Region::alloc`; immutable thereafter for the entry's
    /// lifetime (a single slot keeps its location across reuse).
    /// fix-region-chunk-idx-u16-overflow (2026-08-21): chunk_idx widened u16→u32.
    /// A full 24-lib stdlib build bump-allocates past 65 535 chunks (× CHUNK_SIZE=256
    /// = 16.7M slots); the old u16 chunk index overflowed at `ci + 1`, wrapping
    /// `next_bump` to (0,0) → fresh allocations overwrote live chunk-0 objects →
    /// non-deterministic heap corruption. entry_idx stays u16 (CHUNK_SIZE ≤ 65 536).
    pub(crate) location: (u32, u16),

    /// **add-gc-softref (2026-05-26)**: count of live `SoftGcRef<T>`
    /// handles pointing at this entry. > 0 means the entry is
    /// soft-referenced; the GC revive pass may re-mark it before sweep
    /// when heap pressure is below the soft threshold. Incremented by
    /// `SoftGcRef::new`, decremented by `SoftGcRef::drop`. Uses
    /// `SeqCst` ordering to keep soft-ref count visible across threads
    /// (GC and mutator run concurrently in `ConcurrentMarkSweep`).
    pub(crate) soft_ref_count: AtomicU32,

    /// **fix-young-list-quadratic-sweep (2026-09-06)**: this entry's own index
    /// inside `Region::young_list`, or [`Self::NOT_IN_YOUNG_LIST`] when it is
    /// not listed (old, dead, or standalone). Turns `remove_from_young_list`
    /// from an O(young_list.len()) `position()` scan into an O(1)
    /// `swap_remove`, which is what made sweep O(dead x young) — a 137 s STW
    /// pause on a 230 MB heap, the reason automatic GC was never armed.
    ///
    /// Maintained **only** under the region lock (`push_young` /
    /// `remove_from_young_list` / `retire_chunk`), so the atomic is for
    /// field-through-`&self` mutation, not for cross-thread coordination —
    /// `Relaxed` throughout.
    pub(crate) young_idx: AtomicU32,
}

/// shrink-object-footprint P1: the finalizer slot owns a `Box<FinalizerFn>`
/// (raw pointer, so the entry stays 8 bytes wider instead of 24) — free it when
/// the entry itself goes away, or the `Arc<dyn Fn>` inside leaks.
impl<T> Drop for RegionEntry<T> {
    fn drop(&mut self) {
        let raw = *self.finalizer.get_mut();
        if !raw.is_null() {
            // SAFETY: non-null ⇒ from `Box::into_raw` in `set_finalizer`; `&mut self`
            // means no other reference can observe the slot.
            drop(unsafe { Box::from_raw(raw) });
        }
    }
}

/// **add-generational-gc P0 (2026-05-22)**: number of minor GCs an
/// entry must survive before being promoted to old generation
/// (removed from `young_list`). Default = 2 (industry-standard Java
/// tenure). Configurable via `Z42_GC_TENURE` env var (P3 wiring).
pub const PROMOTION_THRESHOLD: u8 = 2;

impl<T> RegionEntry<T> {
    /// Test / transitional constructor used by `GcRef::new` for
    /// standalone (no-Region) allocations. Wraps a fresh entry with
    /// generation=0, alive=true. See refs.rs for the lifetime model
    /// (intentional leak — process-wide static). `location` is set to
    /// `(u32::MAX, u16::MAX)` — sentinel meaning "not in any Region"
    /// so `finalize_now` skips free-list bookkeeping for these
    /// standalone entries.
    pub fn new_for_test(value: T) -> Self {
        Self::new(value, (u32::MAX, u16::MAX))
    }

    /// **add-gc-tlab (2026-08-29)**: `pub(crate)` so the TLAB fast-fill path
    /// (`gc/tlab.rs::ChunkClaim::fill`) can construct entries directly into a
    /// borrowed chunk's raw slots without the region lock. Ambient `Region::alloc`
    /// still calls it internally.
    pub(crate) fn new(value: T, location: (u32, u16)) -> Self {
        Self {
            value:          Mutex::new(value),
            marked:         AtomicU8::new(0),
            alive:          AtomicBool::new(true),
            gen_age:        AtomicU8::new(0),
            generation:     AtomicU32::new(0),
            finalizer:      AtomicPtr::new(std::ptr::null_mut()),
            location,
            soft_ref_count: AtomicU32::new(0),
            young_idx:      AtomicU32::new(Self::NOT_IN_YOUNG_LIST),
        }
    }

    /// shrink-object-footprint P1: install a finalizer, dropping any previous one.
    /// Fire-once semantics are unchanged — `take_finalizer` still swaps `null` in.
    pub(crate) fn set_finalizer(&self, fin: FinalizerFn) {
        let raw = Box::into_raw(Box::new(fin));
        let prev = self.finalizer.swap(raw, Ordering::AcqRel);
        if !prev.is_null() {
            // SAFETY: non-null ⇒ produced by `Box::into_raw` here, and the swap
            // gives this thread exclusive ownership of the old box.
            drop(unsafe { Box::from_raw(prev) });
        }
    }

    /// Take the finalizer, leaving the slot empty (fire-once).
    pub(crate) fn take_finalizer(&self) -> Option<FinalizerFn> {
        let raw = self.finalizer.swap(std::ptr::null_mut(), Ordering::AcqRel);
        if raw.is_null() {
            return None;
        }
        // SAFETY: see `set_finalizer` — the swap hands us sole ownership.
        Some(*unsafe { Box::from_raw(raw) })
    }

    /// Whether a finalizer is currently installed (no ownership transfer).
    pub(crate) fn has_finalizer(&self) -> bool {
        !self.finalizer.load(Ordering::Acquire).is_null()
    }

    /// **fix-young-list-quadratic-sweep**: sentinel for `young_idx` meaning
    /// "this entry is not in `Region::young_list`". `young_list` can never
    /// reach `u32::MAX` entries (a chunk index is itself a `u32`).
    pub(crate) const NOT_IN_YOUNG_LIST: u32 = u32::MAX;

    /// **fix-young-list-quadratic-sweep**: read this entry's `young_list`
    /// slot, or `None` when it is not listed.
    #[inline]
    pub(crate) fn young_idx(&self) -> Option<usize> {
        match self.young_idx.load(Ordering::Relaxed) {
            Self::NOT_IN_YOUNG_LIST => None,
            i => Some(i as usize),
        }
    }

    /// **fix-young-list-quadratic-sweep**: record this entry's `young_list`
    /// slot. Callers hold the region lock.
    #[inline]
    pub(crate) fn set_young_idx(&self, idx: usize) {
        self.young_idx.store(idx as u32, Ordering::Relaxed);
    }

    /// **fix-young-list-quadratic-sweep**: mark this entry as absent from
    /// `young_list`.
    #[inline]
    pub(crate) fn clear_young_idx(&self) {
        self.young_idx.store(Self::NOT_IN_YOUNG_LIST, Ordering::Relaxed);
    }

    /// **add-generational-gc P0 (2026-05-22)**: read current gen_age.
    /// Used by write barrier override under `GenerationalMarkSweep`
    /// mode to detect cross-gen writes.
    #[inline]
    pub fn gen_age(&self) -> u8 {
        self.gen_age.load(Ordering::Relaxed)
    }

    /// Atomically attempt to mark this entry (0 → 1). Returns `true`
    /// if this call won the CAS (first to mark in the current cycle).
    /// Used by mark phase BFS + concurrent barrier override.
    pub fn mark(&self) -> bool {
        self.marked
            .compare_exchange(0, 1, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    }

    /// Read current mark state. Used by sweep to decide retention.
    pub fn is_marked(&self) -> bool {
        self.marked.load(Ordering::Relaxed) != 0
    }

    /// Reset mark to 0. Used by sweep on survivors to prep next cycle.
    pub fn clear_mark(&self) {
        self.marked.store(0, Ordering::Relaxed);
    }

    /// Increment the soft-ref count for this entry. Called by `SoftGcRef::new`.
    #[inline]
    pub fn inc_soft_ref_count(&self) {
        self.soft_ref_count.fetch_add(1, Ordering::SeqCst);
    }

    /// Decrement the soft-ref count. Called by `SoftGcRef::drop`.
    #[inline]
    pub fn dec_soft_ref_count(&self) {
        self.soft_ref_count.fetch_sub(1, Ordering::SeqCst);
    }

    /// True when at least one `SoftGcRef` points to this entry.
    #[inline]
    pub fn has_soft_ref(&self) -> bool {
        self.soft_ref_count.load(Ordering::SeqCst) > 0
    }
}
