//! `Region<T>` 的年轻代记账：年轻集合（每 chunk 一张位图 + chunk 摘要位）、晋升、卡表。
//! 这一块正是「只有分代 GC 会读」的那部分。
//!
//! 字段住在父模块的 `Region`（`young_bits` / `young_chunks` / `young_len` / `generational` /
//! `card_dirty`）—— 子模块能看到祖先的私有字段。

use super::*;

/// **add-finer-card-granularity (2026-09-10)**: cards per chunk. `card_dirty` is a `Vec<u32>`
/// that only ever used bit 0, so all 32 bits were already paid for — this is the same kind of
/// free space as the block header's alignment padding.
pub(crate) const CARDS_PER_CHUNK: usize = 32;

/// Entries covered by one card. `CHUNK_SIZE / CARDS_PER_CHUNK` = 256 / 32 = 8.
pub(crate) const ENTRIES_PER_CARD: usize = CHUNK_SIZE / CARDS_PER_CHUNK;

const _: () = assert!(CHUNK_SIZE % CARDS_PER_CHUNK == 0, "cards must tile a chunk exactly");

/// Which card covers `entry_idx`.
#[inline]
pub(crate) fn card_of(entry_idx: u16) -> u8 {
    (entry_idx as usize / ENTRIES_PER_CARD) as u8
}

impl<T> Region<T> {
    /// Construct a region that maintains the young set only when `generational` is set (see
    /// [`Self::generational`] for the invariant the caller must uphold).
    pub fn new_for_mode(generational: bool, promotion_age: u8) -> Self {
        // `Region` implements `Drop`, so no functional-update from `default()`.
        let mut r = Self::default();
        r.generational = generational;
        r.promotion_age = promotion_age;
        r
    }

    /// The promotion age this region was built with (see [`Region::promotion_age`]).
    #[inline]
    pub fn promotion_age(&self) -> u8 {
        self.promotion_age
    }

    /// Flip young-set maintenance, bringing the set in line with the new setting.
    ///
    /// Turning it **on** rebuilds the set from every live young entry, so a heap switched to
    /// `GenerationalMarkSweep` after it has already allocated still sees a complete young set
    /// (entries in a TLAB-borrowed chunk are invisible here, exactly as they are to every other
    /// region walk — `retire_chunk` adds them when it merges the chunk back). Turning it
    /// **off** clears the set.
    ///
    /// No-op when already in the requested state.
    pub fn set_generational(&mut self, generational: bool) {
        if self.generational == generational {
            return;
        }
        self.generational = generational;
        self.young_bits.iter_mut().for_each(|b| *b = SlotBits::default());
        self.young_chunks.iter_mut().for_each(|w| *w = 0);
        self.young_len = 0;
        if !generational {
            return;
        }
        for ci in 0..self.chunks.len() {
            if self.borrowed[ci] {
                continue;
            }
            let mut young = SlotBits::default();
            side_bits::for_each(&self.init_bits[ci], |ei| {
                // SAFETY: the bit says this slot holds a constructed entry.
                let entry = unsafe { self.chunks[ci][ei].assume_init_ref() };
                if entry.alive.load(Ordering::Acquire) && entry.gen_age() < self.promotion_age {
                    side_bits::set(&mut young, ei);
                }
            });
            self.push_young_bits(ci as u32, &young);
        }
    }

    /// Add one slot to the young set. Callers pass slots they have just filled.
    #[inline]
    pub(super) fn push_young(&mut self, ci: u32, ei: u16) {
        // Nobody reads the set outside generational mode — don't pay for it per alloc.
        if !self.generational {
            return;
        }
        if side_bits::set(&mut self.young_bits[ci as usize], ei as usize) {
            self.young_len += 1;
        }
        side_bits::set(&mut self.young_chunks, ci as usize);
    }

