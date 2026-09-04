//! `Region<T>` —— chunked region allocator backing for GC entries.
//!
//! **add-custom-allocator P0 (2026-05-22)**: replaces the per-object
//! `Arc<GcAllocation<T>>` storage. Each `Region<T>` owns
//! `Vec<Box<[MaybeUninit<RegionEntry<T>>; CHUNK_SIZE]>>` — chunks are
//! Box-owned and never relocate, so `RegionEntry` addresses remain
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
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU8, Ordering};

use parking_lot::Mutex;

use super::types::FinalizerFn;

/// Chunk capacity (entries per chunk). 256 balances:
/// - Per-chunk allocation cost (1 malloc per CHUNK_SIZE allocs amortizes)
/// - Cache locality for sweep traversal (chunk fits in ~16-64 KB depending on T)
/// - Granularity for future per-thread arenas (256 is a reasonable batch)
pub(crate) const CHUNK_SIZE: usize = 256;

/// Per-object slot inside a `Region<T>`. Holds the user data plus GC
/// metadata. Address stability: once a `RegionEntry` is initialized
/// inside a chunk, its `&self` reference remains valid until the
/// owning chunk's Box is dropped (which happens only when the Region
/// itself drops — never during normal sweep cycles).
pub struct RegionEntry<T> {
    /// User value. `Mutex` provides per-entry locking (preserves the
    /// multi-threading concurrency model). Access via
    /// `entry.value.lock()` from `GcRef::borrow` / `borrow_mut`.
    pub(crate) value: Mutex<T>,

    /// Mark bit (add-mark-sweep-collector + add-concurrent-gc). CAS
    /// from 0 to 1 by mark phase / write barrier. Sweep resets to 0
    /// on survivors. `Relaxed` ordering — visibility sync via the
    /// gc_phase Mutex / mark_queue Mutex established at sweep / drain
    /// boundaries.
    pub(crate) marked: AtomicU8,

    /// Tombstone flag. `true` while the slot holds a live user
    /// object; `false` after sweep reclaims it. `Acquire / Release`
    /// ordering pairs with `WeakGcRef::upgrade` reads + sweep writes
    /// (prevents reading half-tombstoned state).
    pub(crate) alive: AtomicBool,

    /// **add-generational-gc P0 (2026-05-22)**: generation age. 0 =
    /// young (fresh alloc); incremented at each minor GC the entry
    /// survives; >= `PROMOTION_THRESHOLD` means promoted to old gen.
    /// Lock-free atomic read for the write-barrier hot path
    /// (cross-gen detection). Promotion writes happen during STW
    /// minor sweep, so no race.
    pub(crate) gen_age: AtomicU8,

    /// Generation counter. Bumped on every tombstone. `GcRef` and
    /// `WeakGcRef` both record the generation at construction; access
    /// methods (`upgrade`, `borrow`) check the recorded generation
    /// matches the entry's current generation. Mismatch → entry was
    /// reclaimed + slot reused → return None / panic (per design D5).
    pub(crate) generation: AtomicU32,

    /// One-shot finalizer slot, as a **raw `Box<FinalizerFn>` pointer**
    /// (`null` = none). `swap(null)` gives the same fire-once `take()`
    /// semantics the sweep path relies on, atomically and lock-free.
    ///
    /// shrink-object-footprint P1: this was `Mutex<Option<FinalizerFn>>` =
    /// **24 bytes on every entry** (19% of a 128-byte `RegionEntry<ScriptObject>`)
    /// for a capability with **zero production registrations** — `grep
    /// register_finalizer` over `src/` hits only the trait, its impl, these
    /// accessors, and `arc_heap_tests/finalization.rs`. As an `AtomicPtr` it is
    /// 8 bytes, and only an entry that actually registers one pays the 16-byte
    /// box. Freed by this entry's `Drop`.
    pub(crate) finalizer: AtomicPtr<FinalizerFn>,

