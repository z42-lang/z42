//! Giving the pages of pooled chunks back to the OS (`gc::os_mem::decommit`).
//!
//! A fully-dead chunk goes to `free_chunk_pool` so a TLAB can refill it without growing the
//! region. Past a threshold (the heap decides it — `arc_heap/footprint.rs`) the pool is memory
//! the program is unlikely to want back soon, so the oldest pooled chunks are decommitted:
//!
//! 1. **Drop every constructed slot.** A pooled chunk's slots are dead entries that still own
//!    their payloads (`ObjStorage`, extras) — freeing them is half the point, and a decommitted
//!    page may read back as zeroes, which is not a valid `RegionEntry` to drop later. The
//!    constructed-slot bits are cleared, so every later reader skips the slots (`iterate_alive`,
//!    sweeps, `ChunkClaim::fill`'s write mode).
//! 2. **Record a generation floor.** `fill` preserves a constructed slot's tombstone generation
//!    — the ABA guard against a stale handle to the slot's previous occupant. Dropped slots have
//!    no generation left to preserve, so the chunk remembers the highest one any slot reached
//!    and a refill starts every slot there (`ChunkClaim::gen_floor`). A stale handle's
//!    generation is always *below* its slot's (tombstoning bumps it), hence below the floor.
//! 3. **madvise the pages.** Only whole pages inside a run of adjacent decommitted chunks of one
//!    slab — a page shared with a chunk still in use must not be touched.
//!
//! The memory stays mapped: weak references read dead slots' headers (`WeakGcRef::upgrade`),
//! and a decommitted page reads as zeroes or its old bytes — `alive == false` either way.

use super::*;
use crate::gc::os_mem;

impl<T> Region<T> {
    /// What decommitting a pooled chunk gives back: its slot array.
    const SLOT_BYTES: u64 = chunk_bytes::<T>() as u64;

    /// Decommit pooled chunks, oldest first, until about `budget` bytes have been given back
    /// (or none is left). Returns the bytes given back. Runs at the sweep tail (STW).
    pub fn decommit_pool(&mut self, budget: u64) -> u64 {
        if !os_mem::CAN_DECOMMIT {
            return 0;
        }
        let mut freed = 0u64;
        let mut done: Vec<usize> = Vec::new();
        while freed < budget && self.pool_decommitted < self.free_chunk_pool.len() {
            let ci = self.free_chunk_pool[self.pool_decommitted] as usize;
            self.pool_decommitted += 1;
            self.release_slots(ci);
            freed += Self::SLOT_BYTES;
            done.push(ci);
        }
        done.sort_unstable();
        let mut covered_to = None;
        for ci in done {
            if covered_to.is_some_and(|hi| ci <= hi) {
                continue;
            }
            covered_to = Some(self.decommit_run_around(ci));
        }
        freed
    }

    /// Step 1 + 2 for chunk `ci`: drop its constructed slots (crediting their payloads), raise
    /// its generation floor, and account the chunk as decommitted.
    fn release_slots(&mut self, ci: usize) {
        debug_assert!(!self.decommitted[ci] && !self.borrowed[ci], "only a committed pooled chunk");
        let mut floor = self.gen_floor[ci];
        let mut payload = 0u64;
        let init = std::mem::take(&mut self.init_bits[ci]);
        for ei in 0..CHUNK_SIZE {
            if !side_bits::test(&init, ei) {
                continue;
            }
            let slot = &mut self.chunks[ci][ei];
            // SAFETY: initialized ⇒ a constructed entry; the chunk is pooled (every slot dead,
            // no TLAB holds it) and this runs under STW, so nothing else is reading it.
            let entry = unsafe { slot.assume_init_mut() };
            debug_assert!(!entry.alive.load(Ordering::Relaxed), "pooled chunks hold only the dead");
            floor = floor.max(entry.generation.load(Ordering::Acquire));
            payload += (self.payload_of)(entry.value.get_mut());
            // SAFETY: constructed, and `init_bits` now says it is not — dropped exactly once.
            unsafe { slot.assume_init_drop() };
        }
        self.gen_floor[ci] = floor;
        self.init_per_chunk[ci] = 0;
        self.decommitted[ci] = true;
        self.footprint.credit(payload + Self::SLOT_BYTES);
        self.footprint.pool(Self::SLOT_BYTES, false);
    }

    /// Step 3: madvise the whole pages of the run of decommitted chunks around `ci` (within its
    /// slab). Returns the run's last chunk index.
    fn decommit_run_around(&self, ci: usize) -> usize {
        let per = slab::CHUNKS_PER_SLAB;
        let mut lo = ci;
        while lo % per != 0 && self.decommitted[lo - 1] {
            lo -= 1;
        }
        let mut hi = ci;
        while (hi + 1) % per != 0 && hi + 1 < self.chunks.len() && self.decommitted[hi + 1] {
            hi += 1;
        }
        let (start, end) = self.slabs.span(lo, hi);
        if let Some((addr, len)) = os_mem::inner_pages(start, end) {
            // SAFETY: whole pages inside chunks `lo..=hi`, all decommitted (no live data).
            unsafe { os_mem::decommit(addr, len) };
        }
        hi
    }

    /// `borrow_chunk` just popped `ci` off the pool: account it as in use again, recommitting
    /// it first if it was decommitted.
    pub(super) fn take_pooled(&mut self, ci: u32) {
        // The pool is a stack whose decommitted chunks are its bottom `pool_decommitted`; the
        // one just popped was among them iff the pool is now shorter than that.
        if self.pool_decommitted > self.free_chunk_pool.len() {
            self.pool_decommitted -= 1;
        }
        let ci = ci as usize;
        if !self.decommitted[ci] {
            self.footprint.pool(Self::CHUNK_FOOTPRINT, false);
            return;
        }
        self.decommitted[ci] = false;
        let (start, end) = self.slabs.span(ci, ci);
        let page = os_mem::page_size();
        let lo = start / page * page;
        let hi = end.next_multiple_of(page);
        // SAFETY: page-aligned, inside the chunk's slab (slabs are whole pages).
        unsafe { os_mem::recommit(lo, hi - lo) };
        self.footprint.charge(Self::SLOT_BYTES);
        self.footprint.pool(Self::CHUNK_FOOTPRINT - Self::SLOT_BYTES, false);
    }
}

#[cfg(all(test, debug_assertions))]
#[path = "decommit_tests.rs"]
mod decommit_tests;