    /// Add every slot of `bits` (one chunk's worth) to the young set — `retire_chunk`'s filled
    /// prefix, or `set_generational`'s rebuild.
    pub(super) fn push_young_bits(&mut self, ci: u32, bits: &SlotBits) {
        if !self.generational || side_bits::is_empty(bits) {
            return;
        }
        let row = &mut self.young_bits[ci as usize];
        for (w, b) in row.iter_mut().zip(bits) {
            self.young_len += (b & !*w).count_ones() as usize;
            *w |= b;
        }
        side_bits::set(&mut self.young_chunks, ci as usize);
    }

    /// Take one slot out of the young set — `O(1)`; a no-op for a slot that is not in it.
    #[inline]
    pub(super) fn remove_young(&mut self, ci: u32, ei: u16) {
        if !self.generational {
            return;
        }
        let row = &mut self.young_bits[ci as usize];
        if side_bits::clear(row, ei as usize) {
            self.young_len -= 1;
            if side_bits::is_empty(row) {
                side_bits::clear(&mut self.young_chunks, ci as usize);
            }
        }
    }

    /// Visit every slot of the young set, chunk by chunk. The chunk summary is read a word at a
    /// time and each chunk's bits are copied before its slots are visited, so `visit` may take
    /// slots out of the set (every caller does) — it gets `&mut self` for that.
    fn for_each_young(&mut self, mut visit: impl FnMut(&mut Self, u32, u16)) {
        for wi in 0..self.young_chunks.len() {
            let mut cw = self.young_chunks[wi];
            while cw != 0 {
                let ci = wi * 64 + cw.trailing_zeros() as usize;
                cw &= cw - 1;
                let bits = self.young_bits[ci];
                side_bits::for_each(&bits, |ei| visit(self, ci as u32, ei as u16));
            }
        }
    }

    /// The entry in slot `(ci, ei)`, which must be constructed.
    #[inline]
    fn slot_entry(&self, ci: u32, ei: u16) -> &RegionEntry<T> {
        debug_assert!(side_bits::test(&self.init_bits[ci as usize], ei as usize));
        // SAFETY: callers name slots from the young set, which only holds constructed slots.
        unsafe { self.chunks[ci as usize][ei as usize].assume_init_ref() }
    }

    /// Increment the entry's `gen_age`; if the new age reaches the promotion age the entry is
    /// promoted — taken out of the young set so subsequent minor GCs don't visit it. Returns
    /// `true` iff the entry was promoted in this call.
    pub fn promote(&mut self, handle: RegionHandle) -> bool {
        let entry = self.resolve(handle);
        // Guard against stale handle: only promote alive entries with
        // matching generation.
        if !entry.alive.load(Ordering::Acquire)
            || entry.generation.load(Ordering::Acquire) != handle.generation
        {
            return false;
        }
        let prev = entry.gen_age.fetch_add(1, Ordering::AcqRel);
        let new_age = prev.saturating_add(1);
        // Being young at all means the entry was under the line in force when it joined the
        // set, so "this call crosses it" is simply "the new age reaches it" — written this way
        // because the age can be *lowered* between collections (adaptive promotion), which
        // makes `prev >= age` reachable: such an entry must leave the set on this very sweep
        // and still be reported as newly-old, because its card is what keeps whatever it
        // points at reachable from then on.
        if new_age >= self.promotion_age {
            self.remove_young(handle.chunk_idx, handle.entry_idx);
            true
        } else {
            false
        }
    }

    /// **adaptive-promotion (2026-09-12)**: re-point the region at a new promotion age.
    ///
    /// Only ever called from the minor sweep, **after** that minor's mark and **before** its
    /// promotions — the one window where lowering the line is safe, because the mark that
    /// just ran used the old (wider) line and the promotions about to run will drain
    /// everything the new line makes old.
    pub fn set_promotion_age(&mut self, age: u8) {
        self.promotion_age = age;
    }

