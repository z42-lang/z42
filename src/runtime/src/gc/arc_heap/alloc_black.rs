//! Marking-period **allocate-black** (investigate-concurrent-gc-stale-mark-race 3.2a).
//!
//! ## Why
//!
//! Under `GcMode::ConcurrentMarkSweep`, an object allocated *during* a cycle and
//! reachable only from a frame reg used to be shaded by nothing:
//!
//! - `write_barrier_field` only shades refs **stored into a heap field**;
//! - `snapshot_roots_into_mark_queue` does walk frame regs, but the concurrent
//!   path runs it once (Phase 1) and never re-scans roots before the Phase 6
//!   sweep — unlike the STW path, whose `mark_phase` re-scans at collect time.
//!
//! So sweep tombstoned it while the mutator still held a live handle. Birthing
//! such objects marked makes them conservatively survive the cycle they were
//! born in; sweep clears survivors' marks, so they leave it white and the *next*
//! cycle can reclaim them. Retention is one cycle, not forever.
//!
//! ## Why the window has to include `Marking`
//!
//! `request_handshake_pause` flips the phase to `Marking` and *then* waits for
//! mutators to park, so a mutator can still allocate in that window before it
//! reaches its next safepoint. Shading only under `ConcurrentMarking` — the
//! obvious reading — still loses objects. Proved under exhaustive interleaving
//! by `tests/gc_alloc_black_loom.rs`
//! (`allocate_black_on_concurrent_marking_alone_is_insufficient`).
//!
//! Rather than track the phase, the window is one flag opened by
//! `collect_cycles_with_context` when it commits to a cycle and closed right
//! after `sweep_phase`, with mutators still parked.
//!
//! ## Cost
//!
//! One `Relaxed` load per allocation, at the five allocation chokepoints
//! (`finish_alloc` and `alloc_array_obj`, each with a TLAB and an ambient path,
//! plus `acquire_var_block` for every `region_var` block — strings, closures and
//! array backings are mark-swept by `VarRegion::sweep` just like region entries).
//! The flag is `false` for the entire life of a `StwMarkSweep` heap, which is
//! the production default.
//!
//! ## Why shading *after* publishing the entry is sound
//!
//! The ambient (non-TLAB) paths publish the region entry and only then shade it.
//! Sweep runs only with this thread parked, and a thread that is running has by
//! definition not parked, so the collector cannot have got past its handshake —
//! it cannot be sweeping in that gap. On the TLAB paths the question does not
//! arise at all: a TLAB entry is invisible to the collector until the owning
//! thread retires its chunk, which happens at park.

use crate::metadata::Value;

impl crate::gc::arc_heap::ArcMagrGC {
    /// Open the window. Called once `collect_cycles_with_context` holds the
    /// pause and has committed to running a concurrent cycle.
    pub(crate) fn begin_alloc_black(&self) {
        self.alloc_black.store(true, std::sync::atomic::Ordering::Release);
    }

    /// Close the window. Only valid after `sweep_phase` has finished and while
    /// mutators are still parked, so nothing can allocate in between.
    pub(crate) fn end_alloc_black(&self) {
        self.alloc_black.store(false, std::sync::atomic::Ordering::Release);
    }

    #[inline]
    pub(super) fn allocating_black(&self) -> bool {
        self.alloc_black.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Shade a freshly allocated region-entry value if the window is open.
    /// Returns the value so call sites stay expression-shaped.
    #[inline]
    pub(super) fn shade_newborn(&self, value: Value) -> Value {
        if self.allocating_black() {
            Self::mark_if_unmarked(&value);
        }
        value
    }

    /// `shade_newborn` for a `region_var` block (string / closure / array
    /// backing). Same one-cycle retention, same sweep-clears-it semantics.
    #[inline]
    pub(super) fn shade_var_newborn(
        &self,
        vref: crate::gc::var_region::VarGcRef,
    ) -> crate::gc::var_region::VarGcRef {
        if self.allocating_black() {
            vref.mark();
        }
        vref
    }
}