    /// **add-custom-allocator P2 (2026-05-22)**: self-location
    /// (chunk_idx, entry_idx) within the owning Region. Lets the
    /// `MagrGC::finalize_now` path tombstone + recycle this slot
    /// given only a `&RegionEntry<T>` (no separate handle needed).
    /// Set by `Region::alloc`; immutable thereafter for the entry's
    /// lifetime (a single slot keeps its location across reuse).
    /// fix-region-chunk-idx-u16-overflow (2026-08-21): chunk_idx widened u16→u32.
    /// A full 24-lib stdlib build bump-allocates past 65 535 chunks (× CHUNK_SIZE=256
    /// = 16.7M slots); the old u16 chunk index overflowed at `ci + 1`, wrapping
    /// `next_bump` to (0,0) → fresh allocations overwrote live chunk-0 objects →
    /// non-deterministic heap corruption. entry_idx stays u16 (CHUNK_SIZE ≤ 65 536).
    pub(crate) location: (u32, u16),

    /// **add-gc-softref (2026-05-26)**: count of live `SoftGcRef<T>`
    /// handles pointing at this entry. > 0 means the entry is
    /// soft-referenced; the GC revive pass may re-mark it before sweep
    /// when heap pressure is below the soft threshold. Incremented by
    /// `SoftGcRef::new`, decremented by `SoftGcRef::drop`. Uses
    /// `SeqCst` ordering to keep soft-ref count visible across threads
    /// (GC and mutator run concurrently in `ConcurrentMarkSweep`).
    pub(crate) soft_ref_count: AtomicU32,
}

/// shrink-object-footprint P1: the finalizer slot owns a `Box<FinalizerFn>`
/// (raw pointer, so the entry stays 8 bytes wider instead of 24) — free it when
/// the entry itself goes away, or the `Arc<dyn Fn>` inside leaks.
impl<T> Drop for RegionEntry<T> {
    fn drop(&mut self) {
        let raw = *self.finalizer.get_mut();
        if !raw.is_null() {
            // SAFETY: non-null ⇒ from `Box::into_raw` in `set_finalizer`; `&mut self`
            // means no other reference can observe the slot.
            drop(unsafe { Box::from_raw(raw) });
        }
    }
}

/// **add-generational-gc P0 (2026-05-22)**: number of minor GCs an
/// entry must survive before being promoted to old generation
/// (removed from `young_list`). Default = 2 (industry-standard Java
/// tenure). Configurable via `Z42_GC_TENURE` env var (P3 wiring).
pub const PROMOTION_THRESHOLD: u8 = 2;

impl<T> RegionEntry<T> {
    /// Test / transitional constructor used by `GcRef::new` for
    /// standalone (no-Region) allocations. Wraps a fresh entry with
    /// generation=0, alive=true. See refs.rs for the lifetime model
    /// (intentional leak — process-wide static). `location` is set to
    /// `(u32::MAX, u16::MAX)` — sentinel meaning "not in any Region"
    /// so `finalize_now` skips free-list bookkeeping for these
    /// standalone entries.
    pub fn new_for_test(value: T) -> Self {
        Self::new(value, (u32::MAX, u16::MAX))
    }

    /// **add-gc-tlab (2026-08-29)**: `pub(crate)` so the TLAB fast-fill path
    /// (`gc/tlab.rs::ChunkClaim::fill`) can construct entries directly into a
    /// borrowed chunk's raw slots without the region lock. Ambient `Region::alloc`
    /// still calls it internally.
    pub(crate) fn new(value: T, location: (u32, u16)) -> Self {
        Self {
            value:          Mutex::new(value),
            marked:         AtomicU8::new(0),
            alive:          AtomicBool::new(true),
            gen_age:        AtomicU8::new(0),
            generation:     AtomicU32::new(0),
            finalizer:      AtomicPtr::new(std::ptr::null_mut()),
            location,
            soft_ref_count: AtomicU32::new(0),
        }
    }

