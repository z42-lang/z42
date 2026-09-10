//! `VarRegion` —— variable-length GC block allocator (unify-gc-heap PR-1).
//!
//! **Why this exists**: the fixed-size `Region<T>` (`region.rs`) can only store one
//! `size_of::<RegionEntry<T>>()`-wide slot per type `T`, so variable-length payloads —
//! string bytes, array element buffers, closure data — currently live *outside* the GC
//! (Arc<str>, Box<ClosureData>, Vec<…>). This module adds the **variable-length block
//! allocator** the unify-gc-heap program needs so those payloads can move into a single
//! managed heap (design direction **A'**, User-confirmed 2026-08-15).
//!
//! # Block model
//!
//! One GC object = a single allocation of a fixed 16-byte [`GcBlockHeader`] immediately
//! followed by its inline variable-length payload (mirrors the `vstr::StrHeader` + inline
//! bytes layout, but the header carries **GC metadata** instead of an Arc refcount):
//!
//! ```text
//!   ┌────────────────────────────┬─────────────────────────────┐
//!   │ GcBlockHeader (16 B, align8)│ inline payload (size bytes)  │
//!   │  generation / size /        │  Str: UTF-8 bytes            │
//!   │  marked / alive /           │  Array<Value>: [Value; n]    │
//!   │  type_tag / size_class      │  Array<prim>: packed bytes   │
//!   └────────────────────────────┴─────────────────────────────┘
//!     ↑ payload starts at DATA_OFFSET = 16 (8-aligned)
//! ```
//!
//! A [`VarGcRef`] handle is a single 8-byte tagged `NonNull<GcBlockHeader>` (low 48 bits =
//! header address, high 16 bits = narrow generation snapshot) — the same path-A tagged
//! pointer used by `GcRef` (`refs.rs`), but **type-erased**: variable-length blocks mix
//! payload types, so the block's `type_tag` (not a static `T`) tells the GC how to scan it.
//!
//! # Allocation model
//!
//! - **Size classes**: total block size (header + payload) is rounded up to the next
//!   quarter-octave step — 32/40/48/56, 64/80/96/112, … (≥ `MIN_BLOCK` = 32).
//!   `free_lists[size_class]` recycles tombstoned slots of the same class, and because each
//!   class carries exactly one footprint, any slot in it fits any payload in it.
//! - **Fast path**: pop a same-size-class tombstoned slot from its free list (generation was
//!   bumped at tombstone → stale `VarGcRef` can't resolve it).
//! - **Slow path**: bump-allocate within the current 64 KB chunk; grow a fresh chunk when
//!   full. Blocks whose total exceeds a chunk get a dedicated exactly-sized chunk.
//!
//! # Sweep model
//!
//! `iterate_alive` walks the stable block list skipping tombstoned entries. Mark/sweep is
//! driven by the heap (`ArcMagrGC`): mark survivors, `sweep` tombstones the unmarked
//! (alive=false + generation bump + push to the size-class free list). **v1 = STW only**
//! (no generational young-list/card-table yet — deferred to a later PR per the 6.5 gate).
//!
//! # PR-1 scope (inert)
//!
//! This module is **not yet wired to any payload**: `Value::Str`/`Closure`/array backings
//! still use their Arc/Box/Vec representations. PR-1 lands the allocator + unit tests
//! (Miri/ASAN-sensitive) only; PR-2…PR-4 migrate the three payload kinds onto it.
//!
//! # Concurrency
//!
//! `VarRegion` holds raw chunk pointers, so it is `!Send` by default; callers wrap it in
//! `parking_lot::Mutex<VarRegion>` exactly like `Region<T>`. The `unsafe impl Send` is sound
//! because every access goes through that mutex and the chunk memory is owned solely by the
//! region (freed only in `Drop`).
//!
//! # Module layout
//!
//! - [`block`] — `BlockType` / `GcBlockHeader` / payload pointer + drop-glue plumbing.
//! - [`chunk`] — size classes, raw chunks, the TLAB claim, and chunk grow/borrow/reclaim.
//! - [`var_ref`] — the [`VarGcRef`] tagged handle.
//! - this file — the `VarRegion` allocator itself (alloc / resolve / tombstone / sweep).

