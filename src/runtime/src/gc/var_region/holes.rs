//! M12: the TLAB reuses the dead slots ("holes") of partially-live variable-length chunks.
//!
//! A TLAB's bump claim only ever takes a whole fresh or pooled chunk, so the slots that sweeps
//! tombstone inside chunks that still hold live blocks used to come back only when their whole
//! chunk died — or through the locked [`VarRegion::alloc`] free-list path, which production
//! allocation (the TLAB) never takes. Under churn the heap therefore grew by every partially-live
//! chunk.
//!
//! This module hands those slots to the TLAB **in per-size-class batches**: on a miss the owning
//! thread takes up to [`HOLE_BATCH`] slots of the requested class off the region's free list
//! under the lock, then fills them lock-free ([`VarHoleStash::fill`]) before falling back to the
//! bump claim.
//!
//! # Why exact-class slots, not free spans
//!
//! A slot is reused only by a block of its **own size class**, so it keeps its address, its
//! footprint and its header position. That is what keeps every reader of a stale address sound
//! without a format change:
//!
//! - the chunk walk (`for_each_block_in`) still finds a header at every block start, so `[0, hwm)`
//!   stays a run of headers — carving a span into differently-sized blocks would need filler
//!   headers and a new walk rule;
//! - a stale handle (a weak object reference, the string-metadata cache) still lands on a block
//!   header, whose generation was bumped at tombstone and is kept on reuse, so the generation
//!   guard rejects it — carving at new offsets would put such an address in the middle of
//!   another block's payload, where the "generation" is whatever bytes happen to be there;
//! - the free-list entries already name exactly these slots, epoch-checked (`pool_epoch`), so no
//!   new staleness rule is needed.
//!
//! # Cost
//!
//! The fast path reads two per-class bit masks before the bump claim ([`VarHoleStash`]); a
//! workload that frees nothing pays two bit tests per allocation. A refill takes the region lock
//! once per [`HOLE_BATCH`] reused blocks and never reads a block header (the free-list entry names
//! the chunk). A reused slot is a cold line somewhere in the heap, so `fill` prefetches the next
//! slot of the class while the current block is being built. What remains is locality: blocks
//! allocated together land in whatever holes their classes find, not side by side — measured in
//! `docs/internals/src/runtime/gc-tlab.md` ("空洞复用").
//!
//! # Invariants
//!
//! - **One entry per dead slot.** A slot taken into a stash leaves the region's free list, so the
//!   locked path cannot hand it out twice; [`VarRegion::retire_holes`] pushes the unused ones back.
//! - **Census and young set at take time.** A taken slot is counted live (`live_count`,
//!   `live_per_chunk`) and put in the young set when it is *taken*, not when it is filled, and
//!   retire undoes both for the unused ones. Census and young set therefore stay right even if a
//!   filled block is tombstoned by another thread before the owner retires (`finalize_now`), and
//!   a chunk holding a stashed slot can never look fully dead (it is never pooled under a stash).
//! - **Invisible until built.** [`fill_hole`] leaves the block's `alive` flag clear; the allocation
//!   funnel sets it with release ordering once the payload is written
//!   (`VarGcRef::publish`). A bump-filled block gets the same guarantee from its chunk's
//!   unpublished high-water mark; a reused slot sits in a published chunk, so the flag is what keeps
//!   a heap walk on another thread (`snapshot_live_from_registry`) from reading a half-built object.
//! - **Side tables first.** No slot is handed out while the flagged-dead list is non-empty
//!   ([`VarRegion::take_holes`] refuses), so a finalizer / soft reference / remembered-set entry
//!   keyed by a dead block's address is always dropped before a new block can take that address.
//! - **No generation wrap within an epoch.** `tombstone` stops free-listing a slot once its
//!   generation is [`GEN_REUSE_SPAN`] past its chunk's `reuse_gen`, so a 16-bit handle snapshot
//!   taken in this chunk epoch can never match a later occupant (see [`VarRegion::reusable_gen`]).

use std::ptr::NonNull;
use std::sync::atomic::Ordering;