    /// The whole of a minor's work on this region, in **one** walk of the young set: survivors
    /// are aged in place (and leave the set when they cross the line), the dead are tombstoned
    /// where they are judged.
    ///
    /// `prepare_dead` is the caller's business with a dying entry — its size estimate,
    /// breaking its reference edges, taking its finalizer — and runs with the entry still
    /// readable, before the tombstone. The finalizer runs after it.
    ///
    /// **The young generation belongs to the minor** (P1-7): an entry survives only if this minor
    /// reached it. The open cycle's epoch on a young entry does not keep it — that stamp means
    /// "born during the cycle" (the major sweep spares it, it is never doomed, SATB does not
    /// record it), not "the minor must keep it". Why reclaiming an unreached one is safe while a
    /// cycle is open — the grey queue and SATB records are minor roots, old→young edges have dirty
    /// cards, newborns carry the epoch — is in `gc-incremental-major.md`.
    ///
    /// `promote_black`: the open cycle's epoch **while it is sweeping**, for a debug check only.
    /// Marking is complete by then, so every entry a minor can reach carries it — in particular
    /// every entry this pass promotes. An unmarked promotion would be an old entry the sweep is
    /// about to reclaim under a live reference.
    pub fn sweep_young_in_one_pass(
        &mut self,
        observed_age: u8,
        promote_black: Option<crate::gc::refs::MarkKind>,
        mut observe: impl FnMut(bool),
        mut prepare_dead: impl FnMut(&RegionEntry<T>) -> (Option<crate::gc::types::FinalizerFn>, u64),
    ) -> MinorRegionSweep {
        let mut out = MinorRegionSweep::default();
        let threshold = self.promotion_age;
        self.for_each_young(|r, ci, ei| {
            let entry = r.slot_entry(ci, ei);
            debug_assert!(entry.alive.load(Ordering::Acquire), "the young set holds only the alive");
            let age = entry.gen_age();
            if age == observed_age {
                observe(entry.is_marked(crate::gc::refs::MarkKind::Minor));
            }
            let h = RegionHandle {
                chunk_idx: ci,
                entry_idx: ei,
                generation: entry.generation.load(Ordering::Acquire),
            };
            if entry.is_marked(crate::gc::refs::MarkKind::Minor) {
                entry.clear_minor_mark();
                let new_age = age.saturating_add(1);
                entry.gen_age.store(new_age, Ordering::Release);
                if new_age >= threshold {
                    debug_assert!(
                        promote_black.is_none_or(|k| entry.is_marked(k)),
                        "promoted while the cycle sweeps, without its epoch: ({ci}, {ei})"
                    );
                    // Crosses the line: leaves the young set, and its card is what keeps
                    // whatever it points at reachable from now on (the caller dirties it).
                    r.remove_young(ci, ei);
                    out.newly_old.push(h);
                }
                out.survivors += 1;
            } else {
                let (fin, size) = prepare_dead(entry);
                if let Some(f) = fin {
                    f();
                }
                if r.tombstone(h) {
                    out.freed_bytes += size;
                    out.reclaimed += 1;
                }
            }
        });
        out
    }

    /// Age every young survivor of a **major** by one, in one walk of the young set, returning
    /// the ones that crossed the line. The major's sweep already took the dead out of the set.
    pub fn age_young_survivors(&mut self) -> Vec<RegionHandle> {
        let threshold = self.promotion_age;
        let mut newly_old = Vec::new();
        self.for_each_young(|r, ci, ei| {
            let entry = r.slot_entry(ci, ei);
            debug_assert!(entry.alive.load(Ordering::Acquire), "the young set holds only the alive");
            let new_age = entry.gen_age().saturating_add(1);
            entry.gen_age.store(new_age, Ordering::Release);
            if new_age >= threshold {
                newly_old.push(RegionHandle {
                    chunk_idx: ci,
                    entry_idx: ei,
                    generation: entry.generation.load(Ordering::Acquire),
                });
                r.remove_young(ci, ei);
            }
        });
        newly_old
    }