use std::alloc::dealloc;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

mod block;
mod chunk;
mod var_ref;

pub use block::{BlockType, GcBlockHeader, PayloadDropGlue};
pub(crate) use block::payload_ptr_of;
pub(crate) use chunk::{class_for, class_for_with_limit, OVERSIZED_CLASS};
pub use chunk::{loh_bytes, set_loh_bytes};
pub use chunk::{VarChunkClaim, VarChunkReclaim};
pub use var_ref::VarGcRef;

use chunk::{Chunk, NUM_CLASSES};

// The packed `gen_age` in `GcBlockHeader::type_tag` is two bits wide, so the **default**
// promotion age has to fit in it. `Z42_GC_PROMOTION_AGE` is clamped to the same ceiling at
// construction (`gc::promotion_age_from_config`); this guards the compile-time default.
const _: () = assert!(
    crate::gc::region::PROMOTION_THRESHOLD <= block::MAX_GEN_AGE,
    "PROMOTION_THRESHOLD exceeds the gen_age bits packed into GcBlockHeader::type_tag"
);
pub use block::MAX_GEN_AGE;

/// Variable-length GC block allocator. See the module docs for the block / allocation /
/// sweep model.
pub struct VarRegion {
    /// Owned chunk memory. Chunks never move, so header addresses stay stable for
    /// [`VarGcRef`] identity until the region drops.
    chunks: Vec<Chunk>,
    /// Index of the current bump chunk (into `chunks`), or `None` before the first bump
    /// chunk is created. Dedicated oversized chunks are appended but never become the bump
    /// chunk.
    bump_chunk: Option<usize>,
    /// Byte offset of the next bump allocation within `chunks[bump_chunk]`.
    bump_off: usize,
    /// Every distinct block slot ever bump-allocated (stable header pointers). Reused slots
    /// stay here; the list only grows. `iterate_alive` / `sweep` walk it.
    all_blocks: Vec<NonNull<GcBlockHeader>>,
    /// Per-size-class free lists of tombstoned slots available for reuse (LIFO).
    free_lists: Vec<Vec<NonNull<GcBlockHeader>>>,
    /// **fix-minor-gc-skips-var-region (2026-09-08)**: blocks the minor GC must visit —
    /// everything with `gen_age < PROMOTION_THRESHOLD`. Minor scans this instead of
    /// `all_blocks`, which is what makes its cost O(young) rather than O(heap).
    ///
    /// Maintained by **rebuild, not incremental removal**: `alloc` pushes, and
    /// [`Self::sweep_young`] — which has to walk the whole list anyway — writes back only
    /// the entries that are still both alive and young. Tombstone leaves stale entries
    /// behind on purpose; they cost one `is_alive()` check at the next sweep. The
    /// alternative (a `young_idx` per block for O(1) `swap_remove`, as `Region<T>` does) has
    /// nowhere to live: the block header is full. Rebuilding is also strictly cheaper —
    /// #524 had to gate `Region<T>`'s incremental maintenance behind generational mode
    /// because it cost measurable instructions on every alloc.
    ///
    /// Duplicate protection is the header's `IN_YOUNG_BIT`, not a search of this list.
    ///
    /// Maintained **only while [`Self::generational`] is set** — see that field.
    young_list: Vec<NonNull<GcBlockHeader>>,
    /// Whether [`Self::young_list`] is maintained. Minor GC is the list's only consumer and
    /// runs only under `GcMode::GenerationalMarkSweep`, so under any other mode the list is
    /// pure overhead — and not cheap overhead: this region sees ~2.7 M live blocks on a
    /// `z42c.semantics` build, so an unconsumed list costs 20 MB+ of RSS and a push per
    /// alloc. Measured at +48 MB of RSS under `stw-mark-sweep` before this gate existed.
    ///
    /// Same shape and same reason as `Region<T>::generational` (#524) — flipped by
    /// [`Self::set_generational`], which `ArcMagrGC::set_mode` calls alongside the fixed
    /// regions'.
    generational: bool,
    /// Count of live (alive=true) blocks, for diagnostics + auto-collect heuristics.
    live_count: usize,
    /// Optional payload finalizer run once when a block is reclaimed (tombstone) or when the
    /// region drops with the block still alive. `None` = all payloads POD (PR-1). Consumers
    /// storing non-POD payloads (e.g. closure `ClosureData` with an owned `String`) supply it.
    drop_glue: Option<PayloadDropGlue>,