use super::block::{payload_ptr_of, BlockType, GcBlockHeader};
use super::chunk::{FreeEntry, FreeSlot, NUM_CLASSES};
use super::{VarGcRef, VarRegion};

/// Slots taken per refill. Large enough that the region lock is taken once per 64 reused
/// allocations of a class; small enough that a thread holds at most a few KB of another thread's
/// potential reuse between safepoints.
pub(crate) const HOLE_BATCH: usize = 64;

/// How far past its chunk's `reuse_gen` a slot's generation may go before the slot stops being
/// recycled. Handles carry the low 16 bits of the generation, so within one chunk epoch (all
/// generations in `[reuse_gen, reuse_gen + GEN_REUSE_SPAN]`) no two occupants of an address share a
/// snapshot. A slot that reaches the limit stays tombstoned until its chunk is pooled.
pub(super) const GEN_REUSE_SPAN: u32 = u16::MAX as u32;

/// One size class's batch: `slots[next..]` are taken and unused, `slots[..next]` were filled.
#[derive(Default)]
struct HoleBatch {
    slots: Vec<NonNull<GcBlockHeader>>,
    next: usize,
}

/// A thread's stash of dead slots taken from the variable-length region, one batch per size class
/// (lives inline in the thread's `Tlab`). Filled lock-free by the owner; taken and retired under
/// the region lock.
///
/// The allocation fast path reads only the two masks: [`Self::avail`] says whether this class has
/// a slot ready, [`Self::dry`] whether asking the region again is pointless. A workload that never
/// frees anything (every class dry after its first refill) pays two bit tests per allocation.
pub struct VarHoleStash {
    /// Per-class batches, allocated at the first refill (empty until then, so a `Tlab` can be
    /// built in a `const` context and a thread that never reuses a slot never allocates them).
    batches: Vec<HoleBatch>,
    /// Classes whose batch has an unused slot.
    avail: u128,
    /// Classes whose batch holds slots (taken or filled) — what retire has to visit.
    touched: u128,
    /// Classes the region had no slot for at the last refill. Reset at retire: free lists only
    /// grow during a collection, and every collection retires every TLAB first.
    dry: u128,
}

const _: () = assert!(NUM_CLASSES <= 128, "class bitmasks are u128");

impl Default for VarHoleStash {
    fn default() -> Self {
        Self::new()
    }
}

impl VarHoleStash {
    pub const fn new() -> Self {
        Self { batches: Vec::new(), avail: 0, touched: 0, dry: 0 }
    }

    /// Whether a slot of `size_class` is ready for [`Self::fill`].
    #[inline(always)]
    pub(crate) fn has(&self, size_class: u8) -> bool {
        self.avail & (1u128 << size_class) != 0
    }

    /// Fill the next stashed slot of `size_class` with a block of `payload` bytes. The block is
    /// **not yet alive**: the caller publishes it once its payload is written
    /// ([`VarGcRef::publish`]).
    ///
    /// # Safety
    /// Owner-thread only; [`Self::has`] said yes for `size_class`; the stash's slots were taken by
    /// [`VarRegion::take_holes`] of the region the returned handle is used with; `size_class` is
    /// `class_for(payload).1`.
    #[inline]
    pub(crate) unsafe fn fill(&mut self, size_class: u8, payload: usize, block_type: BlockType) -> VarGcRef {
        debug_assert!(self.has(size_class));
        let b = &mut self.batches[size_class as usize];
        let slot = b.slots[b.next];
        b.next += 1;
        match b.slots.get(b.next) {
            // The next slot of this class is somewhere else in the heap, cold: start pulling its
            // line in now, so the store miss overlaps with this block's construction.
            Some(&ahead) => prefetch_write(ahead.as_ptr().cast()),
            None => self.avail &= !(1u128 << size_class),
        }
        // SAFETY: a taken slot is a tombstoned header of `size_class` that only this thread owns.
        unsafe { fill_hole(slot, payload, block_type) }
    }

    /// Whether the region had nothing of `size_class` at the last refill (see [`Self::dry`]).
    #[inline(always)]
    pub(crate) fn is_dry(&self, size_class: u8) -> bool {
        self.dry & (1u128 << size_class) != 0
    }

