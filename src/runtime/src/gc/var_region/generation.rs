//! `VarRegion`'s generational half — the young set and everything that walks it. Split out of
//! `var_region.rs` into the same shape as `region/generation.rs` holds `Region<T>`'s: the
//! generational logic is a distinct concern from the allocator.
//!
//! The young set is a bitmap per chunk with one bit per 8-byte granule, set at a block's start
//! (see `VarRegion::young_bits`), plus a per-chunk count and a one-bit-per-chunk summary so a
//! walk visits only chunks that hold something young.

use super::chunk::young_words;
use super::*;
use crate::gc::side_bits;

impl VarRegion {
    /// **adaptive-promotion (2026-09-12)**: re-point the region at a new promotion age.
    /// Same window as `Region::set_promotion_age` — inside the minor sweep, after its mark.
    pub fn set_promotion_age(&mut self, age: u8) {
        self.promotion_age = age;
    }

    /// A chunk's young-bitmap row: sized for a chunk of `cap` bytes while generational, empty
    /// (no allocation) otherwise.
    pub(super) fn new_young_row(&self, cap: usize) -> Box<[u64]> {
        if self.generational {
            vec![0u64; young_words(cap)].into_boxed_slice()
        } else {
            Box::default()
        }
    }

    /// Flip young-set maintenance, bringing the set in line with the new setting. Mirrors
    /// `Region<T>::set_generational` (#524).
    ///
    /// Turning it **on** allocates the bitmaps and rebuilds the set from every live young
    /// block, so a heap switched to `GenerationalMarkSweep` after it has already allocated still
    /// sees a complete young set. Turning it **off** frees the bitmaps.
    ///
    /// No-op when already in the requested state.
    pub fn set_generational(&mut self, generational: bool) {
        if self.generational == generational {
            return;
        }
        self.generational = generational;
        for ci in 0..self.chunks.len() {
            let cap = self.chunks[ci].cap;
            self.young_bits[ci] = self.new_young_row(cap);
            self.young_per_chunk[ci] = 0;
        }
        self.young_chunks.iter_mut().for_each(|w| *w = 0);
        self.young_len = 0;
        if !generational {
            return;
        }
        let threshold = self.promotion_age;
        for ci in 0..self.chunks.len() {
            self.for_each_block_in_mut(ci, |r, ptr| {
                // SAFETY: a carved header of a chunk this region owns.
                let header = unsafe { ptr.as_ref() };
                if header.is_alive() && header.gen_age() < threshold {
                    r.add_young(ptr, ci);
                }
            });
        }
    }

    /// The young-bitmap bit of the block at `ptr` in chunk `ci`.
    #[inline]
    fn granule(&self, ptr: NonNull<GcBlockHeader>, ci: usize) -> usize {
        (ptr.as_ptr() as usize - self.chunks[ci].base.as_ptr() as usize) >> 3
    }

    /// Put the block at `ptr` (in chunk `ci`) into the young set. No-op outside generational
    /// mode or when it is already in.
    #[inline]
    pub(super) fn add_young(&mut self, ptr: NonNull<GcBlockHeader>, ci: usize) {
        if !self.generational {
            return;
        }
        let g = self.granule(ptr, ci);
        if side_bits::set(&mut self.young_bits[ci], g) {
            self.young_per_chunk[ci] += 1;
            self.young_len += 1;
            side_bits::set(&mut self.young_chunks, ci);
        }
    }

    /// Take the block at `ptr` (in chunk `ci`) out of the young set — `O(1)`; a no-op for a
    /// block that is not in it.
    #[inline]
    pub(super) fn remove_young(&mut self, ptr: NonNull<GcBlockHeader>, ci: usize) {
        if !self.generational {
            return;
        }
        let g = self.granule(ptr, ci);
        if side_bits::clear(&mut self.young_bits[ci], g) {
            self.young_len -= 1;
            self.young_per_chunk[ci] -= 1;
            if self.young_per_chunk[ci] == 0 {
                side_bits::clear(&mut self.young_chunks, ci);
            }
        }
    }

    /// Visit every block of the young set with `&mut self`, chunk by chunk, address order
    /// within a chunk. The summary and each bitmap word are read once and visited from the copy,
    /// so `visit` may take blocks out of the set (every caller only ever clears the bit of the
    /// block it is visiting, which the copy already covers).
    fn for_each_young(&mut self, mut visit: impl FnMut(&mut Self, NonNull<GcBlockHeader>, usize)) {
        for wi in 0..self.young_chunks.len() {
            let mut cw = self.young_chunks[wi];
            while cw != 0 {
                let ci = wi * 64 + cw.trailing_zeros() as usize;
                cw &= cw - 1;
                let base = self.chunks[ci].base.as_ptr();
                for k in 0..self.young_bits[ci].len() {
                    let mut w = self.young_bits[ci][k];
                    while w != 0 {
                        let g = k * 64 + w.trailing_zeros() as usize;
                        w &= w - 1;
                        // SAFETY: a young bit marks the start of a carved, alive block in chunk `ci`.
                        let ptr = unsafe { NonNull::new_unchecked(base.add(g << 3) as *mut GcBlockHeader) };
                        visit(self, ptr, ci);
                    }
                }
            }
        }
    }