    // ── add-gc-tlab stage 3 (2026-08-29): per-thread chunk-exclusive var alloc ──
    /// Per-chunk "borrowed by a TLAB" flag (parallel to `chunks`). A borrowed chunk is being
    /// lock-free bump-filled by its owning mutator; its blocks are NOT yet in `all_blocks`
    /// (retire appends them), so `iterate_alive`/`sweep` — which walk `all_blocks` — never see
    /// them. The flag only gates [`reclaim_dead_var_chunks`] (skip borrowed) and prevents the
    /// pool from handing out a chunk twice.
    borrowed: Vec<bool>,
    /// Chunk-level free pool (D7): indices of **bump** chunks that became fully dead at a sweep,
    /// available for [`borrow_chunk`] to recycle. Their blocks were purged from `all_blocks` /
    /// `free_lists`; the chunk memory is re-bumped from offset 0 with a bumped `reuse_gen`.
    var_free_chunk_pool: Vec<usize>,
    /// Per-chunk generation base for TLAB-bumped blocks (parallel to `chunks`). Fresh chunks
    /// start at 0. On reclaim, bumped **above every generation any block in the chunk reached**,
    /// so a fresh re-bump can never mint a `(address, generation)` pair that collides with a
    /// stale `VarGcRef` into a prior occupant of the same address — the ABA guard for
    /// variable-size chunk reuse (fixed-slot `Region<T>` preserves per-slot generation instead;
    /// var blocks don't re-align on reuse so a per-chunk base is used).
    reuse_gen: Vec<u32>,
    /// **add-promotion-age-knob (2026-09-08)**: minor GCs a block must survive before
    /// promotion — the region's cached copy of the heap's `promotion_age`.
    promotion_age: u8,
    /// **fix-loh-never-freed (2026-09-08)**: indices of `chunks` slots whose memory was
    /// `dealloc`'d (a dead dedicated/oversized chunk). The slot itself must survive — every
    /// other per-chunk table is addressed by index — so it stays as a `cap == 0` tombstone
    /// and lands here for [`Self::push_chunk`] to reuse.
    free_chunk_slots: Vec<usize>,
    /// **add-incremental-chunk-reclaim (2026-09-10)**: per-chunk census, maintained
    /// incrementally so [`Self::reclaim_dead_var_chunks`] never has to walk `all_blocks`.
    ///
    /// That walk — one binary search per block over 2.7 M blocks — was **45 ms of a 54 ms
    /// minor sweep**: 85% of the sweep and 60% of the whole pause, and `O(heap)` rather than
    /// `O(young)`, so shrinking the nursery could not buy any of it back. With these three
    /// vectors the pass is `O(chunks)`.
    ///
    /// `blocks_per_chunk` counts slots ever carved (a never-used chunk has nothing to
    /// recycle); `live_per_chunk` counts the ones still alive; `max_gen_per_chunk` is the
    /// ABA floor a recycled chunk's `reuse_gen` must clear.
    blocks_per_chunk: Vec<u32>,
    live_per_chunk: Vec<u32>,
    max_gen_per_chunk: Vec<u32>,
}

// SAFETY: all state is reached only through a `Mutex<VarRegion>` (the heap wraps it exactly
// like `Region<T>`), and the raw chunk memory is owned solely by this region (freed in Drop),
// so it is sound to move the region across threads. Matches `Region<T>`'s use behind a mutex.
unsafe impl Send for VarRegion {}

