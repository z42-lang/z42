//! **adaptive-promotion / refactor (2026-09-12)**: `VarRegion`'s generational half — the
//! young list and everything that walks it. Split out of `var_region.rs` for the same
//! reason (and into the same shape as) `region/generation.rs` holds `Region<T>`'s: the
//! generational logic is a distinct concern from the allocator, and the parent file had
//! reached the 886-line limit.
//!
//! Nothing here changed in the move except [`VarRegion::set_promotion_age`], which is new.

use super::*;

impl VarRegion {
    /// **adaptive-promotion (2026-09-12)**: re-point the region at a new promotion age.
    /// Same window as `Region::set_promotion_age` — inside the minor sweep, after its mark.
    pub fn set_promotion_age(&mut self, age: u8) {
        self.promotion_age = age;
    }

    /// **fix-minor-gc-skips-var-region (2026-09-08)**: flip young-list maintenance, bringing
    /// the list in line with the new setting. Mirrors `Region<T>::set_generational` (#524).
    ///
    /// Turning it **on** rebuilds the list from every live young block, so a heap switched to
    /// `GenerationalMarkSweep` after it has already allocated still sees a complete young
    /// set. Turning it **off** drops the list and clears every block's membership bit.
    ///
    /// No-op when already in the requested state.
    pub fn set_generational(&mut self, generational: bool) {
        if self.generational == generational {
            return;
        }
        self.generational = generational;
        if !generational {
            for &ptr in &self.young_list {
                // SAFETY: young_list only holds chunk-owned block pointers.
                unsafe { ptr.as_ref() }.set_in_young(false);
            }
            self.young_list.clear();
            self.young_list.shrink_to_fit();
            return;
        }
        let threshold = self.promotion_age;
        let mut rebuilt = Vec::new();
        for ptr in self.all_blocks_iter() {
            // SAFETY: see `iterate_alive`.
            let header = unsafe { ptr.as_ref() };
            if header.is_alive() && header.gen_age() < threshold {
                header.set_in_young(true);
                rebuilt.push(ptr);
            }
        }
        self.young_list = rebuilt;
    }

    /// Visit every block currently listed as young, skipping ones already tombstoned (the
    /// young list uses lazy deletion — see the field docs). Read-only; ordering is
    /// allocation order within the list.
    pub fn iterate_young(&self, mut visit: impl FnMut(VarGcRef, &GcBlockHeader)) {
        for &ptr in &self.young_list {
            // SAFETY: young_list only ever holds chunk-owned block pointers, and chunks
            // outlive the region; reclaimed chunks purge their blocks from this list.
            let header = unsafe { ptr.as_ref() };
            if !header.is_alive() {
                continue;
            }
            visit(VarGcRef::pack(ptr, header.generation()), header);
        }
    }

    /// How many blocks the next minor GC would visit. Includes stale (tombstoned) entries
    /// that the next sweep will drop, so it is an upper bound on real young blocks — the
    /// same shape as `Region<T>::young_count`, and it is only used as the denominator of the
    /// minor-survival heuristic.
    pub fn young_count(&self) -> usize {
        self.young_list.len()
    }