    /// Visit every block currently in the young set. Read-only; chunk order, address order
    /// within a chunk.
    pub fn iterate_young(&self, mut visit: impl FnMut(VarGcRef, &GcBlockHeader)) {
        for (wi, &cw) in self.young_chunks.iter().enumerate() {
            side_bits::for_each(&[cw], |b| {
                let ci = wi * 64 + b;
                let base = self.chunks[ci].base.as_ptr();
                side_bits::for_each(&self.young_bits[ci], |g| {
                    // SAFETY: a young bit marks the start of a carved, alive block in chunk `ci`.
                    let ptr = unsafe { NonNull::new_unchecked(base.add(g << 3) as *mut GcBlockHeader) };
                    let header = unsafe { ptr.as_ref() };
                    visit(VarGcRef::pack(ptr, header.generation()), header);
                });
            });
        }
    }

    /// How many blocks the next minor GC will visit — exact (the set never holds the dead).
    pub fn young_count(&self) -> usize {
        self.young_len
    }

    /// Minor sweep. Walks only the young set, which is what bounds minor cost at O(young):
    ///
    /// - marked → clear the mark and age it; on reaching the promotion age it leaves the young
    ///   set (promotion is a label change — var blocks never move, see `VarGcRef`'s
    ///   address-as-identity contract);
    /// - unmarked → finalize + tombstone (which takes it out of the set), crediting the bytes
    ///   it was charged at alloc.
    ///
    /// Old blocks are never visited — that is the definition of a minor collection. They are
    /// reachable as minor roots only through the dirty-card set.
    /// Only what this minor marked survives, whatever epoch a block carries; `promote_black` is
    /// the debug check of `Region::sweep_young_in_one_pass`.
    pub fn sweep_young(&mut self, promote_black: Option<crate::gc::refs::MarkKind>) -> (usize, u64) {
        let threshold = self.promotion_age;
        let mut reclaimed = 0usize;
        let mut credited: u64 = 0;
        self.for_each_young(|r, ptr, ci| {
            // SAFETY: see `for_each_young`.
            let header = unsafe { ptr.as_ref() };
            debug_assert!(header.is_alive(), "the young set holds only the alive");
            // **fix-old-block-left-in-young-list (2026-09-12)**: a young-set block can already
            // be past the line, and a minor must never reclaim an old block.
            //
            // Two things age a block without taking it out of the set: `age_backing_with_owner`
            // raises an array's element storage to its owner's age (the invariant that the
            // backing is never younger than what owns it), and `adaptive-promotion` can lower
            // the line itself. Such a block is then old — the mark phase skips it as old —
            // while still in the set, so the next sweep would find it unmarked and tombstone
            // storage that is very much alive.
            //
            // It has to be this check rather than a removal at the raise site: this region has
            // no card table, so a raised backing's only claim to being marked is its owner being
            // traced, and the owner stops being traced the moment its card is cleaned — which is
            // exactly what happens when owner and elements go old together.
            if header.gen_age() >= threshold {
                header.clear_minor_mark();
                r.remove_young(ptr, ci);
                return;
            }
            if header.is_marked(crate::gc::refs::MarkKind::Minor) {
                header.clear_minor_mark();
                if header.bump_gen_age() >= threshold {
                    debug_assert!(
                        promote_black.is_none_or(|k| header.is_marked(k)),
                        "var block promoted while the cycle sweeps, without its epoch"
                    );
                    r.remove_young(ptr, ci);
                }
            } else {
                let charge = Self::alloc_charge_bytes(header);
                if r.tombstone(VarGcRef::pack(ptr, header.generation())) {
                    reclaimed += 1;
                    credited += charge;
                }
            }
        });
        (reclaimed, credited)
    }

    /// **Tenure** — the var-region twin of `Region::tenure_young`: every young block leaves the
    /// set at the promotion age, without a mark. While the open cycle sweeps, a block without
    /// its epoch is garbage the sweep has not reached yet and stays young. Returns how many
    /// blocks left.
    pub fn tenure_young(&mut self, doomed_unless: Option<crate::gc::refs::MarkKind>) -> usize {
        let threshold = self.promotion_age;
        let mut tenured = 0usize;
        self.for_each_young(|r, ptr, ci| {
            // SAFETY: see `for_each_young`.
            let header = unsafe { ptr.as_ref() };
            if doomed_unless.is_some_and(|k| !header.is_marked(k)) {
                return;
            }
            header.raise_gen_age_to(threshold);
            r.remove_young(ptr, ci);
            tenured += 1;
        });
        tenured
    }

    /// **fix-minor-and-major-in-one-pause (2026-09-10)**: age the young survivors after a
    /// **major**, the way [`Self::sweep_young`] does after a minor.
    ///
    /// A major is a superset collection — it marks from the roots and sweeps every region — so
    /// surviving a major is exactly as much evidence of longevity as surviving a minor, and
    /// aging is what drains the young set. Without this a major leaves every live block young,
    /// and the *next* minor re-marks the entire heap. The blocks were already swept by
    /// [`Self::sweep`] (which took the dead out of the set); this only walks the survivors' ages.
    pub fn age_young_survivors(&mut self) {
        let threshold = self.promotion_age;
        self.for_each_young(|r, ptr, ci| {
            // SAFETY: see `for_each_young`.
            let header = unsafe { ptr.as_ref() };
            debug_assert!(header.is_alive(), "the young set holds only the alive");
            if header.bump_gen_age() >= threshold {
                r.remove_young(ptr, ci);
            }
        });
    }
}