impl Default for VarRegion {
    fn default() -> Self {
        Self {
            chunks: Vec::new(),
            bump_chunk: None,
            bump_off: 0,
            all_blocks: Vec::new(),
            free_lists: (0..NUM_CLASSES).map(|_| Vec::new()).collect(),
            young_list: Vec::new(),
            // Matches `Region<T>`'s default: a bare `VarRegion::new()` (unit tests, mock
            // heaps) maintains the list; the heap narrows it via `set_generational`.
            generational: true,
            live_count: 0,
            drop_glue: None,
            borrowed: Vec::new(),
            var_free_chunk_pool: Vec::new(),
            reuse_gen: Vec::new(),
            promotion_age: crate::gc::region::PROMOTION_THRESHOLD,
            free_chunk_slots: Vec::new(),
            blocks_per_chunk: Vec::new(),
            live_per_chunk: Vec::new(),
            max_gen_per_chunk: Vec::new(),
        }
    }
}

impl VarRegion {
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct a region whose non-POD payloads are finalized by `glue` on reclaim, with
    /// young-list maintenance set for `generational` (see [`Self::generational`]). Used by
    /// the heap, which knows its mode at construction. Mirrors `Region::new_for_mode`.
    pub fn with_drop_glue_for_mode(
        glue: PayloadDropGlue,
        generational: bool,
        promotion_age: u8,
    ) -> Self {
        let mut r = Self::with_drop_glue(glue);
        r.generational = generational;
        r.promotion_age = promotion_age;
        r
    }

    /// Construct a region whose non-POD payloads are finalized by `glue` on reclaim. Used by
    /// the heap for the closure region (`ClosureData` owns a `String` that must be dropped).
    pub fn with_drop_glue(glue: PayloadDropGlue) -> Self {
        // Build explicitly (can't `..Self::default()` — `VarRegion: Drop` forbids moving fields
        // out of the temporary).
        Self {
            chunks: Vec::new(),
            bump_chunk: None,
            bump_off: 0,
            all_blocks: Vec::new(),
            free_lists: (0..NUM_CLASSES).map(|_| Vec::new()).collect(),
            young_list: Vec::new(),
            // Matches `Region<T>`'s default: a bare `VarRegion::new()` (unit tests, mock
            // heaps) maintains the list; the heap narrows it via `set_generational`.
            generational: true,
            live_count: 0,
            drop_glue: Some(glue),
            borrowed: Vec::new(),
            var_free_chunk_pool: Vec::new(),
            reuse_gen: Vec::new(),
            promotion_age: crate::gc::region::PROMOTION_THRESHOLD,
            free_chunk_slots: Vec::new(),
            blocks_per_chunk: Vec::new(),
            live_per_chunk: Vec::new(),
            max_gen_per_chunk: Vec::new(),
        }
    }

    /// Run the injected payload finalizer on `header`'s payload, if any. Called exactly once
    /// per reclaim (tombstone) or at region teardown for still-alive blocks.
    #[inline]
    unsafe fn finalize_payload(&self, header: NonNull<GcBlockHeader>) {
        if let Some(glue) = self.drop_glue {
            // SAFETY: `header` is a live/just-reclaimed block; the glue gets the block type +
            // raw payload pointer (whole-allocation provenance) + payload size, and drops once.
            let (bt, size) = {
                let h = unsafe { header.as_ref() };
                (h.block_type(), h.size())
            };
            let payload = unsafe { payload_ptr_of(header) };
            unsafe { glue(bt, payload, size) };
        }
    }