    /// shrink-object-footprint P1: install a finalizer, dropping any previous one.
    /// Fire-once semantics are unchanged — `take_finalizer` still swaps `null` in.
    pub(crate) fn set_finalizer(&self, fin: FinalizerFn) {
        let raw = Box::into_raw(Box::new(fin));
        let prev = self.finalizer.swap(raw, Ordering::AcqRel);
        if !prev.is_null() {
            // SAFETY: non-null ⇒ produced by `Box::into_raw` here, and the swap
            // gives this thread exclusive ownership of the old box.
            drop(unsafe { Box::from_raw(prev) });
        }
    }

    /// Take the finalizer, leaving the slot empty (fire-once).
    pub(crate) fn take_finalizer(&self) -> Option<FinalizerFn> {
        let raw = self.finalizer.swap(std::ptr::null_mut(), Ordering::AcqRel);
        if raw.is_null() {
            return None;
        }
        // SAFETY: see `set_finalizer` — the swap hands us sole ownership.
        Some(*unsafe { Box::from_raw(raw) })
    }

    /// Whether a finalizer is currently installed (no ownership transfer).
    pub(crate) fn has_finalizer(&self) -> bool {
        !self.finalizer.load(Ordering::Acquire).is_null()
    }

    /// **add-generational-gc P0 (2026-05-22)**: read current gen_age.
    /// Used by write barrier override under `GenerationalMarkSweep`
    /// mode to detect cross-gen writes.
    #[inline]
    pub fn gen_age(&self) -> u8 {
        self.gen_age.load(Ordering::Relaxed)
    }

