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
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

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

    /// One-shot finalizer slot. `Mutex<Option<FinalizerFn>>` so the
    /// sweep path can `take()` the closure atomically (fire-once
    /// semantics — matches add-mark-sweep-collector behavior).
    pub(crate) finalizer: Mutex<Option<FinalizerFn>>,

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

    fn new(value: T, location: (u32, u16)) -> Self {
        Self {
            value:          Mutex::new(value),
            marked:         AtomicU8::new(0),
            alive:          AtomicBool::new(true),
            gen_age:        AtomicU8::new(0),
            generation:     AtomicU32::new(0),
            finalizer:      Mutex::new(None),
            location,
            soft_ref_count: AtomicU32::new(0),
        }
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

/// Chunked region allocator. Owns user objects of type `T` plus
/// per-object GC metadata. See module-level docs for the allocation
/// + sweep model.
pub struct Region<T> {
    /// Chunks of pre-reserved entries. Each chunk is a fixed-size
    /// `Box<[MaybeUninit<RegionEntry<T>>; CHUNK_SIZE]>` so its
    /// address is stable for the chunk's lifetime.
    chunks: Vec<Box<[MaybeUninit<RegionEntry<T>>; CHUNK_SIZE]>>,

    /// (chunk_idx, entry_idx) — next bump-pointer position. Advances
    /// linearly; never goes back. Free list pops are separate.
    next_bump: (u32, u16),

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

    _phantom: PhantomData<T>,
}

impl<T> Default for Region<T> {
    fn default() -> Self {
        Self {
            chunks:      Vec::new(),
            next_bump:   (0, 0),
            free_list:   Vec::new(),
            initialized: Vec::new(),
            young_list:  Vec::new(),
            card_dirty:  Vec::new(),
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

        // Bump pointer.
        let (ci, ei) = self.next_bump;
        if (ci as usize) >= self.chunks.len() {
            // Grow: push a new chunk of MaybeUninit entries.
            // SAFETY: MaybeUninit<RegionEntry<T>> is valid to leave uninit.
            let chunk: Box<[MaybeUninit<RegionEntry<T>>; CHUNK_SIZE]> = Box::new(unsafe {
                MaybeUninit::<[MaybeUninit<RegionEntry<T>>; CHUNK_SIZE]>::uninit().assume_init()
            });
            self.chunks.push(chunk);
            self.initialized.push(vec![false; CHUNK_SIZE]);
            // add-generational-gc P0: grow card_dirty bitmap (one u32/chunk).
            self.card_dirty.push(0);
        }
        let chunk = &mut self.chunks[ci as usize];
        chunk[ei as usize] = MaybeUninit::new(RegionEntry::new(value, (ci, ei)));
        self.initialized[ci as usize][ei as usize] = true;
        // add-generational-gc P0: track newly-allocated entry as young.
        self.young_list.push((ci, ei));

        // Advance next_bump.
        let next_ei = ei + 1;
        if (next_ei as usize) >= CHUNK_SIZE {
            self.next_bump = (ci + 1, 0);
        } else {
            self.next_bump = (ci, next_ei);
        }

        RegionHandle { chunk_idx: ci, entry_idx: ei, generation: 0 }
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
        let bump_remaining = if (self.next_bump.0 as usize) >= self.chunks.len() {
            0
        } else {
            CHUNK_SIZE - self.next_bump.1 as usize
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
