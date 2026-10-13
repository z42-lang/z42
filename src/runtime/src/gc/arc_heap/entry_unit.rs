//! **Entry-capped gate units**: the default gates are stated in **allocations** (what a
//! collection's work grows with) and capped at their historical **byte** values (what their memory
//! is bounded by). A gate trips at whichever bound the program reaches first.
//!
//! # Why
//!
//! A collection's work is per object — a minor marks and sweeps entries, a major traces the live
//! entries and sweeps the dead ones — while the gates that decide *when* to collect were all bytes:
//! the default nursery (16 MB), the major allowance's floor (4 nurseries), the slice interval and
//! the incremental cycle's growth target (fractions of those). An object model that packs the same
//! objects into fewer bytes (object model R8: an empty object 32 → 16 B, `{int}` 40 → 24 B) then
//! admits more objects per gate, and every decision moves: `09_alloc_ctorless`'s first full mark
//! covered 1.05 M objects instead of 0.84 M, `12_gc_churn` STW's two majors each swept 42% more
//! blocks.
//!
//! # The rule
//!
//! ```text
//!   bytes/allocation  = Δ(used + reclaimed) / Δallocations     ← sampled at every policy consult, EWMA
//!   unit              = min(configured nursery bytes, UNIT_ENTRIES × bytes/allocation)
//!   nursery (default) = unit;  pause-budget bounds = [min(4 MB, unit/4), min(64 MB, 4 × unit)]
//!   major floor       = 4 × unit;  slice interval ≤ unit / 4
//! ```
//!
//! [`UNIT_ENTRIES`] is 16 MB at 64 B per allocation, so a program whose average allocation is at
//! least 64 B (one cache line — `09_alloc_ctorless`'s node) keeps the byte gates exactly; only
//! programs of smaller objects get gates that are smaller in bytes and equal in objects. Memory-shaped
//! limits stay in bytes and are untouched: the allowance's `live × 0.33`, the soft cap, `MAX_NURSERY`.
//! The reference size is a policy choice, not a measurement: `09_alloc_ctorless` (64 B nodes) and
//! `13_gc_large_heap` (82–86 B per allocation) sit at or above it and are unchanged; a larger
//! reference would make them object-bound too, at a measured wall-time cost (see `gc-tuning.md`).
//!
//! **Allocations, not young entries.** `allocations` is a lock-free counter every allocation path
//! already bumps, so a sample costs two loads on a path that already takes the heap mutex. An array
//! counts once (its header and its element block are one allocation), the same way its bytes are
//! charged once.
//!
//! **Before the first sample** the size is unknown, so the first consult is armed at a **probe**
//! ([`UNIT_ENTRIES`] at the smallest block size, 4 MB): early enough that no layout can pass the
//! entry bound unseen, and the consult that lands there measures the size and re-arms at the real
//! gate. Off when the nursery is configured explicitly (`Z42_GC_NURSERY_BYTES`, or a test sizing it):
//! a stated byte nursery is a manual setting and is taken as is.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// The default nursery, in allocations: [`DEFAULT_NURSERY_BYTES`](super::auto_collect) at 64 B each.
pub(super) const UNIT_ENTRIES: u64 = 256 * 1024;

/// The smallest variable-length block (`MIN_BLOCK`): no allocation is smaller, so a gate of
/// `UNIT_ENTRIES` allocations is never smaller than `UNIT_ENTRIES × this` bytes.
const MIN_ENTRY_BYTES: u64 = 16;

/// Allocations a sample needs before it is read (a handful of large ones is not a size).
const MIN_SAMPLE: u64 = 4096;

/// The allocation-size reading and whether the entry bound applies at all.
#[derive(Debug)]
pub(crate) struct EntryUnit {
    enabled: AtomicBool,
    /// EWMA of bytes per allocation; `0` until the first sample.
    bytes_per_entry: AtomicU64,
    /// Cumulative allocated bytes / allocations at the last sample.
    mark_bytes: AtomicU64,
    mark_allocs: AtomicU64,
}