    /// **Tenure**: promote every young entry **without a mark** — the young-generation policy's
    /// answer to minors that do not pay (`arc_heap/young_policy.rs`). One walk, no tracing, no
    /// card work: nothing is left young for a newly-old entry to point at.
    ///
    /// `doomed_unless`: the open cycle's epoch **while it sweeps**. Marking is complete then, so
    /// an entry without the epoch is garbage the sweep has not reached yet (anything in a chunk it
    /// already passed was reclaimed there). Such an entry stays young — promoting it would only
    /// make it an unmarked old entry — and the sweep reclaims it. Everything that does leave the
    /// set therefore carries the epoch, the same `promote_black` guarantee a minor gives.
    ///
    /// Returns how many entries left the set and the bytes `size_of` puts on them.
    pub fn tenure_young(
        &mut self,
        doomed_unless: Option<crate::gc::refs::MarkKind>,
        mut size_of: impl FnMut(&RegionEntry<T>) -> u64,
    ) -> (usize, u64) {
        let threshold = self.promotion_age;
        let (mut tenured, mut bytes) = (0usize, 0u64);
        self.for_each_young(|r, ci, ei| {
            let entry = r.slot_entry(ci, ei);
            if doomed_unless.is_some_and(|k| !entry.is_marked(k)) {
                return;
            }
            if entry.gen_age() < threshold {
                entry.gen_age.store(threshold, Ordering::Release);
            }
            tenured += 1;
            bytes += size_of(entry);
            r.remove_young(ci, ei);
        });
        (tenured, bytes)
    }

    /// Walk every entry of the young set, chunk by chunk. `O(young + chunks / 64)`.
    pub fn iterate_young(&self, mut visit: impl FnMut(RegionHandle, &RegionEntry<T>)) {
        for (wi, &cw) in self.young_chunks.iter().enumerate() {
            side_bits::for_each(&[cw], |b| {
                let ci = wi * 64 + b;
                side_bits::for_each(&self.young_bits[ci], |ei| {
                    let entry = self.slot_entry(ci as u32, ei as u16);
                    let h = RegionHandle {
                        chunk_idx:  ci as u32,
                        entry_idx:  ei as u16,
                        generation: entry.generation.load(Ordering::Acquire),
                    };
                    visit(h, entry);
                });
            });
        }
    }

    /// Number of entries in the young set (diagnostics + escalation heuristic).
    pub fn young_count(&self) -> usize {
        self.young_len
    }

    /// **add-generational-gc P0 (2026-05-22)**: mark the card covering one entry as dirty.
    /// Called by the write-barrier override under `GenerationalMarkSweep` when an old entry
    /// receives a young reference, and by the minor sweep when promotion creates the same
    /// edge without a write. The minor re-roots from dirty cards so the young target isn't
    /// incorrectly swept.
    ///
    /// **add-finer-card-granularity (2026-09-10)**: `card_dirty` has always been a `Vec<u32>`
    /// with **one bit used**, so a chunk was a single card — one cross-gen write re-rooted all
    /// [`CHUNK_SIZE`] (256) of its entries. Measured on `z42c.semantics`: the late minors
    /// seeded **518 530** card roots to find **76** young objects. The other 31 bits were
    /// already allocated, so splitting the chunk into [`CARDS_PER_CHUNK`] cards of
    /// [`ENTRIES_PER_CARD`] entries costs nothing and narrows the root set 32×.
    pub fn mark_card_dirty(&mut self, chunk_idx: u32, entry_idx: u16) {
        let ci = chunk_idx as usize;
        if ci < self.card_dirty.len() {
            self.card_dirty[ci] |= 1u32 << card_of(entry_idx);
        }
    }

    /// **add-generational-gc P0 (2026-05-22)**: whether any card in this chunk is dirty.
    /// Mostly for tests; minor GC iterates via [`Self::iterate_dirty_cards`].
    pub fn is_card_dirty(&self, chunk_idx: u32) -> bool {
        let ci = chunk_idx as usize;
        ci < self.card_dirty.len() && self.card_dirty[ci] != 0
    }

    /// **add-generational-gc P0 (2026-05-22)**: reset all card-dirty
    /// bits. Called at end of minor / major GC so the next minor
    /// cycle starts fresh.
    /// **adaptive-promotion (2026-09-12)**: mark every card dirty, so the next minor re-roots
    /// from every old entry.
    ///
    /// Exists for one caller: the sweep that lowers the promotion age. The card table is a
    /// record of old→young edges under one definition of "old", and lowering the line
    /// changes that definition retroactively — see the call site for what goes wrong without
    /// it. Costs one minor's worth of full-heap rooting, once.
    pub fn dirty_every_card(&mut self) {
        for bits in &mut self.card_dirty {
            *bits = u32::MAX;
        }
    }

