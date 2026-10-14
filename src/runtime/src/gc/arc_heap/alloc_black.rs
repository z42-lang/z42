//! Marking-period **allocate-black**.
//!
//! ## Why
//!
//! An incremental major (`incremental.rs`) snapshots the roots once, when the cycle opens, and
//! lets mutators run between its slices. An object allocated *during* the cycle and reachable
//! only from a frame reg is shaded by nothing:
//!
//! - the SATB barrier (`gc::satb`) only records the **old** value of an overwritten heap slot;
//! - `snapshot_roots_into_mark_queue` walks frame regs, but only at the cycle's opening — the
//!   roots are never re-scanned before the sweep.
//!
//! So sweep would tombstone it while the mutator still holds a live handle. Birthing such
//! objects with the cycle's mark makes them conservatively survive the cycle they were born in;
//! the next cycle's epoch whitens them again, so retention is one cycle, not forever.
//!
//! The window is one flag, opened when the cycle opens (`open_incremental_cycle`) and closed at
//! its wrap-up, after the last sweep slice. A one-shot STW major never opens it: mutators are
//! parked for the whole cycle, so nothing can be born inside it.
//!
//! ## Cost
//!
//! One `Relaxed` load per allocation, at the one allocation chokepoint every block passes —
//! `acquire_var_block` (objects, arrays, strings, closures), on both its TLAB and its locked path.
//! The flag is `false` whenever no incremental cycle is open.
//!
//! ## Why shading a block the region already lists is sound
//!
//! The locked path carves the block (visible to region walks) and only then shades it.
//! Sweep runs only with this thread parked, and a thread that is running has by
//! definition not parked, so the collector cannot be sweeping in that gap. On the
//! TLAB paths the question does not arise at all: a TLAB block is invisible to the
//! collector until the owning thread retires its chunk, which happens at park.

impl crate::gc::arc_heap::ArcMagrGC {
    /// Open the window. Called when an incremental major cycle opens.
    pub(crate) fn begin_alloc_black(&self) {
        self.alloc_black.store(true, std::sync::atomic::Ordering::Release);
    }

    /// Close the window. Only valid after the cycle's sweep has finished and
    /// while mutators are still parked, so nothing can allocate in between.
    pub(crate) fn end_alloc_black(&self) {
        self.alloc_black.store(false, std::sync::atomic::Ordering::Release);
    }

    #[inline]
    pub(super) fn allocating_black(&self) -> bool {
        self.alloc_black.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Shade a freshly allocated block (object / array / string / closure) if the window is
    /// open: it is born with the cycle's mark (one-cycle retention).
    #[inline]
    pub(super) fn shade_var_newborn(
        &self,
        vref: crate::gc::var_region::VarGcRef,
    ) -> crate::gc::var_region::VarGcRef {
        if self.allocating_black() {
            vref.mark(self.major_mark());
        }
        vref
    }
}
