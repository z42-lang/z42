//! `Region<T>` 的年轻代记账：young_list 维护、晋升、卡表。
//! 从 `region.rs` 拆出（fix-young-list-only-when-generational，2026-09-07 —— 父文件
//! 撞到 886 行硬上限）。这一块正是「只有分代 GC 会读」的那部分，所以拆在这里也是
//! 语义上的边界，不只是行数上的。
//!
//! 三个字段仍住在父模块的 `Region`（`young_list` / `generational` / `card_dirty`）——
//! 子模块能看到祖先的私有字段，拆分不改可见性。

use super::*;

impl<T> Region<T> {
    /// **fix-young-list-only-when-generational (2026-09-07)**: construct a region
    /// that maintains [`Self::young_list`] only when `generational` is set. See
    /// that field for the invariant the caller must uphold.
    pub fn new_for_mode(generational: bool) -> Self {
        // `Region` implements `Drop`, so no functional-update from `default()`.
        let mut r = Self::default();
        r.generational = generational;
        r
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
                if entry.alive.load(Ordering::Acquire) && entry.gen_age() < PROMOTION_THRESHOLD {
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
        if prev < PROMOTION_THRESHOLD && new_age >= PROMOTION_THRESHOLD {
            // Transition: young → old. Remove from young_list.
            self.remove_from_young_list(handle.chunk_idx, handle.entry_idx);
            true
        } else {
            false
        }
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

    /// **add-generational-gc P0 (2026-05-22)**: mark a chunk's card
    /// as dirty. Called by write barrier override under
    /// `GenerationalMarkSweep` when an old entry writes a young
    /// reference into one of its slots. The minor GC re-roots from
    /// dirty cards so the young target isn't incorrectly swept.
    pub fn mark_card_dirty(&mut self, chunk_idx: u32) {
        let ci = chunk_idx as usize;
        if ci < self.card_dirty.len() {
            self.card_dirty[ci] |= 1u32;
        }
    }

    /// **add-generational-gc P0 (2026-05-22)**: query a chunk's
    /// card-dirty state. Mostly for tests; minor GC iterates via
    /// `iterate_dirty_cards`.
    pub fn is_card_dirty(&self, chunk_idx: u32) -> bool {
        let ci = chunk_idx as usize;
        ci < self.card_dirty.len() && (self.card_dirty[ci] & 1u32) != 0
    }

    /// **add-generational-gc P0 (2026-05-22)**: reset all card-dirty
    /// bits. Called at end of minor / major GC so the next minor
    /// cycle starts fresh.
    pub fn clear_card_dirty(&mut self) {
        for bit in &mut self.card_dirty {
            *bit = 0;
        }
    }

    /// **add-generational-gc P0 (2026-05-22)**: walk every entry in
    /// dirty chunks. Minor GC uses this to re-root entries in
    /// chunks that received old→young writes since the last collect.
    ///
    /// Callback receives entries regardless of `gen_age` — the
    /// caller filters (typically: re-root old entries to find their
    /// young children for marking).
    pub fn iterate_dirty_cards(&self, mut visit: impl FnMut(RegionHandle, &RegionEntry<T>)) {
        for (ci, card) in self.card_dirty.iter().enumerate() {
            if (*card & 1u32) == 0 {
                continue;
            }
            if ci >= self.chunks.len() {
                continue;
            }
            // add-gc-tlab: skip borrowed chunks (invisible to GC until retire).
            if self.borrowed[ci] {
                continue;
            }
            for ei in 0..CHUNK_SIZE {
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
                visit(h, entry);
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