    /// **fix-minor-gc-skips-var-region (2026-09-08)**: minor sweep. Walks only `young_list`
    /// (not `all_blocks`), which is what bounds minor cost at O(young):
    ///
    /// - marked → clear the mark and age it; on reaching `PROMOTION_THRESHOLD` it leaves the
    ///   young list (promotion is a label change — var blocks never move, see
    ///   `VarGcRef`'s address-as-identity contract);
    /// - unmarked → finalize + tombstone, crediting the bytes it was charged at alloc;
    /// - already tombstoned → a stale entry from lazy deletion; just drop it.
    ///
    /// The list is **compacted in place** to the survivors (`one-pass-var-sweep`, 2026-09-12),
    /// and the header's `IN_YOUNG_BIT` is cleared for everything that leaves, so a recycled
    /// slot knows to re-list itself.
    ///
    /// Old blocks are never visited — that is the definition of a minor collection. They are
    /// reachable as minor roots only through the dirty-card set.
    pub fn sweep_young(&mut self) -> (usize, u64) {
        let threshold = self.promotion_age;
        let mut reclaimed = 0usize;
        let mut credited: u64 = 0;
        // **one-pass-var-sweep (2026-09-12)**: own the list (`mem::take`) rather than borrow
        // it — `&mut self` is then free inside the loop, so the dead are tombstoned where the
        // decision is made, without the `to_reclaim` staging `Vec` that used to carry them
        // there (2.26 M pushes + read-backs and an un-reserved growth per minor) and without
        // the per-minor `Vec::with_capacity`.
        let mut young = std::mem::take(&mut self.young_list);
        let mut w = 0usize;

        for i in 0..young.len() {
            let ptr = young[i];
            // SAFETY: see `iterate_young`.
            let header = unsafe { ptr.as_ref() };
            if !header.is_alive() {
                header.set_in_young(false);
                continue;
            }
            // **fix-old-block-left-in-young-list (2026-09-12)**: a listed block can already
            // be past the line, and a minor must never reclaim an old block.
            //
            // Two things age a block without delisting it: `age_backing_with_owner` raises
            // an array's element storage to its owner's age (the invariant that the backing
            // is never younger than what owns it), and `adaptive-promotion` can lower the
            // line itself. Such a block is then old — the mark phase skips it as old — while
            // still sitting here, so the next sweep would find it unmarked and tombstone
            // storage that is very much alive.
            //
            // It has to be this check rather than a delist at the raise site: this region
            // has no card table, so a raised backing's only claim to being marked is its
            // owner being traced, and the owner stops being traced the moment its card is
            // cleaned — which is exactly what happens when owner and elements go old
            // together. Measured, that is a live `Stream`'s method table vanishing mid-build.
            if header.gen_age() >= threshold {
                header.clear_mark();
                header.set_in_young(false);
                continue;
            }
            if header.is_marked() {
                header.clear_mark();
                if header.bump_gen_age() >= threshold {
                    header.set_in_young(false);
                } else {
                    young[w] = ptr;
                    w += 1;
                }
            } else {
                header.set_in_young(false);
                let charge = Self::alloc_charge_bytes(header);
                if self.tombstone(VarGcRef::pack(ptr, header.generation())) {
                    reclaimed += 1;
                    credited += charge;
                }
            }
        }
        young.truncate(w);
        // In-place compaction keeps the allocation, so the capacity would otherwise stay at
        // the historical peak (426 212 slots = 3.25 MB, against a typical ~100 k length).
        if young.capacity() > 4 * young.len() && young.capacity() - young.len() > 65_536 {
            young.shrink_to(2 * young.len());
        }
        self.young_list = young;
        (reclaimed, credited)
    }

    /// **fix-minor-and-major-in-one-pause (2026-09-10)**: age the young survivors after a
    /// **major**, the way [`Self::sweep_young`] does after a minor.
    ///
    /// A major is a superset collection — it marks from the roots and sweeps every region — so
    /// surviving a major is exactly as much evidence of longevity as surviving a minor, and
    /// aging is what drains the young list. Without this a major leaves every live block young,
    /// and the *next* minor re-marks the entire heap (measured: 1.5 M blocks, 192 ms, on
    /// `09_alloc_ctorless`). The blocks were already swept by [`Self::sweep`]; this only walks
    /// the survivors' ages.
    pub fn age_young_survivors(&mut self) {
        let threshold = self.promotion_age;
        let mut survivors: Vec<NonNull<GcBlockHeader>> = Vec::with_capacity(self.young_list.len());
        for &ptr in &self.young_list {
            // SAFETY: see `iterate_young` — young_list only holds chunk-owned block pointers.
            let header = unsafe { ptr.as_ref() };
            if !header.is_alive() {
                header.set_in_young(false);
                continue;
            }
            if header.bump_gen_age() >= threshold {
                header.set_in_young(false);
            } else {
                survivors.push(ptr);
            }
        }
        self.young_list = survivors;
    }
}