    /// Allocate a block with `payload` bytes of the given `block_type`. Returns a stable 8-byte
    /// [`VarGcRef`]. The payload is **zero-initialized**; the caller writes into it via
    /// [`VarGcRef::payload_mut`].
    ///
    /// Fast path: reuse a same-size-class tombstoned slot (its generation was bumped at
    /// tombstone, so the returned handle carries the current generation). Slow path: bump.
    pub fn alloc(&mut self, payload: usize, block_type: BlockType) -> VarGcRef {
        let (footprint, size_class) = class_for(payload);

        // Fast path: reuse a tombstoned slot of the same class.
        if size_class != OVERSIZED_CLASS {
            if let Some(slot) = self.free_lists[size_class as usize].pop() {
                return self.reinit_slot(slot, payload, block_type, size_class);
            }
        }


        // Slow path: bump (or a dedicated chunk for oversized).
        let (header_ptr, chunk_idx) = if size_class == OVERSIZED_CLASS {
            self.alloc_dedicated(footprint)
        } else {
            self.bump(footprint)
        };
        self.write_fresh_header(
            header_ptr, payload, block_type, size_class, 0, self.generational, chunk_idx,
        );
        self.all_blocks.push(header_ptr);
        if self.generational {
            self.young_list.push(header_ptr);
        }
        self.live_count += 1;
        self.blocks_per_chunk[chunk_idx as usize] += 1;
        self.live_per_chunk[chunk_idx as usize] += 1;
        VarGcRef::pack(header_ptr, 0)
    }

    /// Re-initialize a recycled slot in place: read the (bumped) generation, drop nothing
    /// (payloads are POD bytes in PR-1), write a fresh header preserving the generation,
    /// zero the payload. Returns the fresh handle.
    fn reinit_slot(
        &mut self,
        slot: NonNull<GcBlockHeader>,
        payload: usize,
        block_type: BlockType,
        size_class: u8,
    ) -> VarGcRef {
        // SAFETY: `slot` came from this region's free list → it points at a valid, chunk-
        // owned, tombstoned header whose generation was bumped at tombstone time.
        let (generation, already_listed, chunk_idx) = {
            let h = unsafe { slot.as_ref() };
            (h.generation(), h.is_in_young(), h.chunk_idx)
        };
        // The recycled block is young again (gen_age 0), but the young list uses lazy
        // deletion: if this slot died *after* the last minor sweep it is still listed, and
        // pushing it a second time would age it twice per minor and grow the list without
        // bound. `already_listed` is the header's own `IN_YOUNG_BIT`, so the check is O(1).
        self.write_fresh_header(
            slot, payload, block_type, size_class, generation, self.generational, chunk_idx,
        );
        if self.generational && !already_listed {
            self.young_list.push(slot);
        }
        self.live_count += 1;
        // A recycled slot never moves, so only the live count changes.
        self.live_per_chunk[chunk_idx as usize] += 1;
        VarGcRef::pack(slot, generation)
    }

    /// Write a fresh header at `ptr` (alive=true, unmarked, given generation) and zero its
    /// payload bytes.
    fn write_fresh_header(
        &self,
        ptr: NonNull<GcBlockHeader>,
        payload: usize,
        block_type: BlockType,
        size_class: u8,
        generation: u32,
        in_young: bool,
        chunk_idx: u32,
    ) {
        // SAFETY: `ptr` addresses freshly-carved (bump) or recycled (free-list) space large
        // enough for the header + `payload` bytes; we own exclusive access (`&mut self`).
        unsafe {
            ptr.as_ptr().write(GcBlockHeader {
                generation: AtomicU32::new(generation),
                size: payload as u32,
                marked: AtomicU8::new(0),
                alive: AtomicBool::new(true),
                type_tag: AtomicU8::new(GcBlockHeader::pack_tag(block_type, 0, in_young)),
                size_class,
                chunk_idx,
            });
            // Zero the payload so a consumer never reads uninitialized bytes. Derive the
            // payload pointer from the raw `ptr` (whole-allocation provenance), not `as_ref()`.
            let data = payload_ptr_of(ptr);
            std::ptr::write_bytes(data, 0, payload);
        }
    }


    /// Resolve a handle to a shared `&GcBlockHeader`, checking the generation guard. Returns
    /// `None` if the handle is stale (its slot was tombstoned + possibly reused).
    ///
    /// # Safety
    /// `handle` must have been produced by *this* region (typestate — enforced by the heap
    /// wrapping exactly one region kind per handle kind).
    pub fn resolve(&self, handle: VarGcRef) -> Option<&GcBlockHeader> {
        // SAFETY: a live handle from this region points at a chunk-owned header whose memory
        // outlives `&self`; the generation check below rejects reused slots.
        let header = unsafe { handle.header_ptr().as_ref() };
        if header.generation() as u16 != handle.gen16() {
            return None;
        }
        if !header.is_alive() {
            return None;
        }
        Some(header)
    }