    /// Whether retire has anything to do.
    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.touched == 0 && self.dry == 0
    }

    /// Whether the stash holds slots of some heap (taken or filled, not yet retired) — what keeps
    /// the TLAB bound to that heap.
    #[inline]
    pub(crate) fn holds_slots(&self) -> bool {
        self.touched != 0
    }

    /// Forget the dry marks: the TLAB is being bound to a (possibly different) heap, whose free
    /// lists the marks say nothing about.
    #[inline]
    pub(crate) fn forget_dry(&mut self) {
        self.dry = 0;
    }

    /// Unused slots across every class (tests).
    #[cfg(test)]
    pub(crate) fn unused(&self) -> usize {
        self.batches.iter().map(|b| b.slots.len() - b.next).sum()
    }
}

/// Hint the CPU to fetch `p`'s cache line for writing. A no-op where there is no stable
/// instruction for it.
#[inline(always)]
pub(crate) fn prefetch_write(p: *const u8) {
    #[cfg(all(target_arch = "aarch64", not(miri)))]
    // SAFETY: `prfm` is a hint; it never faults, whatever the address.
    unsafe {
        std::arch::asm!("prfm pstl1keep, [{0}]", in(reg) p, options(nostack, preserves_flags, readonly));
    }
    #[cfg(all(target_arch = "x86_64", not(miri)))]
    // SAFETY: as above — `prefetcht0` is a hint and never faults.
    unsafe {
        std::arch::x86_64::_mm_prefetch(p.cast(), std::arch::x86_64::_MM_HINT_T0);
    }
    #[cfg(any(miri, not(any(target_arch = "aarch64", target_arch = "x86_64"))))]
    let _ = p;
}

/// Re-initialize the tombstoned slot at `slot` for a new block, keeping the generation its
/// tombstone bumped (the ABA guard — same discipline as the locked `reinit_slot`). Writes the
/// size, a fresh `type_tag` (age 0, every flag clear: remembered / side / ASCII) and an empty mark,
/// zeroes the payload, and leaves `alive` clear for the funnel to publish.
///
/// The size class and chunk index are not rewritten: a slot keeps both for life, and a heap walk
/// on another thread may be reading `size_class` to step over this (dead) block.
///
/// # Safety
/// `slot` is a dead header of the class `payload` maps to, owned by the calling thread.
#[inline]
unsafe fn fill_hole(slot: NonNull<GcBlockHeader>, payload: usize, block_type: BlockType) -> VarGcRef {
    let p = slot.as_ptr();
    // SAFETY: (all below) `p` heads a tombstoned block of this region whose footprint holds the
    // header + `payload` bytes; the taking thread has exclusive use of it until it publishes.
    unsafe {
        debug_assert!(!(*p).is_alive(), "a stashed slot is dead");
        debug_assert!(
            super::chunk::class_for(payload).1 == (*p).size_class,
            "a slot is only reused by its own size class"
        );
        std::ptr::addr_of_mut!((*p).size).write(payload as u32);
        (*p).marked.store(0, Ordering::Relaxed);
        (*p).type_tag.store(GcBlockHeader::pack_tag(block_type, 0), Ordering::Relaxed);
        std::ptr::write_bytes(payload_ptr_of(slot), 0, payload);
        let generation = (*p).generation.load(Ordering::Relaxed);
        VarGcRef::pack(slot, generation)
    }
}