    /// Atomically attempt to mark this entry (0 → 1). Returns `true`
    /// if this call won the CAS (first to mark in the current cycle).
    /// Used by mark phase BFS + concurrent barrier override.
    pub fn mark(&self) -> bool {
        self.marked
            .compare_exchange(0, 1, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    }

    /// Read current mark state. Used by sweep to decide retention.
    pub fn is_marked(&self) -> bool {
        self.marked.load(Ordering::Relaxed) != 0
    }

    /// Reset mark to 0. Used by sweep on survivors to prep next cycle.
    pub fn clear_mark(&self) {
        self.marked.store(0, Ordering::Relaxed);
    }

    /// Increment the soft-ref count for this entry. Called by `SoftGcRef::new`.
    #[inline]
    pub fn inc_soft_ref_count(&self) {
        self.soft_ref_count.fetch_add(1, Ordering::SeqCst);
    }

    /// Decrement the soft-ref count. Called by `SoftGcRef::drop`.
    #[inline]
    pub fn dec_soft_ref_count(&self) {
        self.soft_ref_count.fetch_sub(1, Ordering::SeqCst);
    }

    /// True when at least one `SoftGcRef` points to this entry.
    #[inline]
    pub fn has_soft_ref(&self) -> bool {
        self.soft_ref_count.load(Ordering::SeqCst) > 0
    }
}

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

/// **add-gc-tlab (2026-08-29)**: a mutator thread's exclusive write claim on
/// one region chunk (design D1/D2). Produced by [`Region::borrow_chunk`] (under
/// the region lock), then filled **lock-free** by the owning thread via
/// [`ChunkClaim::fill`] until the chunk is full (`next == cap`); the region
/// re-absorbs the filled prefix at [`Region::retire_chunk`].
///
/// # Safety / invariants
/// - `slots` / `init_ptr` are raw pointers into `Region`-owned, `Box`-stable
///   memory (chunk arrays + the chunk's `initialized` row, both fixed-size and
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
    slots: *mut MaybeUninit<RegionEntry<T>>,
    /// Raw pointer to the chunk's `initialized` row (`[bool; CHUNK_SIZE]`
    /// buffer). Read per slot in `fill` to choose write mode; only the region
    /// (owner thread) writes it, and never while filling.
    init_ptr: *const bool,
    /// Next free slot index within the chunk (bump cursor / high-water mark).
    next: u16,
    /// Chunk capacity (`CHUNK_SIZE`).
    cap: u16,
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
        let generation = if was_init {
            // SAFETY: initialized ⇒ constructed (dead) entry.
            let old = unsafe { slot.assume_init_ref() };
            let g = old.generation.load(Ordering::Acquire);
            let ne = RegionEntry::new(value, (self.chunk_idx, ei));
            ne.generation.store(g, Ordering::Release);
            // Overwrite: `*` assignment drops the old dead entry, then moves in.
            unsafe { *slot.assume_init_mut() = ne };
            g
        } else {
            slot.write(RegionEntry::new(value, (self.chunk_idx, ei)));
            0
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

/// Chunked region allocator. Owns user objects of type `T` plus
/// per-object GC metadata. See module-level docs for the allocation
/// + sweep model.
pub struct Region<T> {
    /// Chunks of pre-reserved entries. Each chunk is a fixed-size
    /// `Box<[MaybeUninit<RegionEntry<T>>; CHUNK_SIZE]>` so its
    /// address is stable for the chunk's lifetime.
    chunks: Vec<Box<[MaybeUninit<RegionEntry<T>>; CHUNK_SIZE]>>,

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

    /// Tombstoned slots reusable by fresh allocs. LIFO (Vec::pop).
    free_list: Vec<(u32, u16)>,

    /// Track initialized vs uninitialized slots. Bit `(ci, ei)` is
    /// set if the slot is initialized (was alloc'd at least once).
    /// Sweep uses this to skip never-allocated slots in the last
    /// chunk (where bump hasn't reached the end).
    ///
    /// One bool per slot — could compress to bitmap later; v1
    /// favors clarity.
    initialized: Vec<Vec<bool>>,

    /// **add-generational-gc P0 (2026-05-22)**: track young entries
    /// (gen_age < PROMOTION_THRESHOLD). Updated on alloc (push),
    /// promote (swap_remove once threshold reached), and tombstone
    /// (swap_remove if was young). Minor GC iterates this list for
    /// O(young) cost instead of walking all chunks.
    young_list: Vec<(u32, u16)>,

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
    /// region-lock iteration (`iterate_alive`/`iterate_young`/
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

    _phantom: PhantomData<T>,
}

impl<T> Default for Region<T> {
    fn default() -> Self {
        Self {
            chunks:      Vec::new(),
            ambient_cur: None,
            free_list:   Vec::new(),
            initialized: Vec::new(),
            young_list:  Vec::new(),
            card_dirty:  Vec::new(),
            borrowed:        Vec::new(),
            free_chunk_pool: Vec::new(),
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
        if let Some((ci, ei)) = self.free_list.pop() {
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
            // Replace the entry in place. Drop the old, write new.
            let new_entry = RegionEntry::new(value, (ci, ei));
            // Manually preserve the generation across the replacement.
            new_entry.generation.store(generation, Ordering::Release);
            // SAFETY: ptr-write replaces the old entry with new.
            // The old's Drop runs as part of the assignment.
            *slot = new_entry;
            // add-generational-gc P0: reused slot starts at gen_age=0 (young).
            self.young_list.push((ci, ei));
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
        let chunk = &mut self.chunks[ci as usize];
        chunk[ei as usize] = MaybeUninit::new(RegionEntry::new(value, (ci, ei)));
        self.initialized[ci as usize][ei as usize] = true;
        // add-generational-gc P0: track newly-allocated entry as young.
        self.young_list.push((ci, ei));
        // Advance the ambient cursor (ei+1 == CHUNK_SIZE → next alloc grows fresh).
        self.ambient_cur = Some((ci, ei + 1));

        RegionHandle { chunk_idx: ci, entry_idx: ei, generation: 0 }
    }

    /// **add-gc-tlab (2026-08-29)**: append a brand-new, fully-uninitialized
    /// chunk to `chunks` and grow every parallel per-chunk table
    /// (`initialized` all-false, `card_dirty` 0, `borrowed` false). Returns
    /// the new chunk index. Shared by the ambient bump grow and
    /// [`borrow_chunk`]'s pool-miss path — the single point where `chunks`
    /// grows, so all per-chunk tables stay length-consistent (validated by
    /// `CardDirtyLengthMismatch`).
    fn grow_new_chunk(&mut self) -> u32 {
        // SAFETY: MaybeUninit<RegionEntry<T>> is valid to leave uninit.
        let chunk: Box<[MaybeUninit<RegionEntry<T>>; CHUNK_SIZE]> = Box::new(unsafe {
            MaybeUninit::<[MaybeUninit<RegionEntry<T>>; CHUNK_SIZE]>::uninit().assume_init()
        });
        let ci = self.chunks.len() as u32;
        self.chunks.push(chunk);
        self.initialized.push(vec![false; CHUNK_SIZE]);
        self.card_dirty.push(0);
        self.borrowed.push(false);
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
        // initialized[ci][ei] = true. As long as the handle came
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
    /// **add-generational-gc P0 (2026-05-22)**: also removes the
    /// (chunk_idx, entry_idx) from `young_list` if the tombstoned
    /// entry was still young (gen_age < PROMOTION_THRESHOLD). Old
    /// entries weren't in young_list, so the lookup is a no-op for
    /// them.
    pub fn tombstone(&mut self, handle: RegionHandle) -> bool {
        let entry = self.resolve(handle);
        if entry.generation.load(Ordering::Acquire) != handle.generation {
            return false;
        }
        if !entry.alive.load(Ordering::Acquire) {
            return false;
        }
        let was_young = entry.gen_age() < PROMOTION_THRESHOLD;
        entry.alive.store(false, Ordering::Release);
        entry.generation.fetch_add(1, Ordering::AcqRel);
        self.free_list.push((handle.chunk_idx, handle.entry_idx));
        if was_young {
            self.remove_from_young_list(handle.chunk_idx, handle.entry_idx);
        }
        true
    }

    /// **add-generational-gc P0 (2026-05-22)**: helper to remove a
    /// `(chunk_idx, entry_idx)` pair from `young_list` via
    /// `swap_remove` (O(young_list.len()) lookup — acceptable since
    /// tombstone is sweep-time work, not the alloc hot path).
    fn remove_from_young_list(&mut self, ci: u32, ei: u16) {
        if let Some(pos) = self.young_list.iter().position(|&p| p == (ci, ei)) {
            self.young_list.swap_remove(pos);
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

    /// Iterate every currently-alive entry. Skips uninit slots in
    /// the last chunk (bump hasn't reached the end) and tombstoned
    /// slots. Order: chunk 0 → chunk N, entry 0 → CHUNK_SIZE-1 within.
    pub fn iterate_alive(&self, mut visit: impl FnMut(RegionHandle, &RegionEntry<T>)) {
        for (ci, chunk) in self.chunks.iter().enumerate() {
            // add-gc-tlab: a borrowed chunk is being lock-free filled by its
            // owning mutator — skip it (its slots are invisible to GC until
            // retire merges them). Under STW no chunk is borrowed.
            if self.borrowed[ci] {
                continue;
            }
            for ei in 0..CHUNK_SIZE {
                if !self.initialized[ci][ei] {
                    continue;
                }
                let slot = &chunk[ei];
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

    /// **add-custom-allocator P2 (2026-05-22)**: tombstone an entry
    /// using only the entry reference (no separate handle). Uses the
    /// entry's self-recorded `location` to push the slot back into the
    /// free list. Idempotent: if alive is already false, no-op.
    /// Returns `true` if this call actually tombstoned (alive 1→0).
    ///
    /// The `(u16::MAX, u16::MAX)` sentinel (test-only entries from
    /// `GcRef::new` Box::leak) skips the free-list push — those
    /// entries aren't in any Region, just leaked.
    ///
    /// **add-generational-gc P0 (2026-05-22)**: also removes from
    /// `young_list` if the entry was young.
    pub fn tombstone_via_entry(&mut self, entry: &RegionEntry<T>) -> bool {
        if !entry.alive.swap(false, Ordering::Release) {
            return false;
        }
        let was_young = entry.gen_age() < PROMOTION_THRESHOLD;
        entry.generation.fetch_add(1, Ordering::AcqRel);
        let (ci, ei) = entry.location;
        if ci != u32::MAX {
            self.free_list.push((ci, ei));
            if was_young {
                self.remove_from_young_list(ci, ei);
            }
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
    /// for the chunk's lifetime because chunks are `Box`-owned (never move when
    /// `chunks` reallocs). Caller (owning thread) fills lock-free via
    /// [`ChunkClaim::fill`].
    pub fn borrow_chunk(&mut self) -> ChunkClaim<T> {
        let ci = match self.free_chunk_pool.pop() {
            Some(ci) => ci,
            None => self.grow_new_chunk(),
        };
        self.borrowed[ci as usize] = true;
        let slots = self.chunks[ci as usize].as_mut_ptr();
        // `init_ptr` points at the chunk's `initialized` row buffer (fixed
        // CHUNK_SIZE, never resized → stable). `fill` reads it per slot to pick
        // fresh-write (uninit) vs generation-preserving overwrite (constructed
        // dead entry from a pooled chunk).
        let init_ptr = self.initialized[ci as usize].as_ptr();
        ChunkClaim { chunk_idx: ci, slots, init_ptr, next: 0, cap: CHUNK_SIZE as u16 }
    }

    /// **add-gc-tlab**: merge a TLAB's filled chunk prefix `[0, claim.next)`
    /// back into the shared region (design D2). Marks those slots
    /// `initialized` (idempotent for a reused chunk) and pushes them onto
    /// `young_list` (every fresh alloc is young, gen_age 0), then clears the
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
        let init_row = &mut self.initialized[ci as usize];
        for ei in 0..hw {
            init_row[ei] = true;
            self.young_list.push((ci, ei as u16));
        }
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
    /// pooled chunk's slots from `free_list` (else the ambient slot-reuse path
    /// could hand out a slot inside a soon-to-be-borrowed chunk). Skips the
    /// ambient cursor chunk and any already-borrowed/pooled chunk. Runs under
    /// STW at the sweep tail.
    ///
    /// Returns the number of chunks reclaimed (diagnostics/tests).
    pub fn reclaim_dead_chunks(&mut self) -> usize {
        let ambient_ci = self.ambient_cur.map(|(c, _)| c);
        let already_pooled: std::collections::HashSet<u32> =
            self.free_chunk_pool.iter().copied().collect();
        let mut reclaimed: Vec<u32> = Vec::new();

        for ci in 0..self.chunks.len() as u32 {
            if self.borrowed[ci as usize]
                || already_pooled.contains(&ci)
                || ambient_ci == Some(ci)
            {
                continue;
            }
            // A chunk is reclaimable iff every initialized slot is dead AND it
            // has at least one initialized slot (never-touched chunks have no
            // storage to recycle and no free_list entries to purge — skip).
            let init_row = &self.initialized[ci as usize];
            let mut any_init = false;
            let mut all_dead = true;
            for ei in 0..CHUNK_SIZE {
                if !init_row[ei] {
                    continue;
                }
                any_init = true;
                // SAFETY: initialized slot → constructed entry.
                let entry = unsafe { self.chunks[ci as usize][ei].assume_init_ref() };
                if entry.alive.load(Ordering::Acquire) {
                    all_dead = false;
                    break;
                }
            }
            if any_init && all_dead {
                reclaimed.push(ci);
            }
        }

        if reclaimed.is_empty() {
            return 0;
        }
        let reclaimed_set: std::collections::HashSet<u32> = reclaimed.iter().copied().collect();
        // Purge free_list of any slot inside a reclaimed chunk (else the ambient
        // slot-reuse path could hand out a slot inside a borrowed chunk).
        self.free_list.retain(|&(ci, _)| !reclaimed_set.contains(&ci));
        // No normalization: a reclaimed chunk may be mixed (initialized dead
        // slots + a never-initialized tail). `ChunkClaim::fill` consults the
        // chunk's `initialized` row per slot — preserving the tombstone
        // generation for constructed slots (ABA guard) and writing fresh gen-0
        // entries into never-initialized ones (safe: never handed out).
        for &ci in &reclaimed {
            self.free_chunk_pool.push(ci);
        }
        reclaimed.len()
    }

    /// **add-generational-gc P0 (2026-05-22)**: chunk count for tests
    /// + diagnostics.
    #[cfg(test)]
    pub(crate) fn chunks_count_for_test(&self) -> usize {
        self.chunks.len()
    }

    /// **add-gc-debug-invariants P1 (2026-05-22)**: test-only corruption
    /// injection helper — clears `young_list` directly so the next
    /// `validate()` reports `YoungEntryNotInList`.
    #[cfg(test)]
    pub(crate) fn clear_young_list_for_test(&mut self) {
        self.young_list.clear();
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
        self.free_list.len() + bump_remaining
    }
}

// ── add-gc-debug-invariants P0 (2026-05-22) ─────────────────────────────────

/// Per-region invariant violation. Returned by [`Region::validate`].
/// Variants 来自 add-write-barriers / add-custom-allocator /
/// add-concurrent-gc / add-generational-gc design 段的 invariants。
#[cfg(debug_assertions)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    /// young_list 中找到 gen_age >= PROMOTION_THRESHOLD 的 entry
    /// (generational invariant).
    OldEntryInYoungList { chunk_idx: u32, entry_idx: u16, gen_age: u8 },
    /// alive young entry (gen_age < threshold) 不在 young_list 中
    /// (generational invariant).
    YoungEntryNotInList { chunk_idx: u32, entry_idx: u16 },
    /// young_list 中同一 (ci, ei) 出现多次（违反 swap_remove 契约）.
    DuplicateInYoungList { chunk_idx: u32, entry_idx: u16 },
    /// free_list 中找到 alive=true 的 slot（违反 tombstone 契约 —
    /// custom-allocator invariant）.
    AliveSlotInFreeList { chunk_idx: u32, entry_idx: u16 },
    /// `entry.location` 不等于实际 (chunk_idx, entry_idx)（自定位错乱 —
    /// custom-allocator invariant）.
    LocationMismatch { chunk_idx: u32, entry_idx: u16, recorded: (u32, u16) },
    /// `card_dirty.len()` 与 `chunks.len()` 不一致（generational invariant；
    /// alloc-time grow 应保持一一对应）.
    CardDirtyLengthMismatch { expected: usize, actual: usize },
}

#[cfg(debug_assertions)]
impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OldEntryInYoungList { chunk_idx, entry_idx, gen_age } =>
                write!(f, "young_list contains old entry (chunk={}, entry={}, gen_age={})",
                    chunk_idx, entry_idx, gen_age),
            Self::YoungEntryNotInList { chunk_idx, entry_idx } =>
                write!(f, "alive young entry not in young_list (chunk={}, entry={})",
                    chunk_idx, entry_idx),
            Self::DuplicateInYoungList { chunk_idx, entry_idx } =>
                write!(f, "duplicate in young_list (chunk={}, entry={})",
                    chunk_idx, entry_idx),
            Self::AliveSlotInFreeList { chunk_idx, entry_idx } =>
                write!(f, "free_list contains alive slot (chunk={}, entry={})",
                    chunk_idx, entry_idx),
            Self::LocationMismatch { chunk_idx, entry_idx, recorded } =>
                write!(f, "location mismatch at ({}, {}): entry.location = ({}, {})",
                    chunk_idx, entry_idx, recorded.0, recorded.1),
            Self::CardDirtyLengthMismatch { expected, actual } =>
                write!(f, "card_dirty length mismatch: expected {}, actual {}",
                    expected, actual),
        }
    }
}

impl<T> Region<T> {
    /// **add-gc-debug-invariants P0 (2026-05-22)**: validate region
    /// internal invariants. Returns `Ok(())` on a healthy region; the
    /// first violation found is returned as `Err(Violation)` so test
    /// fixtures can pattern-match a specific variant.
    ///
    /// Cost: O(chunks * CHUNK_SIZE + young_list + free_list) =
    /// O(total slots). Acceptable on collect timescale (µs-ms);
    /// would be too slow per-alloc.
    #[cfg(debug_assertions)]
    pub fn validate(&self) -> Result<(), Violation> {
        // 1. card_dirty length matches chunks count.
        if self.card_dirty.len() != self.chunks.len() {
            return Err(Violation::CardDirtyLengthMismatch {
                expected: self.chunks.len(),
                actual: self.card_dirty.len(),
            });
        }

        // 2. young_list: no duplicates, all gen_age < threshold,
        //    location matches.
        let mut in_young: std::collections::HashSet<(u32, u16)> =
            std::collections::HashSet::with_capacity(self.young_list.len());
        for &(ci, ei) in &self.young_list {
            if !in_young.insert((ci, ei)) {
                return Err(Violation::DuplicateInYoungList { chunk_idx: ci, entry_idx: ei });
            }
            // SAFETY: presence in young_list implies the slot was
            // initialized at alloc; we just read metadata.
            let entry = unsafe {
                self.chunks[ci as usize][ei as usize].assume_init_ref()
            };
            if entry.gen_age() >= PROMOTION_THRESHOLD {
                return Err(Violation::OldEntryInYoungList {
                    chunk_idx: ci, entry_idx: ei, gen_age: entry.gen_age(),
                });
            }
        }

        // 3. Walk every initialized entry: alive young must be in
        //    young_list; location must match.
        for (ci, chunk) in self.chunks.iter().enumerate() {
            // add-gc-tlab: borrowed chunks are mid-fill; skip (STW validate
            // never runs with a chunk borrowed, but stay defensive).
            if self.borrowed[ci] {
                continue;
            }
            for ei in 0..CHUNK_SIZE {
                if !self.initialized[ci][ei] {
                    continue;
                }
                let entry = unsafe { chunk[ei].assume_init_ref() };
                // Location self-consistency.
                if entry.location != (ci as u32, ei as u16) {
                    return Err(Violation::LocationMismatch {
                        chunk_idx: ci as u32, entry_idx: ei as u16,
                        recorded: entry.location,
                    });
                }
                // Alive young entries must be in young_list.
                if entry.alive.load(Ordering::Acquire)
                    && entry.gen_age() < PROMOTION_THRESHOLD
                    && !in_young.contains(&(ci as u32, ei as u16))
                {
                    return Err(Violation::YoungEntryNotInList {
                        chunk_idx: ci as u32, entry_idx: ei as u16,
                    });
                }
            }
        }

        // 4. free_list slots all alive=false.
        for &(ci, ei) in &self.free_list {
            let entry = unsafe {
                self.chunks[ci as usize][ei as usize].assume_init_ref()
            };
            if entry.alive.load(Ordering::Acquire) {
                return Err(Violation::AliveSlotInFreeList {
                    chunk_idx: ci, entry_idx: ei,
                });
            }
        }

        Ok(())
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
            for ei in 0..CHUNK_SIZE {
                if !self.initialized[ci][ei] {
                    continue;
                }
                // SAFETY: initialized slot. Drop in place.
                unsafe { chunk[ei].assume_init_drop(); }
            }
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