    /// Tombstone the block behind `handle`: alive=false, bump generation, push its slot to
    /// the size-class free list. No-op (returns `false`) on a stale/already-dead handle.
    pub fn tombstone(&mut self, handle: VarGcRef) -> bool {
        let ptr = handle.header_ptr();
        // SAFETY: handle from this region → valid chunk-owned header.
        let header = unsafe { ptr.as_ref() };
        if header.generation() as u16 != handle.gen16() {
            return false;
        }
        if !header.alive.swap(false, Ordering::Release) {
            return false;
        }
        // Run the payload finalizer (e.g. drop a closure's `String`) exactly once, now that
        // this call won the alive 1→0 race, before the slot can be recycled.
        // SAFETY: the block is freshly reclaimed and still points at its initialized payload.
        unsafe { self.finalize_payload(ptr) };
        let generation = header.generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.live_count -= 1;
        // add-incremental-chunk-reclaim: O(1) — the block carries its own chunk index, so the
        // census stays current without a lookup. `max_gen` is tracked here because tombstone
        // is the only thing that ever raises a block's generation.
        let ci = header.chunk_idx as usize;
        if ci < self.live_per_chunk.len() {
            self.live_per_chunk[ci] -= 1;
            if generation > self.max_gen_per_chunk[ci] {
                self.max_gen_per_chunk[ci] = generation;
            }
        }
        let sc = header.size_class;
        if sc != OVERSIZED_CLASS {
            self.free_lists[sc as usize].push(ptr);
        }
        true
    }

    /// Iterate every currently-alive block, passing its handle + header to `visit`. Skips
    /// tombstoned slots. Order: allocation order.
    pub fn iterate_alive(&self, mut visit: impl FnMut(VarGcRef, &GcBlockHeader)) {
        for &ptr in &self.all_blocks {
            // SAFETY: every pointer in `all_blocks` is a live chunk-owned slot for the
            // region's lifetime (chunks never move / free before Drop).
            let header = unsafe { ptr.as_ref() };
            if !header.is_alive() {
                continue;
            }
            let h = VarGcRef::pack(ptr, header.generation());
            visit(h, header);
        }
    }

    /// **fix-minor-gc-skips-var-region (2026-09-08)**: flip young-list maintenance, bringing
    /// the list in line with the new setting. Mirrors `Region<T>::set_generational` (#524).
    ///
    /// Turning it **on** rebuilds the list from every live young block, so a heap switched to
    /// `GenerationalMarkSweep` after it has already allocated still sees a complete young
    /// set. Turning it **off** drops the list and clears every block's membership bit.
    ///
    /// No-op when already in the requested state.
    pub fn set_generational(&mut self, generational: bool) {
        if self.generational == generational {
            return;
        }
        self.generational = generational;
        if !generational {
            for &ptr in &self.young_list {
                // SAFETY: young_list only holds chunk-owned block pointers.
                unsafe { ptr.as_ref() }.set_in_young(false);
            }
            self.young_list.clear();
            self.young_list.shrink_to_fit();
            return;
        }
        let threshold = self.promotion_age;
        let mut rebuilt = Vec::new();
        for &ptr in &self.all_blocks {
            // SAFETY: see `iterate_alive`.
            let header = unsafe { ptr.as_ref() };
            if header.is_alive() && header.gen_age() < threshold {
                header.set_in_young(true);
                rebuilt.push(ptr);
            }
        }
        self.young_list = rebuilt;
    }

