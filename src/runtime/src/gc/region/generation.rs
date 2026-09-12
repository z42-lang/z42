//! `Region<T>` 的年轻代记账：young_list 维护、晋升、卡表。
//! 从 `region.rs` 拆出（fix-young-list-only-when-generational，2026-09-07 —— 父文件
//! 撞到 886 行硬上限）。这一块正是「只有分代 GC 会读」的那部分，所以拆在这里也是
//! 语义上的边界，不只是行数上的。
//!
//! 三个字段仍住在父模块的 `Region`（`young_list` / `generational` / `card_dirty`）——
//! 子模块能看到祖先的私有字段，拆分不改可见性。

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
    /// **fix-young-list-only-when-generational (2026-09-07)**: construct a region
    /// that maintains [`Self::young_list`] only when `generational` is set. See
    /// that field for the invariant the caller must uphold.
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

    /// **fix-young-list-only-when-generational (2026-09-07)**: flip young-list
    /// maintenance, bringing the list in line with the new setting.
    ///
    /// Turning it **on** rebuilds the list from every live young entry, so a heap
    /// switched to `GenerationalMarkSweep` after it has already allocated still
    /// sees a complete young set (entries in a TLAB-borrowed chunk are invisible
    /// here, exactly as they are to every other region walk — `retire_chunk`
    /// lists them when it merges the chunk back). Turning it **off** drops the
    /// list and every back-pointer into it.
    ///
    /// No-op when already in the requested state.
    pub fn set_generational(&mut self, generational: bool) {
        if self.generational == generational {
            return;
        }
        self.generational = generational;
        if !generational {
            for &(ci, ei) in &self.young_list {
                // SAFETY: presence in `young_list` implies a constructed slot.
                let entry = unsafe { self.chunks[ci as usize][ei as usize].assume_init_ref() };
                entry.clear_young_idx();
            }
            self.young_list.clear();
            self.young_list.shrink_to_fit();
            return;
        }
        // Rebuild. Collect first: `push_young` needs `&mut self`, so it cannot
        // run while `self.chunks` is borrowed by the scan (same reason
        // `retire_chunk` splits its two loops).
        let mut young: Vec<(u32, u16)> = Vec::new();
        for (ci, chunk) in self.chunks.iter().enumerate() {
            if self.borrowed[ci] {
                continue;
            }
            for ei in 0..CHUNK_SIZE {
                if !self.initialized[ci][ei] {
                    continue;
                }
                // SAFETY: `initialized` says this slot holds a constructed entry.
                let entry = unsafe { chunk[ei].assume_init_ref() };
                if entry.alive.load(Ordering::Acquire) && entry.gen_age() < self.promotion_age {
                    young.push((ci as u32, ei as u16));
                }
            }
        }
        for (ci, ei) in young {
            self.push_young(ci, ei);
        }
    }

    /// Allocate `value` into the region. Returns a stable handle.
    ///
    /// Fast path: pop a tombstoned slot from `free_list`. The slot
    /// already has initialized memory; we drop the old (dead)
    /// `RegionEntry` and write a fresh one. The generation was
    /// bumped at tombstone time so the new handle's generation is
    /// the current entry generation.
    ///
    /// Slow path: bump pointer. If the current chunk is full, push

    /// **fix-young-list-quadratic-sweep (2026-09-06)**: the single entry point
    /// for appending to `young_list`. Records the new slot's index inside the
    /// entry itself (`RegionEntry::young_idx`) — that back-pointer is what lets
    /// [`remove_from_young_list`](Self::remove_from_young_list) skip the linear
    /// scan. Every push must go through here; a missed one leaves an entry that
    /// can never be removed in O(1) (it degrades to a silent no-op removal,
    /// caught by `validate`'s `YoungIndexMismatch`).
    pub(super) fn push_young(&mut self, ci: u32, ei: u16) {
        // fix-young-list-only-when-generational: nobody reads the list outside
        // generational mode — don't pay a Vec push + back-pointer store per alloc.
        if !self.generational {
            return;
        }
        let idx = self.young_list.len();
        self.young_list.push((ci, ei));
        // SAFETY: callers push only slots they have just initialized
        // (`alloc`) or merged back from a filled TLAB chunk (`retire_chunk`),
        // so the slot holds a constructed entry.
        let entry = unsafe { self.chunks[ci as usize][ei as usize].assume_init_ref() };
        entry.set_young_idx(idx);
    }

    /// **add-generational-gc P0 (2026-05-22)**: helper to remove a
    /// `(chunk_idx, entry_idx)` pair from `young_list` via `swap_remove`.
    ///
    /// **fix-young-list-quadratic-sweep (2026-09-06)**: O(1). This used to be
    /// `young_list.iter().position(...)`, justified as "acceptable since
    /// tombstone is sweep-time work, not the alloc hot path" — but sweep calls
    /// `tombstone` **once per dead object**, so a linear scan here made sweep
    /// O(dead x young). Measured: a single collection of a 230 MB heap
    /// (187 MB of it garbage) spent **137 s** STW with 100% of native-stack
    /// samples inside this function, which is why `Z42_GC_MAX_BYTES` — the
    /// switch that arms automatic collection at all — was left unset by
    /// default. `promote` had the same problem against the *survivor* count.
    ///
    /// The entry stores its own index into `young_list`, so removal is a
    /// `swap_remove` plus one back-pointer fixup on the element that moved
    /// into the hole.
    pub(super) fn remove_from_young_list(&mut self, ci: u32, ei: u16) {
        // fix-young-list-only-when-generational: the list is empty and every
        // `young_idx` is the sentinel — skip the deref entirely.
        if !self.generational {
            return;
        }
        // SAFETY: every caller has already resolved this slot (it is a
        // constructed entry it just tombstoned/promoted).
        let entry = unsafe { self.chunks[ci as usize][ei as usize].assume_init_ref() };
        let Some(pos) = entry.young_idx() else { return };
        entry.clear_young_idx();
        // Defensive: a desynchronized back-index (the test-only
        // `clear_young_list_for_test` corruption injection produces one) must
        // degrade to a no-op — never a panic, and never evicting another
        // entry's slot.
        if self.young_list.get(pos) != Some(&(ci, ei)) {
            return;
        }
        self.young_list.swap_remove(pos);
        // `swap_remove` moved the tail element into `pos` (no move happened if
        // we removed the tail itself) — repair its back-pointer.
        if let Some(&(mci, mei)) = self.young_list.get(pos) {
            // SAFETY: presence in `young_list` implies a constructed slot.
            let moved = unsafe { self.chunks[mci as usize][mei as usize].assume_init_ref() };
            moved.set_young_idx(pos);
        }
    }

    /// **add-generational-gc P0 (2026-05-22)**: increment the entry's
    /// `gen_age`. If the new age reaches `PROMOTION_THRESHOLD`, the
    /// entry is "promoted" — removed from `young_list` so subsequent
    /// minor GCs don't visit it. Returns `true` iff the entry was
    /// promoted in this call (transitioned `< threshold` →
    /// `>= threshold`).
    ///
    /// Called by minor GC after sweep, on each surviving young entry.
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
        // Being in `young_list` at all means the entry was young under the line in force
        // when it was listed, so "this call crosses it" is simply "the new age reaches it".
        //
        // **adaptive-promotion (2026-09-12)**: written this way rather than as the old
        // `prev < age && new_age >= age` because the age can now be *lowered* between
        // collections, which makes `prev >= age` reachable — the entry was young under the
        // old line and is already past the new one. Such an entry must leave the young list
        // on this very sweep (the next mark skips it as old, so staying listed would have
        // the sweep after that find it unmarked and reclaim a live object), and must still
        // be reported as newly-old, because its card is what keeps whatever it points at
        // reachable from then on.
        if new_age >= self.promotion_age {
            // Transition: young → old. Remove from young_list.
            self.remove_from_young_list(handle.chunk_idx, handle.entry_idx);
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

    /// **add-generational-gc P0 (2026-05-22)**: walk every entry in
    /// `young_list`. O(young) iteration cost. Order: insertion order
    /// (last-promoted entries swap-removed; insertion order otherwise).
    pub fn iterate_young(&self, mut visit: impl FnMut(RegionHandle, &RegionEntry<T>)) {
        for &(ci, ei) in &self.young_list {
            if !self.initialized[ci as usize][ei as usize] {
                continue;
            }
            let slot = &self.chunks[ci as usize][ei as usize];
            let entry = unsafe { slot.assume_init_ref() };
            if !entry.alive.load(Ordering::Acquire) {
                continue;
            }
            let h = RegionHandle {
                chunk_idx:  ci,
                entry_idx:  ei,
                generation: entry.generation.load(Ordering::Acquire),
            };
            visit(h, entry);
        }
    }

    /// **add-generational-gc P0 (2026-05-22)**: number of entries in
    /// young_list (for diagnostics + escalation heuristic).
    pub fn young_count(&self) -> usize {
        self.young_list.len()
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
                    if !self.initialized[ci][ei] {
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

    /// **add-gc-debug-invariants P1 (2026-05-22)**: test-only corruption
    /// injection helper — clears `young_list` directly so the next
    /// `validate()` reports `YoungEntryNotInList`.
    #[cfg(test)]
    pub(crate) fn clear_young_list_for_test(&mut self) {
        self.young_list.clear();
    }
}