impl VarRegion {
    /// Refill `stash`'s batch of `size_class` with up to [`HOLE_BATCH`] dead slots off the free
    /// list. Returns whether any was taken; on `false` the class is marked dry until retire.
    ///
    /// Each taken slot is counted live and put in the young set now (see the module docs) —
    /// `retire_holes` undoes that for the ones the thread does not fill.
    pub fn take_holes(&mut self, size_class: u8, stash: &mut VarHoleStash) -> bool {
        let sc = size_class as usize;
        let bit = 1u128 << sc;
        // Side tables first: an address on the flagged-dead list still has entries keyed by it.
        // Every sweep drains the list before mutators resume, so this only bites the window of a
        // `finalize_now` on another thread.
        if !self.flagged_dead.is_empty() {
            stash.dry |= bit;
            return false;
        }
        if stash.batches.is_empty() {
            stash.batches = (0..NUM_CLASSES).map(|_| HoleBatch::default()).collect();
        }
        let b = &mut stash.batches[sc];
        debug_assert_eq!(b.next, b.slots.len(), "refill only an exhausted batch");
        b.slots.clear();
        b.next = 0;
        // Consecutive entries mostly share a chunk (a sweep pushes a chunk's dead slots together),
        // so the per-chunk counters and the young summary are bumped once per run, not per slot.
        let (mut run_ci, mut run) = (usize::MAX, 0u32);
        while b.slots.len() < HOLE_BATCH {
            let Some((slot, ci)) = self.pop_free_slot(size_class) else { break };
            if ci != run_ci {
                self.count_taken(run_ci, run);
                (run_ci, run) = (ci, 0);
            }
            run += 1;
            if self.generational {
                let g = (slot.as_ptr() as usize - self.chunks[ci].base.as_ptr() as usize) >> 3;
                let fresh = crate::gc::side_bits::set(&mut self.young_bits[ci], g);
                debug_assert!(fresh, "a dead slot is never in the young set");
            }
            b.slots.push(slot);
        }
        self.count_taken(run_ci, run);
        if b.slots.is_empty() {
            stash.dry |= bit;
            return false;
        }
        stash.touched |= bit;
        stash.avail |= bit;
        true
    }

    /// Account `n` slots just taken from chunk `ci` as live and young (their young bits are set
    /// by the caller). No-op for `n == 0`.
    #[inline]
    fn count_taken(&mut self, ci: usize, n: u32) {
        if n == 0 {
            return;
        }
        self.live_count += n as usize;
        self.live_per_chunk[ci] += n;
        if self.generational {
            self.young_per_chunk[ci] += n;
            self.young_len += n as usize;
            crate::gc::side_bits::set(&mut self.young_chunks, ci);
        }
    }

    /// Hand `stash` back: its unused slots return to the free list (and leave the census and the
    /// young set again), its filled ones stay — they were counted when taken. Clears the stash,
    /// including the dry marks. Called wherever the thread's TLAB is retired, i.e. before every
    /// collection.
    pub fn retire_holes(&mut self, stash: &mut VarHoleStash) {
        let mut touched = stash.touched;
        while touched != 0 {
            let sc = touched.trailing_zeros() as usize;
            touched &= touched - 1;
            let b = &mut stash.batches[sc];
            for &slot in &b.slots[b.next..] {
                // SAFETY: a taken, never-filled slot — still the tombstoned header it was.
                let ci = unsafe { slot.as_ref() }.chunk_idx as usize;
                self.live_count -= 1;
                self.live_per_chunk[ci] -= 1;
                self.remove_young(slot, ci);
                let off = slot.as_ptr() as usize - self.chunks[ci].base.as_ptr() as usize;
                // The chunk cannot have been pooled since the take (the slot kept it live), so its
                // epoch is the one the entry was pushed under.
                if let Some(s) = FreeSlot::pack(ci, off) {
                    self.free_lists[sc].push(FreeEntry { slot: s, epoch: self.pool_epoch[ci] });
                }
            }
            b.slots.clear();
            b.next = 0;
        }
        stash.avail = 0;
        stash.touched = 0;
        stash.dry = 0;
    }

    /// Whether a slot of chunk `ci` that just reached `generation` may be recycled (see
    /// [`GEN_REUSE_SPAN`]). Every handle into the chunk minted in its current epoch carries a
    /// generation in `[reuse_gen, generation)`; keeping the span under 2^16 keeps their 16-bit
    /// snapshots distinct from every later occupant of the same address.
    #[inline]
    pub(super) fn reusable_gen(&self, ci: usize, generation: u32) -> bool {
        generation.wrapping_sub(self.reuse_gen[ci]) <= GEN_REUSE_SPAN
    }
}

#[cfg(test)]
#[path = "holes_tests.rs"]
mod holes_tests;
