//! `Region<T>` —— chunked region allocator backing for GC entries.
//!
//! **add-custom-allocator P0 (2026-05-22)**: replaces the per-object
//! `Arc<GcAllocation<T>>` storage. Each `Region<T>` owns
//! fixed-size chunks of `MaybeUninit<RegionEntry<T>>` carved from page-aligned slabs
//! (`region/slab.rs`) that never relocate, so `RegionEntry` addresses remain
//! stable for `GcRef::as_ptr` (identity hashing) until the entry is
//! tombstoned by sweep.
//!
//! # Allocation model
//!
//! - **Fast path**: free list pop — reuses a tombstoned slot from a
//!   prior sweep cycle. Generation counter incremented at tombstone
//!   time prevents stale `WeakGcRef` from upgrading to the new
//!   occupant (ABA prevention).
//! - **Slow path**: bump pointer within the current chunk. When the
//!   chunk fills, grow `chunks` and start fresh.
//!
//! # Sweep model (P1+ wiring)
//!
//! `iterate_alive(visit)` walks all chunks linearly, skipping
//! tombstoned (alive=false) entries. `tombstone(handle)` flips alive
//! to false, bumps generation, pushes the slot to free_list. No
//! `Drop` runs on the data — finalizer dispatch is the caller's
//! responsibility (`sweep_phase` in `ArcMagrGC` per spec D3).
//!
//! # Concurrency
//!
//! The region itself is **not** `Sync`; callers (`ArcMagrGC`) wrap
//! it in `parking_lot::Mutex<Region<T>>` for the alloc / tombstone
//! paths. `RegionEntry` data access goes through its own
//! `parking_lot::Mutex<T>` for fine-grained locking (preserves
//! `add-multithreading-foundation` concurrency model per design D6).

use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU8, Ordering};

use parking_lot::Mutex;

use super::side_bits;
use super::types::FinalizerFn;

/// Chunk capacity (entries per chunk). 256 balances:
/// - Per-chunk allocation cost (1 malloc per CHUNK_SIZE allocs amortizes)
/// - Cache locality for sweep traversal (chunk fits in ~16-64 KB depending on T)
/// - Granularity for future per-thread arenas (256 is a reasonable batch)
pub(crate) const CHUNK_SIZE: usize = 256;

/// One bit per slot of a chunk — the shape of every per-slot side table (`gc::side_bits`).
pub(crate) type SlotBits = [u64; CHUNK_SIZE / 64];

mod entry;
pub use entry::*;

mod invariants;
pub use invariants::*;

mod claim;
pub use claim::ChunkClaim;

mod footprint;

mod decommit;
mod slab;
pub(crate) use slab::{chunk_bytes, ChunkPtr, Slabs};

pub(crate) mod generation;

/// Opaque handle into a `Region<T>`. Encodes (chunk index, entry
/// index within chunk, generation snapshot). 12 bytes total —
/// `Copy`-able primitive components but the public `GcRef<T>` wrapper
/// in `refs.rs` enforces `Clone`-only (per design D9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegionHandle {
    pub(crate) chunk_idx: u32,
    pub(crate) entry_idx: u16,
    pub(crate) generation: u32,
}

/// Chunked region allocator. Owns user objects of type `T` plus
/// per-object GC metadata. See module-level docs for the allocation
/// + sweep model.
pub struct Region<T> {
    /// Chunks of pre-reserved entries, each a fixed-size slot array carved from `slabs`, so
    /// its address is stable for the region's lifetime.
    chunks: Vec<ChunkPtr<T>>,
    /// The page-aligned slabs `chunks` are carved from (`region/slab.rs`).
    slabs: Slabs<T>,

    /// **add-gc-tlab (2026-08-29)**: ambient (locked-path) bump cursor —
    /// `Some((chunk_idx, next_entry_idx))` of the chunk the ambient
    /// `Region::alloc` currently bumps into, or `None` before the first
    /// ambient alloc / after its chunk filled (next ambient alloc grows a
    /// *fresh* chunk via [`grow_new_chunk`]). Was `next_bump: (u32,u16)`;
    /// the old `ci >= chunks.len()` grow heuristic collided with TLAB
    /// `borrow_chunk` (which also appends to `chunks`) — the ambient path
    /// could bump into a borrowed chunk's index. Tracking its *own* current
    /// chunk (only ever a fresh-grown, ambient-exclusive one) makes ambient
    /// and TLAB share `chunks` without index collision. Ambient never pulls
    /// from `free_chunk_pool` (that's TLAB-only); it reuses dead slots via
    /// `free_list` and grows fresh chunks otherwise.
    ambient_cur: Option<(u32, u16)>,