    /// Visit every block currently listed as young, skipping ones already tombstoned (the
    /// young list uses lazy deletion — see the field docs). Read-only; ordering is
    /// allocation order within the list.
    pub fn iterate_young(&self, mut visit: impl FnMut(VarGcRef, &GcBlockHeader)) {
        for &ptr in &self.young_list {
            // SAFETY: young_list only ever holds chunk-owned block pointers, and chunks
            // outlive the region; reclaimed chunks purge their blocks from this list.
            let header = unsafe { ptr.as_ref() };
            if !header.is_alive() {
                continue;
            }
            visit(VarGcRef::pack(ptr, header.generation()), header);
        }
    }

    /// How many blocks the next minor GC would visit. Includes stale (tombstoned) entries
    /// that the next sweep will drop, so it is an upper bound on real young blocks — the
    /// same shape as `Region<T>::young_count`, and it is only used as the denominator of the
    /// minor-survival heuristic.
    pub fn young_count(&self) -> usize {
        self.young_list.len()
    }

    /// **fix-minor-gc-skips-var-region (2026-09-08)**: minor sweep. Walks only `young_list`
    /// (not `all_blocks`), which is what bounds minor cost at O(young):
    ///
    /// - marked → clear the mark and age it; on reaching `PROMOTION_THRESHOLD` it leaves the
    ///   young list (promotion is a label change — var blocks never move, see
    ///   `VarGcRef`'s address-as-identity contract);
    /// - unmarked → finalize + tombstone, crediting the bytes it was charged at alloc;
    /// - already tombstoned → a stale entry from lazy deletion; just drop it.
    ///
    /// The list is **rebuilt** from the survivors rather than element-wise mutated, and the
    /// header's `IN_YOUNG_BIT` is cleared for everything that leaves, so a recycled slot
    /// knows to re-list itself.
    ///
    /// Old blocks are never visited — that is the definition of a minor collection. They are
    /// reachable as minor roots only through the dirty-card set.
    pub fn sweep_young(&mut self) -> (usize, u64) {
        let threshold = self.promotion_age;
        let mut reclaimed = 0usize;
        let mut credited: u64 = 0;
        // Tombstoning mutates `free_lists` / `live_count`, so it cannot run while
        // `young_list` is borrowed — collect first, then apply (mirrors `sweep`).
        let mut to_reclaim: Vec<(VarGcRef, u64)> = Vec::new();
        let mut survivors: Vec<NonNull<GcBlockHeader>> = Vec::with_capacity(self.young_list.len());

        for &ptr in &self.young_list {
            // SAFETY: see `iterate_young`.
            let header = unsafe { ptr.as_ref() };
            if !header.is_alive() {
                header.set_in_young(false);
                continue;
            }
            if header.is_marked() {
                header.clear_mark();
                if header.bump_gen_age() >= threshold {
                    header.set_in_young(false);
                } else {
                    survivors.push(ptr);
                }
            } else {
                header.set_in_young(false);
                let charge = Self::alloc_charge_bytes(header);
                to_reclaim.push((VarGcRef::pack(ptr, header.generation()), charge));
            }
        }
        self.young_list = survivors;

        for (h, charge) in to_reclaim {
            if self.tombstone(h) {
                reclaimed += 1;
                credited += charge;
            }
        }
        (reclaimed, credited)
    }

    /// STW sweep: tombstone every unmarked live block, clear the mark on survivors. Returns
    /// `(blocks reclaimed, bytes credited)`. (v1 = STW only; generational minor sweep is a
    /// later PR.)
    ///
    /// **fix-var-sweep-accounting**: the byte figure must mirror exactly what `used_bytes`
    /// was *charged* at alloc, or the auto-collect budget reads a number that drifts from
    /// the heap. See [`Self::alloc_charge_bytes`] for the per-`BlockType` rule.
    pub fn sweep(&mut self) -> (usize, u64) {
        let mut reclaimed = 0;
        let mut credited: u64 = 0;
        // Collect the slots to reclaim first (can't tombstone while borrowing all_blocks).
        // The charge is read here, while the header is still readable (tombstone bumps the
        // generation and may hand the slot straight back to a free list).
        let mut to_reclaim: Vec<(VarGcRef, u64)> = Vec::new();
        for &ptr in &self.all_blocks {
            // SAFETY: see `iterate_alive`.
            let header = unsafe { ptr.as_ref() };
            if !header.is_alive() {
                continue;
            }
            if header.is_marked() {
                header.clear_mark();
            } else {
                let charge = Self::alloc_charge_bytes(header);
                to_reclaim.push((VarGcRef::pack(ptr, header.generation()), charge));
            }
        }
        for (h, charge) in to_reclaim {
            if self.tombstone(h) {
                reclaimed += 1;
                credited += charge;
            }
        }
        (reclaimed, credited)
    }

