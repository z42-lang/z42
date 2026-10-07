//! [`ChunkClaim`] — a mutator thread's lock-free write claim on one region chunk (the TLAB
//! side of `Region<T>`). Split out of `region.rs` to keep it under the line limit; the region
//! constructs it in `borrow_chunk` and absorbs it in `retire_chunk`.

use std::ptr::NonNull;
use std::mem::MaybeUninit;
use std::sync::atomic::Ordering;

use super::RegionEntry;

/// **add-gc-tlab (2026-08-29)**: a mutator thread's exclusive write claim on
/// one region chunk (design D1/D2). Produced by [`Region::borrow_chunk`] (under
/// the region lock), then filled **lock-free** by the owning thread via
/// [`ChunkClaim::fill`] until the chunk is full (`next == cap`); the region
/// re-absorbs the filled prefix at [`Region::retire_chunk`].
///
/// # Safety / invariants
/// - `slots` / `init_ptr` are raw pointers into `Region`-owned, never-moving
///   memory (slab chunk arrays + the chunk's `initialized` row, both fixed-size and
///   never reallocated), valid for the region's lifetime.
/// - The chunk is marked `borrowed` in the region while a claim is live, so
///   every region-lock iterate skips it → the owner thread is the **sole**
///   accessor of these slots. That single-writer/no-reader discipline is what
///   makes the un-synchronized `fill` writes sound.
/// - A claim must be retired (or its chunk's `borrowed` flag cleared) before
///   any GC scan of the region — enforced by safepoint retire-on-park.
pub struct ChunkClaim<T> {
    /// Index of the borrowed chunk within `Region::chunks`.
    pub(crate) chunk_idx: u32,
    /// Raw pointer to the chunk's `[MaybeUninit<RegionEntry<T>>; CHUNK_SIZE]`.
    pub(super) slots: *mut MaybeUninit<RegionEntry<T>>,
    /// Raw pointer to the chunk's `initialized` row (`[bool; CHUNK_SIZE]`
    /// buffer). Read per slot in `fill` to choose write mode; only the region
    /// (owner thread) writes it, and never while filling.
    pub(super) init_ptr: *const bool,
    /// Next free slot index within the chunk (bump cursor / high-water mark).
    pub(super) next: u16,
    /// Chunk capacity (`CHUNK_SIZE`).
    pub(super) cap: u16,
    /// The region's out-of-slot payload measure (see `Region::payload_of`).
    pub(super) payload_of: fn(&T) -> u64,
    /// Footprint change of the fills so far (new payloads minus the dead ones they replaced),
    /// handed to the region's `Footprint` at retire — the fill itself touches no shared state.
    pub(super) payload_delta: i64,
    /// Generation a never-initialized slot starts at: `0` for a fresh chunk, above every
    /// generation the chunk's slots ever reached for one that was decommitted (its slots were
    /// dropped and un-initialized, so `fill` cannot read their tombstone generations — this
    /// floor is the ABA guard instead; see `region/decommit.rs`).
    pub(super) gen_floor: u32,
}

impl<T> ChunkClaim<T> {
    /// Bump-fill one object into the claimed chunk **without any lock**.
    /// Returns `(entry_ptr, generation)` for `GcRef` construction, or `None`
    /// when the chunk is full (caller retires + borrows a fresh one).
    ///
    /// Per-slot write mode (via `init_ptr`):
    /// - **uninitialized** slot (fresh-grown chunk): `ptr::write` a new
    ///   `RegionEntry` at generation 0.
    /// - **initialized** slot (pooled chunk's dead entry): read the tombstone
    ///   generation, drop the dead entry, and write the new one preserving that
    ///   generation — the ABA guard (mirrors `Region::alloc`'s free_list path).
    ///
    /// # Safety
    /// Owner-thread-exclusive (see the type's safety contract); the chunk is
    /// `borrowed` so no concurrent reader exists.
    #[inline]
    pub(crate) fn fill(&mut self, value: T) -> Option<(NonNull<RegionEntry<T>>, u32)> {
        if self.next >= self.cap {
            return None;
        }
        let ei = self.next;
        // SAFETY: ei < cap == CHUNK_SIZE; `slots`/`init_ptr` point at the
        // chunk's fixed-size arrays; owner-exclusive access.
        let slot = unsafe { &mut *self.slots.add(ei as usize) };
        let was_init = unsafe { *self.init_ptr.add(ei as usize) };
        self.payload_delta += (self.payload_of)(&value) as i64;
        let generation = if was_init {
            // SAFETY: initialized ⇒ constructed (dead) entry.
            let old = unsafe { slot.assume_init_mut() };
            self.payload_delta -= (self.payload_of)(old.value.get_mut()) as i64;
            let g = old.generation.load(Ordering::Acquire);
            let ne = RegionEntry::new(value, (self.chunk_idx, ei));
            ne.generation.store(g, Ordering::Release);
            // Overwrite: `*` assignment drops the old dead entry, then moves in.
            unsafe { *slot.assume_init_mut() = ne };
            g
        } else {
            let ne = RegionEntry::new(value, (self.chunk_idx, ei));
            ne.generation.store(self.gen_floor, Ordering::Release);
            slot.write(ne);
            self.gen_floor
        };
        self.next = ei + 1;
        // SAFETY: just wrote a valid entry into this slot.
        let entry = unsafe { slot.assume_init_ref() };
        Some((NonNull::from(entry), generation))
    }

    /// Number of objects filled so far (the retire high-water mark).
    #[inline]
    pub(crate) fn filled(&self) -> u16 {
        self.next
    }

    /// True while the claimed chunk still has a free slot to `fill`.
    #[inline]
    pub(crate) fn has_room(&self) -> bool {
        self.next < self.cap
    }
}