    /// Tombstoned slots reusable by fresh allocs, one bit per slot per chunk (M8) —
    /// `free_bits[ci]` has bit `ei` set when slot `(ci, ei)` is dead and may be refilled.
    /// A fixed 32 bytes a chunk, whatever the number of dead slots; reclaiming a chunk drops its
    /// slots from the set by zeroing its four words.
    free_bits: Vec<SlotBits>,
    /// Chunks with at least one free slot. The invariant this file keeps is
    /// `ci ∈ free_chunks ⟺ free_bits[ci] ≠ 0` (no duplicates), which is what lets
    /// [`Self::pop_free_slot`] find a reusable slot in `O(1)` instead of scanning chunks.
    free_chunks: Vec<u32>,
    /// Total set bits across `free_bits`. Kept incrementally because [`Self::free_slot_count`]
    /// would otherwise be `O(chunks)`.
    free_len: usize,

    /// Constructed slots, one bit per slot per chunk: bit `(ci, ei)` is set once the slot holds
    /// a `RegionEntry` (alive or dead). Sweeps and walks visit only these, which also skips the
    /// never-filled tail of a chunk.
    init_bits: Vec<SlotBits>,

    /// The young set (M8): bit `(ci, ei)` is set while slot `(ci, ei)` holds an alive entry
    /// younger than [`Self::promotion_age`]. Set on alloc / retire, cleared on promotion and on
    /// tombstone, so it is exact at every point — a minor walks only these bits.
    ///
    /// Maintained only while [`Self::generational`] is set (no other mode reads it).
    young_bits: Vec<SlotBits>,
    /// One bit per chunk: whether `young_bits[ci]` has any bit set. A minor walks the chunks
    /// named here, so its cost follows the young set, not the heap — 64 chunks per word of
    /// summary.
    young_chunks: Vec<u64>,
    /// Set bits across `young_bits` ([`Self::young_count`]).
    young_len: usize,

    /// Whether this region maintains the young set ([`Self::young_bits`]) at all.
    ///
    /// The young set is read by exactly one consumer — minor GC, which runs only
    /// under `GcMode::GenerationalMarkSweep` — the production default since
    /// flip-gc-default-to-generational (2026-09-10). Under `StwMarkSweep`
    /// `promote` is never called, so nothing ever leaves the
    /// list and it grows to hold *every* live entry: measured 830 k entries
    /// (6.5 MB) compiling `z42c.semantics`, for a list no one reads. Gating the
    /// two maintenance points on this flag measured −0.22% instructions and
    /// −12.2 MB peak RSS on that workload.
    ///
    /// **Invariant**: this must equal "the owning heap's `GcMode` is
    /// `GenerationalMarkSweep`". It is set once at construction from the
    /// resolved mode and thereafter changed *only* by
    /// [`Self::set_generational`], which `ArcMagrGC::set_mode` calls under the
    /// region lock in the same breath as the mode store — so the two cannot
    /// drift. Turning it on rebuilds the list from the live entries, so a heap
    /// switched to generational mid-run collects correctly.
    generational: bool,

    /// **add-generational-gc P0 (2026-05-22)**: per-chunk dirty card
    /// bitmap. Bit `ci` set when an old→young write happened to an
    /// entry in chunk `ci` (recorded by write barrier override under
    /// `GenerationalMarkSweep` mode). Minor GC scans dirty chunks +
    /// adds their entries as additional roots (in case any reaches
    /// a young object).
    ///
    /// One `u32` per chunk — over-allocated for alignment + future
    /// sub-chunk card granularity. v1 uses bit 0 only.
    card_dirty: Vec<u32>,

    /// **add-gc-tlab (2026-08-29)**: per-chunk "currently borrowed by a TLAB"
    /// flag (one bool per chunk, parallel to `chunks`). A borrowed chunk is
    /// being lock-free bump-filled by its owning mutator thread, so every
    /// region-lock iteration (`iterate_alive`/
    /// `iterate_dirty_cards`/`validate`/reclaim) **skips it wholesale** — its
    /// in-flight objects are invisible to GC until [`retire_chunk`] merges the
    /// filled prefix back (flips this to `false`). Under STW every TLAB is
    /// retired first (safepoint retire-on-park), so a collector always sees a
    /// fully-merged region with no borrowed chunks. Prevents the data race
    /// between a mutator's un-synchronized fill write and a concurrent
    /// diagnostic iterate (which no longer serialize on the region lock the
    /// way the pre-TLAB per-object `alloc` did).
    borrowed: Vec<bool>,

