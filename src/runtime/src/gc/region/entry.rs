//! `RegionEntry` — one GC slot: the user value plus its GC metadata.
//!
//! shrink-object-footprint: split out of `region.rs` (1085 lines, and the
//! line-limit ratchet forbids an already-over-limit file from growing).
//! Re-exported from the parent, so `gc::region::RegionEntry` is unchanged.

use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU8, Ordering};

use parking_lot::Mutex;

use super::super::types::FinalizerFn;

/// Per-object slot inside a `Region<T>`. Holds the user data plus GC
/// metadata — 24 bytes of it besides the value's `Mutex` (M8: young-set membership lives in
/// the region's per-chunk bitmap, not in the entry). Address stability: once a `RegionEntry` is initialized
/// inside a chunk, its `&self` reference remains valid until the
/// owning chunk's Box is dropped (which happens only when the Region
/// itself drops — never during normal sweep cycles).
pub struct RegionEntry<T> {
    /// User value. `Mutex` provides per-entry locking (preserves the
    /// multi-threading concurrency model). Access via
    /// `entry.value.lock()` from `GcRef::borrow` / `borrow_mut`.
    pub(crate) value: Mutex<T>,

    /// Mark bit (add-mark-sweep-collector). CAS
    /// from 0 to 1 by the mark phase. Sweep resets to 0
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

    /// Self-location within the owning Region: chunk index and slot index. Lets the
    /// `MagrGC::finalize_now` path tombstone + recycle this slot given only a
    /// `&RegionEntry<T>` (no separate handle needed), and the write barrier find the card.
    /// Set at construction; immutable for the entry's lifetime (a slot keeps its location
    /// across reuse). Read through [`Self::location`].
    ///
    /// Split into a `u32` and a `u8` rather than kept as a `(u32, u16)` tuple: the tuple pads
    /// to 8 bytes, and those spare bytes are what pushed the header past 24 (M8). The slot
    /// index fits a byte because a chunk holds `CHUNK_SIZE` = 256 slots (asserted below).
    /// `loc_chunk == u32::MAX` marks a standalone (test-only, not in any region) entry.
    pub(crate) loc_chunk: u32,
    pub(crate) loc_entry: u8,

    /// **add-gc-softref (2026-05-26)**: count of live `SoftGcRef<T>`
    /// handles pointing at this entry. > 0 means the entry is
    /// soft-referenced; the GC revive pass may re-mark it before sweep
    /// when heap pressure is below the soft threshold. Incremented by
    /// `SoftGcRef::new`, decremented by `SoftGcRef::drop`. Uses
    /// `SeqCst` ordering to keep soft-ref count visible across threads
    /// (the handles are created and dropped on mutator threads).
    pub(crate) soft_ref_count: AtomicU32,

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

/// **add-generational-gc P0 (2026-05-22)**: the **default** number of minor GCs an entry must
/// survive before being promoted to the old generation (leaving the young set). 2 is the
/// industry-standard Java tenure.
///
/// **add-promotion-age-knob (2026-09-08)**: this is now a default, not the value. Each heap
/// reads `Z42_GC_PROMOTION_AGE` **once at construction** and caches it (`ArcMagrGC` /
/// `Region` / `VarRegion` all carry a `promotion_age` field); nothing reads a global on the
/// write-barrier hot path. Tests keep using this constant as "the default age".
///
/// (An older comment claimed `Z42_GC_TENURE` configured it — that env var never existed
/// anywhere in the repo.)
/// **retune-gc-nursery-and-promotion-age (2026-09-11)**: 2 → **3**, in the same breath as
/// `DEFAULT_NURSERY_BYTES` 32M → 16M — see that constant for the measurements. The two move
/// together on purpose: a minor promotes whatever survives it, so halving the nursery halves
/// how much allocation an object must outlive to be promoted, and objects that would have died
/// young end up in the old generation where only a major can reclaim them. Raising the age is
/// the direct cure, and without it the smaller nursery **doubled** peak RSS on `12_gc_churn`
/// (198 → 405 MB); with it that rung reads 142 MB.
///
/// ⚠️ **3 is the ceiling.** `gen_age` is packed into two spare bits of
/// `GcBlockHeader::type_tag` (`var_region::MAX_GEN_AGE`), and `var_region.rs` static-asserts
/// `PROMOTION_THRESHOLD <= MAX_GEN_AGE`. So the default now sits at the top of the knob's
/// range: `Z42_GC_PROMOTION_AGE` can still be lowered, but no longer raised. Going higher
/// needs the age to find more room — `size_class` has one spare bit (its largest index is 64),
/// or the header grows, which costs far more than it sounds (see `chunk::class_for`).
pub const PROMOTION_THRESHOLD: u8 = 3;

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
            loc_chunk:      location.0,
            loc_entry:      location.1 as u8,
            soft_ref_count: AtomicU32::new(0),
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

    /// `(chunk index, slot index)` of this entry in its region; the chunk is `u32::MAX` for a
    /// standalone entry (see [`Self::loc_chunk`]).
    #[inline]
    pub(crate) fn location(&self) -> (u32, u16) {
        (self.loc_chunk, self.loc_entry as u16)
    }

    /// **add-generational-gc P0 (2026-05-22)**: read current gen_age.
    /// Used by write barrier override under `GenerationalMarkSweep`
    /// mode to detect cross-gen writes.
    #[inline]
    pub fn gen_age(&self) -> u8 {
        self.gen_age.load(Ordering::Relaxed)
    }

    /// Mark this entry for `kind` (see [`crate::gc::refs::MarkKind`]). Returns `true` iff this
    /// call made the transition (first to mark in the current cycle).
    #[inline]
    pub fn mark(&self, kind: crate::gc::refs::MarkKind) -> bool {
        crate::gc::refs::mark_cell(&self.marked, kind)
    }

    /// Whether this entry carries `kind`'s mark.
    #[inline]
    pub fn is_marked(&self, kind: crate::gc::refs::MarkKind) -> bool {
        crate::gc::refs::is_marked_cell(&self.marked, kind)
    }

    /// Clear the minor mark only (the minor sweep on a survivor).
    #[inline]
    pub fn clear_minor_mark(&self) {
        crate::gc::refs::clear_minor_cell(&self.marked);
    }

    /// The stored major epoch (0 = never major-marked). Invariant checks only.
    #[inline]
    pub fn major_epoch(&self) -> u8 {
        crate::gc::refs::major_epoch_of_cell(&self.marked)
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

// The location's slot byte must be able to name every slot of a chunk.
const _: () = assert!(super::CHUNK_SIZE <= 256, "RegionEntry::loc_entry is a u8");