    pub fn clear_card_dirty(&mut self) {
        for bit in &mut self.card_dirty {
            *bit = 0;
        }
    }

    /// **add-finer-card-granularity (2026-09-10)**: clean one card. The minor calls this on a
    /// card it has just scanned and found to hold no cross-generational edge any more —
    /// "clean on scan", the other half of what keeps the dirty set from snowballing.
    ///
    /// Cards accumulate otherwise: nothing clears them between majors, while both the write
    /// barrier *and* promotion keep adding to them. Measured: the dirty set grew to ~65% of
    /// the live heap by the late minors.
    pub fn clean_card(&mut self, chunk_idx: u32, card_idx: u8) {
        let ci = chunk_idx as usize;
        if ci < self.card_dirty.len() {
            self.card_dirty[ci] &= !(1u32 << card_idx);
        }
    }

    /// **add-generational-gc P0 (2026-05-22)**: walk every live entry in a dirty **card**.
    /// Minor GC uses this to re-root entries that received old→young writes (or became old
    /// while holding a young reference) since the last collect.
    ///
    /// The callback also receives the card index, so the caller can clean a card it finds no
    /// longer holds a cross-generational edge — see [`Self::clean_card`].
    ///
    /// Callback receives entries regardless of `gen_age` — the caller filters (typically:
    /// re-root old entries to find their young children for marking).
    pub fn iterate_dirty_cards(&self, mut visit: impl FnMut(RegionHandle, &RegionEntry<T>, u8)) {
        for (ci, card) in self.card_dirty.iter().enumerate() {
            if *card == 0 {
                continue;
            }
            if ci >= self.chunks.len() {
                continue;
            }
            // add-gc-tlab: skip borrowed chunks (invisible to GC until retire).
            if self.borrowed[ci] {
                continue;
            }
            let mut bits = *card;
            while bits != 0 {
                let card_idx = bits.trailing_zeros() as u8;
                bits &= bits - 1;
                let lo = card_idx as usize * ENTRIES_PER_CARD;
                for ei in lo..lo + ENTRIES_PER_CARD {
                    if !side_bits::test(&self.init_bits[ci], ei) {
                        continue;
                    }
                    let slot = &self.chunks[ci][ei];
                    let entry = unsafe { slot.assume_init_ref() };
                    if !entry.alive.load(Ordering::Acquire) {
                        continue;
                    }
                    let h = RegionHandle {
                        chunk_idx:  ci as u32,
                        entry_idx:  ei as u16,
                        generation: entry.generation.load(Ordering::Acquire),
                    };
                    visit(h, entry, card_idx);
                }
            }
        }
    }

    /// Test-only corruption injection — empties the young set (consistently: bits, chunk
    /// summary and count) so the next `validate()` reports `YoungEntryNotInList`.
    #[cfg(test)]
    pub(crate) fn clear_young_set_for_test(&mut self) {
        self.young_bits.iter_mut().for_each(|b| *b = SlotBits::default());
        self.young_chunks.iter_mut().for_each(|w| *w = 0);
        self.young_len = 0;
    }
}

/// What one minor collection did to one [`Region`](super::Region). See
/// [`Region::sweep_young_in_one_pass`].
#[derive(Debug, Default)]
pub struct MinorRegionSweep {
    /// Entries that crossed the promotion line on this sweep. The caller needs them for the
    /// byte accounting and for dirtying their cards.
    pub newly_old: Vec<RegionHandle>,
    /// Bytes credited back by the entries this sweep reclaimed.
    pub freed_bytes: u64,
    /// Entries reclaimed.
    pub reclaimed: usize,
    /// Entries that survived (promoted or not) — the phase timer's count.
    pub survivors: usize,
}