    /// **add-gc-tlab (2026-08-29)**: chunk-level free pool (D7). Indices of
    /// chunks that became **fully dead** (every slot tombstoned) at a sweep
    /// and were normalized (see [`reclaim_dead_chunks`]) — every slot is a
    /// constructed, dead, generation-preserved `RegionEntry`. [`borrow_chunk`]
    /// pops from here before growing a brand-new chunk, so short-lived-object
    /// workloads (the compiler) recycle chunk memory instead of growing
    /// unboundedly. Slot-level reuse of partial-live chunks stays Deferred.
    free_chunk_pool: Vec<u32>,
    /// **add-promotion-age-knob (2026-09-08)**: how many minor GCs an entry must survive
    /// before promotion. Read once from `Z42_GC_PROMOTION_AGE` when the heap is built and
    /// cached here — the alternative (a global read) would land on the write-barrier hot
    /// path, which is exactly why this knob was "deliberately not done" before.
    /// Defaults to [`PROMOTION_THRESHOLD`].
    promotion_age: u8,
    /// **add-incremental-chunk-reclaim (2026-09-10)**: per-chunk census, maintained
    /// incrementally so [`Self::reclaim_dead_chunks`] never has to scan slots.
    ///
    /// That scan was `O(chunks × CHUNK_SIZE)` — every slot of every chunk, every collection —
    /// measured at 3.2 ms (objects) + 3.3 ms (arrays) of a 54 ms minor sweep. With these two
    /// vectors the pass is `O(chunks)`, and the question "is this chunk fully dead?" is a
    /// single comparison.
    ///
    /// `init_per_chunk` counts constructed slots (a never-touched chunk has no storage to
    /// recycle); `live_per_chunk` counts the ones still alive. A **pooled** chunk keeps its
    /// `init` count — its slots stay constructed so `ChunkClaim::fill` can preserve their
    /// tombstone generations (the ABA guard).
    init_per_chunk: Vec<u32>,
    live_per_chunk: Vec<u32>,

    /// The owning heap's footprint (`gc::footprint`); a private one until the heap attaches its
    /// own (`attach_footprint`). `payload_of` measures what an entry's value holds outside the
    /// slot; `side_accounted` is the last side-table reading charged to it.
    footprint: std::sync::Arc<crate::gc::footprint::Footprint>,
    payload_of: fn(&T) -> u64,
    side_accounted: u64,
    /// Pooled-chunk decommit (`region/decommit.rs`): per chunk, whether its pages are given
    /// back and the generation its slots restart from; and how many chunks at the **front** of
    /// `free_chunk_pool` are decommitted (`borrow_chunk` pops from the back, so committed
    /// chunks are reused first).
    decommitted: Vec<bool>,
    gen_floor: Vec<u32>,
    pool_decommitted: usize,

    _phantom: PhantomData<T>,
}

impl<T> Default for Region<T> {
    fn default() -> Self {
        Self {
            chunks:      Vec::new(),
            slabs:       Slabs::default(),
            ambient_cur: None,
            free_bits:   Vec::new(),
            free_chunks: Vec::new(),
            free_len:    0,
            init_bits:   Vec::new(),
            young_bits:  Vec::new(),
            young_chunks: Vec::new(),
            young_len:   0,
            // fix-young-list-only-when-generational: `Default` (and therefore
            // `new()`) keeps the pre-2026-09-07 behaviour — maintain the list.
            // The heap passes the resolved mode via `new_for_mode`; only tests
            // and other direct constructors land here.
            generational: true,
            card_dirty:  Vec::new(),
            borrowed:        Vec::new(),
            free_chunk_pool: Vec::new(),
            promotion_age: PROMOTION_THRESHOLD,
            init_per_chunk: Vec::new(),
            live_per_chunk: Vec::new(),
            footprint: Default::default(),
            payload_of: |_| 0,
            side_accounted: 0,
            decommitted: Vec::new(),
            gen_floor: Vec::new(),
            pool_decommitted: 0,
            _phantom:    PhantomData,
        }
    }
}