    /// The number of `used_bytes` a block of this kind added when it was allocated — the
    /// only figure sweep may credit back.
    ///
    /// - `Str` / `Closure` — `alloc_str_in_region` / `alloc_closure_in_region` charge
    ///   `DATA_OFFSET + payload`, so that is what comes back.
    /// - `ArrayValue` / `ArrayPrim` / `ArrayStruct` — **zero**. `alloc_var_block` records
    ///   no stats at all (by design); the owning array header charged the element storage
    ///   through `object_size_bytes`, and `array_size_estimate` credits it back when the
    ///   header is tombstoned. Crediting the block here as well would refund it twice.
    #[inline]
    fn alloc_charge_bytes(header: &GcBlockHeader) -> u64 {
        match header.block_type() {
            BlockType::Str | BlockType::Closure => {
                (GcBlockHeader::DATA_OFFSET + header.size()) as u64
            }
            BlockType::ArrayValue | BlockType::ArrayPrim | BlockType::ArrayStruct => 0,
        }
    }

    /// Number of live blocks (diagnostics).
    #[inline]
    pub fn live_count(&self) -> usize {
        self.live_count
    }

    /// Count of chunks that currently own memory (tests / diagnostics). Slots tombstoned by
    /// `Chunk::free_in_place` are excluded — they are bookkeeping, not footprint.
    #[cfg(test)]
    pub(crate) fn chunk_count(&self) -> usize {
        self.chunks.iter().filter(|c| !c.is_freed()).count()
    }

    /// Number of `chunks` slots including freed tombstones (tests: proves slot reuse).
    #[cfg(test)]
    pub(crate) fn chunk_slot_count(&self) -> usize {
        self.chunks.len()
    }

    /// The per-chunk census (tests: reconciled against a full scan of `all_blocks`).
    #[cfg(test)]
    pub(crate) fn live_per_chunk_for_test(&self) -> Vec<u32> {
        self.live_per_chunk.clone()
    }

    /// See [`Self::live_per_chunk_for_test`].
    #[cfg(test)]
    pub(crate) fn blocks_per_chunk_for_test(&self) -> Vec<u32> {
        self.blocks_per_chunk.clone()
    }

    /// **add-gc-tlab stage 3**: reclaimed-chunk pool size (tests).
    #[cfg(test)]
    pub(crate) fn free_chunk_pool_len(&self) -> usize {
        self.var_free_chunk_pool.len()
    }
}

impl Drop for VarRegion {
    /// Finalize every still-alive block's payload (if a drop glue was injected), then free
    /// every owned chunk. Reclaimed (tombstoned) blocks were already finalized at tombstone.
    fn drop(&mut self) {
        if self.drop_glue.is_some() {
            for &ptr in &self.all_blocks {
                // SAFETY: chunk-owned header valid until the dealloc below.
                let alive = unsafe { ptr.as_ref() }.is_alive();
                if alive {
                    // SAFETY: alive block still owns its initialized payload; finalize once.
                    unsafe { self.finalize_payload(ptr) };
                }
            }
        }
        for chunk in &self.chunks {
            // fix-loh-never-freed: a tombstoned slot's memory is already back with the
            // allocator (`Chunk::free_in_place`) — freeing it again would be a double free.
            if chunk.is_freed() {
                continue;
            }
            // SAFETY: each chunk was allocated with `chunk.layout()`; freed exactly once here.
            unsafe { dealloc(chunk.base.as_ptr(), chunk.layout()) }
        }
    }
}

#[cfg(test)]
#[path = "var_region_tests.rs"]
mod var_region_tests;