impl EntryUnit {
    pub(crate) fn new(enabled: bool) -> Self {
        EntryUnit {
            enabled: AtomicBool::new(enabled),
            bytes_per_entry: AtomicU64::new(0),
            mark_bytes: AtomicU64::new(0),
            mark_allocs: AtomicU64::new(0),
        }
    }

    /// A byte nursery was stated (tests): the gates are bytes only from now on.
    #[cfg(test)]
    pub(crate) fn disable(&self) {
        self.enabled.store(false, Ordering::Relaxed);
    }

    /// Fold the allocation stream since the last sample into the size reading. `allocated` is the
    /// cumulative bytes ever allocated, `allocations` the cumulative count.
    pub(crate) fn sample(&self, allocated: u64, allocations: u64) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        let da = allocations.saturating_sub(self.mark_allocs.load(Ordering::Relaxed));
        if da < MIN_SAMPLE {
            return;
        }
        let db = allocated.saturating_sub(self.mark_bytes.load(Ordering::Relaxed));
        self.mark_allocs.store(allocations, Ordering::Relaxed);
        self.mark_bytes.store(allocated, Ordering::Relaxed);
        let s = (db / da).max(1);
        let prev = self.bytes_per_entry.load(Ordering::Relaxed);
        self.bytes_per_entry.store(if prev == 0 { s } else { (3 * prev + s) / 4 }, Ordering::Relaxed);
    }

    /// `bytes`, capped at `entries` allocations of the measured size. Unchanged while the bound is
    /// off or nothing has been measured yet.
    #[inline]
    pub(crate) fn cap(&self, bytes: u64, entries: u64) -> u64 {
        match self.reading() {
            0 => bytes,
            bpe => bytes.min(entries.saturating_mul(bpe).max(1)),
        }
    }

    /// The gate to arm: `gate`, or — while the size is unmeasured — no further than the probe.
    #[inline]
    pub(crate) fn probe(&self, gate: u64) -> u64 {
        if self.enabled.load(Ordering::Relaxed) && self.bytes_per_entry.load(Ordering::Relaxed) == 0 {
            gate.min(UNIT_ENTRIES * MIN_ENTRY_BYTES)
        } else {
            gate
        }
    }

    /// The size reading in force (`0`: off or unmeasured), for the gates and diagnostics.
    #[inline]
    pub(crate) fn reading(&self) -> u64 {
        if self.enabled.load(Ordering::Relaxed) { self.bytes_per_entry.load(Ordering::Relaxed) } else { 0 }
    }
}

impl crate::gc::arc_heap::ArcMagrGC {
    /// Sample the allocation size at a policy consult (the caller read `reclaimed` under the heap
    /// mutex), and — while the pause budget has not sized the nursery yet, or is off — keep the
    /// default nursery at the entry-capped unit.
    pub(super) fn sample_entry_unit(&self, used: u64, reclaimed: u64) {
        let allocations = self.allocations.load(Ordering::Relaxed);
        self.entry_unit.sample(used.saturating_add(reclaimed), allocations);
        if !self.pause_budget.has_sized() {
            self.nursery_bytes.store(self.allowance_unit(), Ordering::Relaxed);
        }
    }

    /// The pause budget's nursery bounds, entry-capped like the unit (see the module note).
    pub(super) fn nursery_bounds(&self) -> super::pause_budget::Bounds {
        use super::pause_budget::{MAX_NURSERY, MIN_NURSERY};
        super::pause_budget::Bounds {
            min: self.entry_unit.cap(MIN_NURSERY, UNIT_ENTRIES / 4),
            max: self.entry_unit.cap(MAX_NURSERY, UNIT_ENTRIES * 4),
        }
    }
}

#[cfg(test)]
#[path = "entry_unit_tests.rs"]
mod entry_unit_tests;