impl<T> Region<T> {
    pub fn new() -> Self {
        Self::default()
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
    /// a new chunk first.
    pub fn alloc(&mut self, value: T) -> RegionHandle {
        if let Some((ci, ei)) = self.pop_free_slot() {
            // Slot is initialized (we tombstoned it previously). Drop
            // the dead RegionEntry, write a fresh one preserving the
            // bumped generation.
            let chunk = &mut self.chunks[ci as usize];
            // SAFETY: slot was init at first alloc; we're reading the
            // current RegionEntry to extract its generation, then
            // overwriting in place. Dropping a `RegionEntry<T>` runs
            // its Mutex / AtomicU8 / etc. Drop impls — all safe.
            let slot = unsafe { chunk[ei as usize].assume_init_mut() };
            let generation = slot.generation.load(Ordering::Acquire);
            let freed = (self.payload_of)(slot.value.get_mut());
            self.footprint.apply((self.payload_of)(&value) as i64 - freed as i64);
            // Replace the entry in place. Drop the old, write new.
            let new_entry = RegionEntry::new(value, (ci, ei));
            // Manually preserve the generation across the replacement.
            new_entry.generation.store(generation, Ordering::Release);
            // SAFETY: ptr-write replaces the old entry with new.
            // The old's Drop runs as part of the assignment.
            *slot = new_entry;
            // A recycled slot never moves, so only the live count changes.
            self.live_per_chunk[ci as usize] += 1;
            // add-generational-gc P0: reused slot starts at gen_age=0 (young).
            self.push_young(ci, ei);
            return RegionHandle { chunk_idx: ci, entry_idx: ei, generation: generation };
        }

        // Bump pointer (ambient path). add-gc-tlab: bump into the ambient
        // cursor's chunk; when it's absent or full, grow a *fresh*
        // ambient-exclusive chunk (never a pooled/borrowed one — those belong
        // to the TLAB path). This keeps ambient off any borrowed chunk index.
        let (ci, ei) = match self.ambient_cur {
            Some((c, e)) if (e as usize) < CHUNK_SIZE => (c, e),
            _ => (self.grow_new_chunk(), 0),
        };
        self.footprint.charge((self.payload_of)(&value));
        let chunk = &mut self.chunks[ci as usize];
        chunk[ei as usize] = MaybeUninit::new(RegionEntry::new(value, (ci, ei)));
        if side_bits::set(&mut self.init_bits[ci as usize], ei as usize) {
            self.init_per_chunk[ci as usize] += 1;
        }
        self.live_per_chunk[ci as usize] += 1;
        // add-generational-gc P0: track newly-allocated entry as young.
        self.push_young(ci, ei);
        // Advance the ambient cursor (ei+1 == CHUNK_SIZE → next alloc grows fresh).
        self.ambient_cur = Some((ci, ei + 1));

        RegionHandle { chunk_idx: ci, entry_idx: ei, generation: 0 }
    }

    /// Record slot `(ci, ei)` as reusable.
    ///
    /// The empty test is what maintains `ci ∈ free_chunks ⟺ free_bits[ci] ≠ 0`: the chunk is
    /// listed exactly when its bits go empty → non-empty, and delisted in
    /// [`Self::pop_free_slot`] / [`Self::reclaim_dead_chunks`] when they go back.
    #[inline]
    fn push_free_slot(&mut self, ci: u32, ei: u16) {
        let bits = &mut self.free_bits[ci as usize];
        if side_bits::is_empty(bits) {
            self.free_chunks.push(ci);
        }
        if side_bits::set(bits, ei as usize) {
            self.free_len += 1;
        }
    }

    /// Take a reusable slot, or `None`.
    ///
    /// Drains one chunk before moving to the next: reuse stays inside one chunk (better
    /// locality), and concentrating it there leaves the *other* dead chunks wholly dead, which
    /// is exactly the condition [`Self::reclaim_dead_chunks`] pools on.
    #[inline]
    fn pop_free_slot(&mut self) -> Option<(u32, u16)> {
        let ci = *self.free_chunks.last()?;
        let bits = &mut self.free_bits[ci as usize];
        let ei = side_bits::first(bits).expect("free_chunks only lists chunks with a free slot");
        side_bits::clear(bits, ei);
        if side_bits::is_empty(bits) {
            self.free_chunks.pop();
        }
        self.free_len -= 1;
        Some((ci, ei as u16))
    }

    /// Append a brand-new, fully-uninitialized chunk to `chunks` and grow every parallel
    /// per-chunk table (bitmaps empty, `card_dirty` 0, `borrowed` false). Returns the new chunk
    /// index. Shared by the ambient bump grow and [`Self::borrow_chunk`]'s pool-miss path — the
    /// single point where `chunks` grows, so all per-chunk tables stay length-consistent
    /// (validated by `CardDirtyLengthMismatch`).
    fn grow_new_chunk(&mut self) -> u32 {
        let chunk = self.slabs.carve();
        let ci = self.chunks.len() as u32;
        self.chunks.push(chunk);
        self.decommitted.push(false);
        self.gen_floor.push(0);
        self.free_bits.push(SlotBits::default());
        self.init_bits.push(SlotBits::default());
        self.young_bits.push(SlotBits::default());
        if self.chunks.len() > self.young_chunks.len() * 64 {
            self.young_chunks.push(0);
        }
        self.init_per_chunk.push(0);
        self.live_per_chunk.push(0);
        self.card_dirty.push(0);
        self.borrowed.push(false);
        self.footprint.charge(Self::CHUNK_FOOTPRINT);
        ci
    }

    /// Resolve a handle to a `&RegionEntry<T>` reference. Panics if
    /// the handle's chunk/entry is out of bounds (programmer error;
    /// should never happen with valid `GcRef`).
    ///
    /// Does NOT check generation or alive — that's the caller's job
    /// (different paths want different responses: `WeakGcRef::upgrade`
    /// returns None, `GcRef::borrow` panics).
    pub fn resolve(&self, handle: RegionHandle) -> &RegionEntry<T> {
        let chunk = &self.chunks[handle.chunk_idx as usize];
        let slot = &chunk[handle.entry_idx as usize];
        // SAFETY: the handle was constructed via `alloc`, which sets
        // the slot's `init_bits` bit. As long as the handle came
        // from this Region (typestate), the slot is init.
        unsafe { slot.assume_init_ref() }
    }

    /// Tombstone the entry pointed to by `handle`. Sets `alive=false`,
    /// bumps generation, pushes slot to free_list. Does NOT call the
    /// finalizer — that's the caller's responsibility (sweep extracts
    /// + invokes the finalizer separately).
    ///
    /// Returns `false` if the handle's generation no longer matches
    /// (slot was already tombstoned + reused — stale handle). In that
    /// case the call is a no-op.
    ///
    /// Also clears the slot's young bit — `O(1)`, so the young set never holds the dead.
    pub fn tombstone(&mut self, handle: RegionHandle) -> bool {
        let entry = self.resolve(handle);
        if entry.generation.load(Ordering::Acquire) != handle.generation {
            return false;
        }
        if !entry.alive.load(Ordering::Acquire) {
            return false;
        }
        entry.alive.store(false, Ordering::Release);
        entry.generation.fetch_add(1, Ordering::AcqRel);
        // add-incremental-chunk-reclaim: O(1) — the handle already names the chunk.
        self.live_per_chunk[handle.chunk_idx as usize] -= 1;
        self.push_free_slot(handle.chunk_idx, handle.entry_idx);
        self.remove_young(handle.chunk_idx, handle.entry_idx);
        true
    }

    /// **one-pass-major-sweep (2026-09-13)**: the major's whole sweep of this region, in
    /// **one** walk. `prepare_dead` is the caller's business with a dying entry — size
    /// estimate, breaking its edges, taking its finalizer — and runs with the entry still
    /// readable. The finalizer runs after it, then the tombstone.
    pub fn sweep_all_in_one_pass(
        &mut self,
        major: crate::gc::refs::MarkKind,
        prepare_dead: impl FnMut(&RegionEntry<T>) -> (Option<crate::gc::types::FinalizerFn>, u64),
    ) -> (u64, usize) {
        let (freed_bytes, reclaimed, _) = self.sweep_chunks(major, 0, usize::MAX, prepare_dead);
        (freed_bytes, reclaimed)
    }

    /// Number of chunks — the bound of a [`Self::sweep_chunks`] cursor.
    #[inline]
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// **add-incremental-major-gc M2b**: the major sweep over at most `max_chunks` chunks starting at
    /// chunk `from`. Returns `(freed bytes, reclaimed entries, next chunk)`; `next ==
    /// self.chunk_count()` means the region is done. Resumable across STW slices: between two calls
    /// mutators may refill chunks (a pooled one, or a new one past the cursor), but everything they
    /// allocate during a cycle is born with the cycle's epoch, so the rest of the walk keeps it.
    /// Every mutator has retired its TLAB before a slice runs, so no chunk here is borrowed.
    ///
    ///
    /// Each dead entry leaves the young set as it is tombstoned: minors run between the slices
    /// of an incremental sweep, and they walk the young set.
    pub fn sweep_chunks(
        &mut self,
        major: crate::gc::refs::MarkKind,
        from: usize,
        max_chunks: usize,
        mut prepare_dead: impl FnMut(&RegionEntry<T>) -> (Option<crate::gc::types::FinalizerFn>, u64),
    ) -> (u64, usize, usize) {
        let mut freed_bytes: u64 = 0;
        let mut reclaimed = 0usize;
        let end = from.saturating_add(max_chunks).min(self.chunks.len());
        for ci in from.min(end)..end {
            debug_assert!(!self.borrowed[ci], "major sweep over a TLAB-borrowed chunk {ci}");
            // A copy: tombstoning below does not touch `init_bits`, but the walk must not
            // hold a borrow of `self` across it.
            let init = self.init_bits[ci];
            side_bits::for_each(&init, |ei| {
                // SAFETY: an initialized slot holds a constructed entry.
                let entry = unsafe { self.chunks[ci][ei].assume_init_ref() };
                if !entry.alive.load(Ordering::Acquire) {
                    return;
                }
                if entry.is_marked(major) {
                    // add-incremental-major-gc M1: the epoch stays (it is what makes this
                    // survivor white again next cycle); only a stray minor bit is cleared.
                    entry.clear_minor_mark();
                    return;
                }
                let (fin, size) = prepare_dead(entry);
                let h = RegionHandle {
                    chunk_idx: ci as u32,
                    entry_idx: ei as u16,
                    generation: entry.generation.load(Ordering::Acquire),
                };
                if let Some(f) = fin {
                    f();
                }
                if self.tombstone(h) {
                    freed_bytes += size;
                    reclaimed += 1;
                }
            });
        }
        (freed_bytes, reclaimed, end.max(from.min(self.chunks.len())))
    }

    /// Iterate every currently-alive entry. Skips never-filled and tombstoned slots. Order:
    /// chunk 0 → chunk N, entry 0 → CHUNK_SIZE-1 within.
    pub fn iterate_alive(&self, mut visit: impl FnMut(RegionHandle, &RegionEntry<T>)) {
        for (ci, chunk) in self.chunks.iter().enumerate() {
            // add-gc-tlab: a borrowed chunk is being lock-free filled by its
            // owning mutator — skip it (its slots are invisible to GC until
            // retire merges them). Under STW no chunk is borrowed.
            if self.borrowed[ci] {
                continue;
            }
            side_bits::for_each(&self.init_bits[ci], |ei| {
                // SAFETY: an initialized slot holds a constructed entry.
                let entry = unsafe { chunk[ei].assume_init_ref() };
                if !entry.alive.load(Ordering::Acquire) {
                    return;
                }
                let h = RegionHandle {
                    chunk_idx:  ci as u32,
                    entry_idx:  ei as u16,
                    generation: entry.generation.load(Ordering::Acquire),
                };
                visit(h, entry);
            });
        }
    }

    /// **add-custom-allocator P2 (2026-05-22)**: tombstone an entry
    /// using only the entry reference (no separate handle). Uses the
    /// entry's self-recorded `location` to push the slot back into the
    /// free list. Idempotent: if alive is already false, no-op.
    /// Returns `true` if this call actually tombstoned (alive 1→0).
    ///
    /// The `u32::MAX` chunk sentinel (test-only entries from
    /// `GcRef::new` Box::leak) skips the free-list push — those
    /// entries aren't in any Region, just leaked.
    ///
    /// Also takes the entry out of the young set.
    pub fn tombstone_via_entry(&mut self, entry: &RegionEntry<T>) -> bool {
        if !entry.alive.swap(false, Ordering::Release) {
            return false;
        }
        entry.generation.fetch_add(1, Ordering::AcqRel);
        let (ci, ei) = entry.location();
        if ci != u32::MAX {
            self.live_per_chunk[ci as usize] -= 1;
            self.push_free_slot(ci, ei);
            self.remove_young(ci, ei);
        }
        true
    }

    /// Number of alive entries (linear scan). Mostly for tests +
    /// diagnostics; production uses stats counters.
    pub fn alive_count(&self) -> usize {
        let mut n = 0;
        self.iterate_alive(|_, _| n += 1);
        n
    }

    /// Total slot capacity across all chunks. `alive_count <= total <=
    /// chunks.len() * CHUNK_SIZE`.
    #[allow(dead_code)]
    pub(crate) fn total_capacity(&self) -> usize {
        self.chunks.len() * CHUNK_SIZE
    }

    // ── add-gc-tlab (2026-08-29): chunk borrow / retire / reclaim ────────────

    /// **add-gc-tlab**: hand a whole chunk's write ownership to a mutator
    /// thread's TLAB (design D1/D2). Pops a normalized fully-dead chunk from
    /// `free_chunk_pool` (→ `reused = true`, every slot a constructed dead
    /// generation-preserved entry) or grows a brand-new one (→ `reused =
    /// false`, uninitialized slots). Marks the chunk `borrowed` so every
    /// region-lock iterate skips it until [`retire_chunk`]. The returned
    /// [`ChunkClaim`] carries a raw pointer to the chunk's slot array — stable
    /// for the region's lifetime because chunks live in slabs (never moved or
    /// unmapped while the region lives). Caller (owning thread) fills lock-free via
    /// [`ChunkClaim::fill`].
    pub fn borrow_chunk(&mut self) -> ChunkClaim<T> {
        let ci = match self.free_chunk_pool.pop() {
            Some(ci) => {
                self.take_pooled(ci);
                ci
            }
            None => self.grow_new_chunk(),
        };
        self.borrowed[ci as usize] = true;
        let slots = self.chunks[ci as usize].as_mut_ptr();
        // A copy of the chunk's constructed-slot bits: `fill` reads it per slot to pick
        // fresh-write (uninit) vs generation-preserving overwrite (constructed dead entry from
        // a pooled chunk). Nothing changes a borrowed chunk's bits until `retire_chunk`.
        let init = self.init_bits[ci as usize];
        ChunkClaim {
            chunk_idx: ci, slots, init, next: 0, cap: CHUNK_SIZE as u16,
            payload_of: self.payload_of, payload_delta: 0, gen_floor: self.gen_floor[ci as usize],
        }
    }

    /// **add-gc-tlab**: merge a TLAB's filled chunk prefix `[0, claim.next)`
    /// back into the shared region (design D2). Marks those slots
    /// constructed (idempotent for a reused chunk) and young (every fresh
    /// alloc is young, gen_age 0) — two word-wide ORs — then clears the
    /// `borrowed` flag so the chunk rejoins GC iteration as ordinary populated
    /// slots. A partially-filled chunk's tail `[claim.next, cap)` stays as it
    /// was (uninitialized for a fresh chunk, old dead entries for a reused
    /// one) — that tail capacity is abandoned (bounded ≤ CHUNK_SIZE-1 per
    /// safepoint retire; reclaimed wholesale when the chunk later dies).
    /// Stats are flushed lock-free per-object by the fast path, so retire does
    /// no stats work.
    pub fn retire_chunk(&mut self, claim: &ChunkClaim<T>) {
        let ci = claim.chunk_idx;
        let hw = claim.next as usize;
        let filled: SlotBits = side_bits::prefix(hw);
        let init_row = &mut self.init_bits[ci as usize];
        let mut newly_init = 0u32;
        for (w, f) in init_row.iter_mut().zip(filled) {
            newly_init += (f & !*w).count_ones();
            *w |= f;
        }
        // add-incremental-chunk-reclaim: the TLAB filled these lock-free; the region only
        // learns of them here, so this is where its per-chunk census picks them up.
        self.init_per_chunk[ci as usize] += newly_init;
        self.live_per_chunk[ci as usize] += hw as u32;
        self.push_young_bits(ci, &filled);
        self.footprint.apply(claim.payload_delta);
        self.borrowed[ci as usize] = false;
    }

    /// **add-gc-tlab (D7)**: after a sweep, move every **fully dead** chunk
    /// (all slots tombstoned) into `free_chunk_pool` for [`borrow_chunk`] to
    /// recycle. Normalizes each pooled chunk so its whole `[0, CHUNK_SIZE)`
    /// slot range is a constructed, dead, generation-preserved `RegionEntry`
    /// (a partial-retire tail that was never initialized gets filled with a
    /// fresh dead gen-0 entry — safe because such slots were never handed out,
    /// so no stale handle can alias them). This lets [`ChunkClaim::fill`] use a
    /// single uniform "reused" write mode across the whole chunk while
    /// preserving each slot's tombstone generation (ABA guard). Purges the
    /// pooled chunk's slots from the free set (else the ambient slot-reuse path
    /// could hand out a slot inside a soon-to-be-borrowed chunk). Skips the
    /// ambient cursor chunk and any already-borrowed/pooled chunk. Runs under
    /// STW at the sweep tail.
    ///
    /// Returns the number of chunks reclaimed (diagnostics/tests).
    pub fn reclaim_dead_chunks(&mut self) -> usize {
        let ambient_ci = self.ambient_cur.map(|(c, _)| c);
        // Flag tables rather than hash sets: both of these are probed once per chunk here and
        // once per free-list entry below, and the free list runs to hundreds of thousands of
        // slots — a hash lookup per element was the cost left over after the census made the
        // scan itself `O(chunks)`.
        let mut already_pooled = vec![false; self.chunks.len()];
        for &ci in &self.free_chunk_pool {
            already_pooled[ci as usize] = true;
        }
        let mut reclaimed: Vec<u32> = Vec::new();

        for ci in 0..self.chunks.len() as u32 {
            if self.borrowed[ci as usize]
                || already_pooled[ci as usize]
                || ambient_ci == Some(ci)
            {
                continue;
            }
            // A chunk is reclaimable iff every constructed slot is dead AND it has at least
            // one (never-touched chunks have no storage to recycle and no free_list entries
            // to purge — skip).
            //
            // **add-incremental-chunk-reclaim (2026-09-10)**: both facts come from the
            // per-chunk census that alloc / retire / tombstone keep current. This used to
            // scan every slot of every chunk — `O(chunks × CHUNK_SIZE)` on every collection,
            // measured at 3.2 ms (objects) + 3.3 ms (arrays) of a 54 ms minor sweep.
            if self.init_per_chunk[ci as usize] > 0 && self.live_per_chunk[ci as usize] == 0 {
                reclaimed.push(ci);
            }
        }

        if reclaimed.is_empty() {
            return 0;
        }
        let mut is_reclaimed = vec![false; self.chunks.len()];
        for &ci in &reclaimed {
            is_reclaimed[ci as usize] = true;
        }
        // Take a reclaimed chunk's slots out of the free set (else the ambient slot-reuse path
        // could hand out a slot inside a borrowed chunk): four words per chunk reclaimed.
        for &ci in &reclaimed {
            let bits = &mut self.free_bits[ci as usize];
            self.free_len -= side_bits::count(bits);
            *bits = SlotBits::default();
        }
        // Keeps `ci ∈ free_chunks ⟺ free_bits[ci] ≠ 0`. `free_chunks` holds at most one entry
        // per chunk, so this is the same `O(chunks)` the reclaim scan above already pays.
        self.free_chunks.retain(|&ci| !is_reclaimed[ci as usize]);
        // No normalization: a reclaimed chunk may be mixed (initialized dead
        // slots + a never-initialized tail). `ChunkClaim::fill` consults the
        // chunk's constructed-slot bits per slot — preserving the tombstone
        // generation for constructed slots (ABA guard) and writing fresh gen-0
        // entries into never-initialized ones (safe: never handed out).
        for &ci in &reclaimed {
            self.free_chunk_pool.push(ci);
            self.footprint.pool(Self::CHUNK_FOOTPRINT, true);
        }
        reclaimed.len()
    }

    /// **add-generational-gc P0 (2026-05-22)**: chunk count for tests
    /// + diagnostics.
    #[cfg(test)]
    pub(crate) fn chunks_count_for_test(&self) -> usize {
        self.chunks.len()
    }

    /// **add-bounded-nursery (2026-09-08)**: how many fully-dead chunks are waiting in the
    /// pool for `borrow_chunk` to recycle (tests: proves a minor gave chunks back).
    #[cfg(test)]
    pub(crate) fn free_chunk_pool_len_for_test(&self) -> usize {
        self.free_chunk_pool.len()
    }

    /// **perf-bucket-region-free-list (2026-09-13)**: which chunks are pooled (tests: prove a
    /// reclaimed chunk's slots left the free lists).
    #[cfg(test)]
    pub(crate) fn free_chunk_pool_for_test(&self) -> std::collections::HashSet<u32> {
        self.free_chunk_pool.iter().copied().collect()
    }

    /// Number of free slots available without growing (`free_list +
    /// remaining bump capacity in current chunk`). Used by P3 bench
    /// + diagnostics.
    #[allow(dead_code)]
    pub(crate) fn free_slot_count(&self) -> usize {
        let bump_remaining = match self.ambient_cur {
            Some((_, ei)) => CHUNK_SIZE - ei as usize,
            None => 0,
        };
        self.free_len + bump_remaining
    }
}

impl<T> Drop for Region<T> {
    /// Drop every initialized entry. Each entry's own Drop impl
    /// handles its Mutex + Atomic / etc. The `value: Mutex<T>` Drop
    /// runs `T::drop` for the user data — at this point the Region
    /// is being torn down (heap shutdown), so prompt user-data drop
    /// is appropriate.
    fn drop(&mut self) {
        for (ci, chunk) in self.chunks.iter_mut().enumerate() {
            side_bits::for_each(&self.init_bits[ci], |ei| {
                // SAFETY: initialized slot. Drop in place.
                unsafe { chunk[ei].assume_init_drop(); }
            });
        }
    }
}

// Tests call `Region::validate()` / `Violation`, which are
// `#[cfg(debug_assertions)]` only. Gate the module to match so
// `cargo build --release --lib --tests` doesn't try to compile against
// methods that don't exist in release builds.
// (fix-gc-tests-release-build 2026-05-27)
#[cfg(all(test, debug_assertions))]
#[path = "region_tests.rs"]
mod region_tests;
