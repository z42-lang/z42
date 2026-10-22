//! `VarRegion` —— variable-length GC block allocator (unify-gc-heap PR-1).
//!
//! The heap's only region: every managed value — object, array, string, closure — is one
//! variable-length block (a 16 B header, then the payload), and this is their allocator.
//!
//! # Block model
//!
//! One GC object = a single allocation of a fixed 16-byte [`GcBlockHeader`] immediately
//! followed by its inline variable-length payload:
//!
//! ```text
//!   ┌──────────────────────────────────────────┬────────────────────────────────┐
//!   │ GcBlockHeader (16 B, align 8)            │ inline payload (size bytes)     │
//!   │  GcWord: generation(+alive) / marked /   │  Object: field cells [+ meta]   │
//!   │          type_tag / size                 │  Array: elements (per mode)     │
//!   │  kind word: object → Arc<TypeDesc>,      │  Str: UTF-8 bytes               │
//!   │             array → &ArrayDesc           │  Closure: ClosureData           │
//!   └──────────────────────────────────────────┴────────────────────────────────┘
//!     ↑ payload starts at DATA_OFFSET = 16 (8-aligned)
//! ```
//!
//! Object model R8: the header records the payload length but not the size class (derived from
//! it, `chunk::payload_class`) nor the owning chunk (sweeps know it; `chunk_of` finds it from an
//! address), and the alive flag is the low bit of the generation (`block::ALIVE_GEN_BIT`) — that
//! is what leaves the kind word free for an object's type descriptor.
//!
//! A [`VarGcRef`] handle is a single 8-byte tagged `NonNull<GcBlockHeader>` (low 48 bits =
//! header address, high 16 bits = narrow generation snapshot) — the same path-A tagged
//! pointer used by `ArrRef` (`refs.rs`), but **type-erased**: variable-length blocks mix
//! payload types, so the block's `type_tag` (not a static `T`) tells the GC how to scan it.
//!
//! # Allocation model
//!
//! - **Size classes**: total block size (header + payload) is rounded up to the next
//!   quarter-octave step — 16/24, 32/40/48/56, 64/80/96/112, … (≥ `MIN_BLOCK` = 16).
//!   `free_lists[size_class]` recycles tombstoned slots of the same class, and because each
//!   class carries exactly one footprint, any slot in it fits any payload in it.
//! - **Fast path**: pop a same-size-class tombstoned slot from its free list (generation was
//!   bumped at tombstone → stale `VarGcRef` can't resolve it).
//! - **Slow path**: bump-allocate within the current 64 KB chunk; grow a fresh chunk when
//!   full. Blocks whose total exceeds a chunk get a dedicated exactly-sized chunk.
//! - **TLAB** (production allocation, `arc_heap/alloc.rs`): the same free lists, taken a batch
//!   per size class at a time and filled lock-free ([`holes`], M12), before the thread's bump
//!   claim on a whole fresh or pooled chunk ([`chunk::VarChunkClaim`]).
//!
//! # Sweep model
//!
//! `iterate_alive` walks each chunk's carved prefix `[0, hwm)` header by header (every header
//! names its size class, hence the next block's offset), skipping tombstoned blocks. Mark/sweep
//! is driven by the heap (`ArcMagrGC`): mark survivors, `sweep` tombstones the unmarked
//! (alive=false + generation bump + push to the size-class free list). The young set is a
//! per-chunk bitmap of block starts (`generation.rs`).
//!
//! # Concurrency
//!
//! `VarRegion` holds raw chunk pointers, so it is `!Send` by default; callers wrap it in
//! `parking_lot::Mutex<VarRegion>`. The `unsafe impl Send` is sound
//! because every access goes through that mutex and the chunk memory is owned solely by the
//! region (freed only in `Drop`).
//!
//! # Module layout
//!
//! - [`block`] — `BlockType` / `GcBlockHeader` / payload pointer + drop-glue plumbing.
//! - [`chunk`] — size classes, raw chunks, the TLAB claim, and chunk grow/borrow/reclaim.
//! - [`holes`] — the TLAB's batched reuse of dead slots in partially-live chunks (M12).
//! - [`var_ref`] — the [`VarGcRef`] tagged handle.
//! - this file — the `VarRegion` allocator itself (alloc / resolve / tombstone / sweep).

use std::alloc::dealloc;
use std::collections::BTreeMap;
use std::ptr::NonNull;
use std::sync::atomic::Ordering;

mod block;
mod chunk;
mod generation;
mod holes;
mod objects;
mod var_ref;

pub use block::{drop_type_bit, BlockType, GcBlockHeader, GcWord, PayloadDropGlue};
pub(crate) use block::KIND_WORD_OFFSET;
pub use objects::{DetachedBlock, VarMinorSweep};
pub(crate) use block::payload_ptr_of;
#[cfg(feature = "jit")]
pub(crate) use block::{ASCII_STR_BIT, HEADER_SIZE_OFFSET, HEADER_TYPE_TAG_OFFSET};
#[cfg(all(feature = "jit", target_pointer_width = "64"))]
pub(crate) use var_ref::HANDLE_ADDR_MASK;
pub(crate) use chunk::{class_for, class_for_with_limit, OVERSIZED_CLASS};
use block::{ALIVE_GEN_BIT, FIRST_GEN};
use chunk::payload_class;
pub use chunk::{loh_bytes, set_loh_bytes};
pub use chunk::{VarChunkClaim, VarChunkReclaim};
pub use holes::VarHoleStash;
pub(crate) use holes::prefetch_write;
pub use var_ref::VarGcRef;

use chunk::{Chunk, FreeEntry, FreeSlot, NUM_CLASSES};

// The packed `gen_age` in `GcBlockHeader::type_tag` is two bits wide, so the **default**
// promotion age has to fit in it. `Z42_GC_PROMOTION_AGE` is clamped to the same ceiling at
// construction (`gc::promotion_age_from_config`); this guards the compile-time default.
const _: () = assert!(
    crate::gc::PROMOTION_THRESHOLD <= block::MAX_GEN_AGE,
    "PROMOTION_THRESHOLD exceeds the gen_age bits packed into GcBlockHeader::type_tag"
);
pub use block::MAX_GEN_AGE;

/// Variable-length GC block allocator. See the module docs for the block / allocation /
/// sweep model.
pub struct VarRegion {
    /// Owned chunk memory. Chunks never move, so header addresses stay stable for
    /// [`VarGcRef`] identity until the region drops.
    chunks: Vec<Chunk>,
    /// Chunk base address → index, for every chunk that owns memory — what
    /// [`Self::chunk_of`] searches (object model R8: a block header no longer names its chunk).
    chunk_index: BTreeMap<usize, u32>,
    /// Index of the current bump chunk (into `chunks`), or `None` before the first bump
    /// chunk is created. Dedicated oversized chunks are appended but never become the bump
    /// chunk.
    bump_chunk: Option<usize>,
    /// Byte offset of the next bump allocation within `chunks[bump_chunk]`.
    bump_off: usize,
    /// Per chunk, how many bytes from its base are carved into blocks (the bump high-water
    /// mark; `0` for a pooled, freed or still-borrowed chunk). Blocks are carved back to back
    /// and a reused slot keeps its size class, so `[0, hwm)` is a run of headers each followed
    /// by its class footprint — [`Self::for_each_block_in`] walks it. This is the region's whole
    /// index of its blocks (M8): four bytes per chunk instead of a pointer per block.
    hwm: Vec<u32>,
    /// Per-size-class free lists of tombstoned slots available for reuse (LIFO).
    ///
    /// **lazy-var-free-list **: entries are **not** removed when their chunk is
    /// pooled. They used to be — `purge_blocks` ran a `retain` over every class, dereferencing
    /// one header per entry to read its `chunk_idx`, and that scan was **142.5 ms of a 513 ms
    /// total pause** on `z42c.semantics` (45 minors × up to 1 235 912 entries). It is
    /// `O(heap)` work to evict `O(chunks reclaimed)` entries, and shrinking the nursery in
    /// retune-gc-nursery-and-promotion-age quadrupled how often it ran.
    ///
    /// Dropping the scan is safe because **free lists can never hold a dangling pointer**: the
    /// only chunks whose memory is actually handed back are dedicated (oversized) ones, and
    /// [`Self::tombstone`] never pushes an `OVERSIZED_CLASS` block. Everything in here points
    /// into a bump chunk, which is *pooled* — its memory stays mapped.
    ///
    /// What is **not** safe is handing out a stale slot: a pooled chunk is re-bumped from
    /// offset 0, so an entry that outlived the pooling would alias whatever lands there next.
    /// [`Self::pool_epoch`] is the guard — see the pop loop in [`Self::alloc`].
    ///
    /// **perf-free-slot-encoding **: an entry is a four-byte [`FreeSlot`], not the
    /// slot's header pointer. The staleness test needs the entry's **chunk**, and reading that
    /// out of the block header was a random memory access per entry — 13.2 ms of the build's
    /// GC pause in [`Self::compact_class`] alone. See [`FreeSlot`].
    /// **perf-merge-free-entry **: the entry carries its chunk's `pool_epoch`
    /// alongside the slot ([`FreeEntry`]) rather than living in a second, parallel `Vec` —
    /// one push per tombstone instead of two, and one stream to walk in
    /// [`Self::compact_class`]. See [`FreeEntry`] for why splitting them stopped paying.
    free_lists: Vec<Vec<FreeEntry>>,
    /// **lazy-var-free-list **: how many entries across [`Self::free_lists`] are
    /// known-stale. Exact, not an estimate: a chunk only gets pooled when **every** block in
    /// it is tombstoned, and every non-oversized tombstone pushes a free-list entry, so
    /// pooling chunk `ci` staled exactly as many entries as it had blocks.
    ///
    /// Bounds the memory the lazy scheme can waste. Without it a cold size class would hold
    /// its stale entries forever — nothing pops them, so nothing discovers them.
    stale_free: usize,
    /// **lazy-var-free-list **: the same count, split per size class, so a
    /// compaction can walk **one** list instead of all [`NUM_CLASSES`] of them. Compacting
    /// globally cost 17–25 ms a go, which forced the threshold so loose that stale entries
    /// piled up into ~40 MB of RSS; per class it is cheap enough to run tight.
    stale_per_class: Vec<usize>,
    /// The young set (M8): one bit per 8-byte granule of a chunk, set at a block's start while
    /// the block is alive and younger than [`Self::promotion_age`] — `young_bits[ci]` covers
    /// chunk `ci` (128 words for a bump chunk, a few for a dedicated one, none while the region
    /// is not generational). Set at alloc / retire, cleared on promotion and on tombstone, so it
    /// is exact at every point. A minor walks only these bits, which is what makes its cost
    /// `O(young)` rather than `O(heap)`.
    young_bits: Vec<Box<[u64]>>,
    /// Per chunk, set bits in `young_bits[ci]` — so taking a chunk's last young block out also
    /// clears its summary bit in `O(1)`.
    young_per_chunk: Vec<u32>,
    /// One bit per chunk: whether it holds any young block. A minor visits only these chunks.
    young_chunks: Vec<u64>,
    /// Set bits across `young_bits` ([`Self::young_count`]).
    young_len: usize,
    /// Whether the young set is maintained. Minor GC is its only consumer and runs only under
    /// `GcMode::GenerationalMarkSweep`, so under any other mode the bitmaps are not allocated
    /// and alloc / tombstone skip them.
    ///
    /// Flipped by [`Self::set_generational`], which `ArcMagrGC::set_mode` calls.
    generational: bool,
    /// Count of live (alive=true) blocks, for diagnostics + auto-collect heuristics.
    live_count: usize,
    /// Debug builds: how many hole stashes hold each slot address (taken by `take_holes`, not yet
    /// handed back by `retire_holes`) — what the young-set walks check a dead entry against
    /// (`generation::young_entry_is_block`). A count, not a set: a slot filled from one stash can be
    /// reclaimed by a collection that runs outside the safepoint protocol and taken by a second
    /// stash before the first one retires.
    #[cfg(debug_assertions)]
    stashed: std::collections::HashMap<usize, u32>,
    /// Optional payload finalizer run once when a block is reclaimed (tombstone) or when the
    /// region drops with the block still alive. `None` = all payloads POD (PR-1). Consumers
    /// storing non-POD payloads (e.g. closure `ClosureData` with an owned `String`) supply it.
    drop_glue: Option<PayloadDropGlue>,
    /// Which block types `drop_glue` runs for (`block::drop_type_bit` mask). Checked before the
    /// indirect call, so a reclaimed POD block (a string, a packed array) costs one branch.
    drop_types: u8,
    /// Object model R6: addresses of blocks tombstoned while carrying `SIDE_BIT` /
    /// `REMEMBERED_BIT` — the heap drops its side-table entries for them before any chunk is
    /// pooled or freed (`take_flagged_dead`).
    flagged_dead: Vec<usize>,

    // ── add-gc-tlab stage 3 : per-thread chunk-exclusive var alloc ──
    /// Per-chunk "borrowed by a TLAB" flag (parallel to `chunks`). A borrowed chunk is being
    /// lock-free bump-filled by its owning mutator; its high-water mark stays `0` until retire
    /// publishes it, so `iterate_alive`/`sweep` — which walk `[0, hwm)` — never see its blocks.
    /// The flag only gates [`reclaim_dead_var_chunks`] (skip borrowed) and prevents the pool
    /// from handing out a chunk twice.
    borrowed: Vec<bool>,
    /// Chunk-level free pool (D7): indices of **bump** chunks that became fully dead at a sweep,
    /// available for [`borrow_chunk`] to recycle. Their high-water mark is reset (their blocks
    /// stop existing to every walk); the chunk memory is re-bumped from offset 0 with a bumped
    /// `reuse_gen`.
    var_free_chunk_pool: Vec<usize>,
    /// Per-chunk generation base (odd) for the blocks carved in the chunk's current epoch
    /// (parallel to `chunks`). Fresh chunks start at `FIRST_GEN`. On reclaim, moved **right past
    /// every generation any block in the chunk reached** (`reuse_gen + max_span + 1`, mod 2^16), so
    /// a fresh re-bump can never mint a `(address, generation)` pair that collides with a stale
    /// `VarGcRef` minted in the epoch before — the ABA guard for variable-size chunk reuse
    /// (blocks don't re-align on reuse, so a per-chunk base is used).
    reuse_gen: Vec<u16>,
    /// **lazy-var-free-list **: how many times chunk `ci` has been pooled. Stamped
    /// onto every [`Self::free_lists`] entry at push and re-checked at pop; that is the whole
    /// staleness test.
    ///
    /// Deliberately **not** reusing [`Self::reuse_gen`], which is derived from block
    /// generations (`reuse_gen + max_span + 1`, mod 2^16) for the `VarGcRef` ABA guard and
    /// wraps. Whether that derivation separates every pooling is a property of a *different*
    /// invariant; hanging slot-recycling correctness off it would couple the two. A plain
    /// counter is four bytes per chunk and cannot be wrong.
    pool_epoch: Vec<u32>,
    /// **add-promotion-age-knob **: minor GCs a block must survive before
    /// promotion — the region's cached copy of the heap's `promotion_age`.
    promotion_age: u8,
    /// **fix-loh-never-freed **: indices of `chunks` slots whose memory was
    /// `dealloc`'d (a dead dedicated/oversized chunk). The slot itself must survive — every
    /// other per-chunk table is addressed by index — so it stays as a `cap == 0` tombstone
    /// and lands here for [`Self::push_chunk`] to reuse.
    free_chunk_slots: Vec<usize>,
    /// **add-incremental-chunk-reclaim **: per-chunk census, maintained
    /// incrementally so [`Self::reclaim_dead_var_chunks`] never has to walk the blocks.
    ///
    /// That walk — one binary search per block over 2.7 M blocks — was **45 ms of a 54 ms
    /// minor sweep**: 85% of the sweep and 60% of the whole pause, and `O(heap)` rather than
    /// `O(young)`, so shrinking the nursery could not buy any of it back. With these three
    /// vectors the pass is `O(chunks)`.
    ///
    /// `blocks_per_chunk` counts slots ever carved (a never-used chunk has nothing to
    /// recycle); `live_per_chunk` counts the ones still alive; `max_span_per_chunk` is how far
    /// past `reuse_gen` (mod 2^16) any tombstone in the current epoch took a generation — the
    /// ABA floor a recycled chunk's next `reuse_gen` must clear.
    blocks_per_chunk: Vec<u32>,
    live_per_chunk: Vec<u32>,
    max_span_per_chunk: Vec<u16>,
    /// The owning heap's footprint (`gc::footprint`) and the last side-table reading charged to
    /// it — see `chunk::VarRegion::refresh_side_tables`.
    footprint: std::sync::Arc<crate::gc::footprint::Footprint>,
    side_accounted: u64,
    /// How many chunks at the bottom of `var_free_chunk_pool` have their pages decommitted
    /// (`chunk::VarRegion::decommit_pool`); `borrow_chunk` pops from the top.
    pool_decommitted: usize,
}

// SAFETY: all state is reached only through a `Mutex<VarRegion>` (the heap's), and the raw chunk
// memory is owned solely by this region (freed in Drop), so it is sound to move the region
// across threads.
unsafe impl Send for VarRegion {}

impl Default for VarRegion {
    fn default() -> Self {
        Self {
            chunks: Vec::new(),
            chunk_index: BTreeMap::new(),
            bump_chunk: None,
            bump_off: 0,
            hwm: Vec::new(),
            free_lists: (0..NUM_CLASSES).map(|_| Vec::new()).collect(),
            stale_free: 0,
            stale_per_class: vec![0; NUM_CLASSES],
            young_bits: Vec::new(),
            young_per_chunk: Vec::new(),
            young_chunks: Vec::new(),
            young_len: 0,
            // A bare `VarRegion::new()` (unit tests) maintains the young set; the heap narrows it
            // via `set_generational`.
            generational: true,
            live_count: 0,
            #[cfg(debug_assertions)]
            stashed: std::collections::HashMap::new(),
            drop_glue: None,
            drop_types: 0,
            flagged_dead: Vec::new(),
            borrowed: Vec::new(),
            var_free_chunk_pool: Vec::new(),
            reuse_gen: Vec::new(),
            pool_epoch: Vec::new(),
            promotion_age: crate::gc::PROMOTION_THRESHOLD,
            free_chunk_slots: Vec::new(),
            blocks_per_chunk: Vec::new(),
            live_per_chunk: Vec::new(),
            max_span_per_chunk: Vec::new(),
            footprint: Default::default(),
            side_accounted: 0,
            pool_decommitted: 0,
        }
    }
}

impl VarRegion {
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct a region whose non-POD payloads are finalized by `glue` on reclaim, with
    /// young-list maintenance set for `generational` (see [`Self::generational`]). Used by
    /// the heap, which knows its mode at construction.
    ///
    /// **perf-skip-pod-drop-glue **: `glue` is optional because an embedding whose
    /// payloads are all POD has nothing for it to do, and a `None` here is what removes the
    /// indirect call from the reclaim path — see [`Self::finalize_payload`].
    pub fn with_drop_glue_for_mode(
        glue: Option<(PayloadDropGlue, u8)>,
        generational: bool,
        promotion_age: u8,
    ) -> Self {
        let mut r = match glue {
            Some((glue, types)) => Self::with_drop_glue(glue, types),
            None => Self::new(),
        };
        r.generational = generational;
        r.promotion_age = promotion_age;
        r
    }

    /// Construct a region whose non-POD payloads — blocks of the types in the `types` mask
    /// (`block::drop_type_bit`) — are finalized by `glue` on reclaim.
    pub fn with_drop_glue(glue: PayloadDropGlue, types: u8) -> Self {
        let mut r = Self::default();
        r.drop_glue = Some(glue);
        r.drop_types = types;
        r
    }

    /// Run the injected payload finalizer on `header`'s payload, if any. Called exactly once
    /// per reclaim (tombstone) or at region teardown for still-alive blocks.
    ///
    /// **perf-skip-pod-drop-glue **: when no payload needs dropping the region is
    /// built with `drop_glue: None` and this is one predictable branch. It used to read the
    /// block's type and size and make an **indirect call** per reclaimed block into a function
    /// that then did nothing — `1.6 ms` of `minor/var sweep` over 2.42 M dead blocks a
    /// `z42c.semantics` build. A fn pointer is opaque to the optimizer, so the emptiness of
    /// the callee could not remove the call; only not installing it can.
    #[inline]
    unsafe fn finalize_payload(&self, header: NonNull<GcBlockHeader>) {
        if let Some(glue) = self.drop_glue {
            // SAFETY: `header` is a live/just-reclaimed block; the glue gets the block type +
            // raw payload pointer (whole-allocation provenance) + payload size, and drops once.
            let (bt, size) = {
                let h = unsafe { header.as_ref() };
                (h.block_type(), h.size())
            };
            if self.drop_types & block::drop_type_bit(bt) == 0 {
                return;
            }
            let freed = unsafe { glue(bt, header, size) };
            if freed != 0 {
                self.footprint.credit(freed);
            }
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
        //
        // **lazy-var-free-list **: a loop rather than one `pop`, because entries
        // are no longer evicted when their chunk is pooled (see [`Self::free_lists`]). An
        // entry whose stamp no longer matches its chunk's `pool_epoch` is discarded here —
        // that is the one thing standing between this list and handing out a slot inside a
        // pooled or re-bumped chunk.
        //
        // M12: not while the flagged-dead list is non-empty — the heap still has side-table
        // entries keyed by those addresses (same rule as `holes::take_holes`).
        if size_class != OVERSIZED_CLASS && self.flagged_dead.is_empty() {
            if let Some((slot, ci)) = self.pop_free_slot(size_class) {
                return self.reinit_slot(slot, ci, payload, block_type);
            }
        }


        // Slow path: bump (or a dedicated chunk for oversized).
        let (header_ptr, chunk_idx) = if size_class == OVERSIZED_CLASS {
            self.alloc_dedicated(footprint)
        } else {
            self.bump(footprint)
        };
        // A bump or dedicated chunk is always fresh (pooled chunks only go to TLABs), so its
        // epoch's generation base is `FIRST_GEN`.
        debug_assert_eq!(self.reuse_gen[chunk_idx as usize], FIRST_GEN);
        self.write_fresh_header(header_ptr, payload, block_type, FIRST_GEN);
        self.add_young(header_ptr, chunk_idx as usize);
        self.live_count += 1;
        self.blocks_per_chunk[chunk_idx as usize] += 1;
        self.live_per_chunk[chunk_idx as usize] += 1;
        VarGcRef::pack(header_ptr, FIRST_GEN)
    }

    /// **lazy-var-free-list **: pop the next *usable* slot of `size_class`,
    /// discarding entries whose chunk has been pooled since they were pushed.
    ///
    /// Returns `None` when the class is exhausted — which now means "exhausted of live
    /// entries", not "empty". Each discarded entry is `O(1)` and happens at most once per
    /// entry ever pushed, so the amortised cost of the whole scheme is one extra compare per
    /// allocation that reuses a slot.
    /// Also returns the slot's chunk index (from the entry — no header read: M12's batch take
    /// pops dozens at a time, and the header would be a cache miss each).
    pub(super) fn pop_free_slot(&mut self, size_class: u8) -> Option<(NonNull<GcBlockHeader>, usize)> {
        let sc = size_class as usize;
        loop {
            let FreeEntry { slot, epoch } = self.free_lists[sc].pop()?;
            // perf-free-slot-encoding: the chunk comes out of the entry, so the staleness test
            // touches no block memory at all — and the pointer is only rebuilt once the entry
            // has passed it.
            let ci = slot.chunk();
            if self.pool_epoch.get(ci).copied() == Some(epoch) {
                // SAFETY: the entry named a live bump chunk when it was pushed, and a matching
                // `pool_epoch` says that chunk has not been pooled since. Bump chunks are
                // pooled, never `dealloc`'d (only dedicated/oversized chunks are freed, and
                // `tombstone` never pushes those), so `base` is mapped and `base + offset` is
                // the slot's own header, in bounds and 8-aligned.
                let raw = unsafe { self.chunks[ci].base.as_ptr().add(slot.offset()) };
                return Some((unsafe { NonNull::new_unchecked(raw as *mut GcBlockHeader) }, ci));
            }
            // Stale: the chunk was pooled after this entry was pushed, so the slot either
            // belongs to the pool or has already been re-bumped into something live.
            self.stale_free = self.stale_free.saturating_sub(1);
            self.stale_per_class[sc] = self.stale_per_class[sc].saturating_sub(1);
        }
    }

    /// **lazy-var-free-list **: drop every stale entry in one pass.
    ///
    /// Called only when the stale entries outnumber the live ones, so the `O(entries)` walk is
    /// paid at most once per entry that goes stale — amortised `O(1)` per pooling, against the
    /// unconditional `O(entries)` **per collection** it replaces. Without it a size class that
    /// stops being allocated would keep its stale entries forever: nothing pops them, so
    /// nothing discovers them.
    fn compact_free_lists(&mut self) {
        for sc in 0..self.free_lists.len() {
            self.compact_class(sc);
        }
    }

    /// **lazy-var-free-list **: drop every stale entry of **one** size class.
    ///
    /// Per class rather than all of them, because that is what makes a tight threshold
    /// affordable: a whole-region compaction measured 17–25 ms, so it could only be run once
    /// or twice a build, and the stale entries it was not running often enough to collect were
    /// worth ~40 MB of RSS. One class is a fraction of that and can run as soon as the class
    /// is more stale than live.
    fn compact_class(&mut self, sc: usize) {
        let entries = &mut self.free_lists[sc];
        let mut w = 0;
        for r in 0..entries.len() {
            // perf-free-slot-encoding: one sequential array read and one `pool_epoch` lookup
            // (a few KB, so `L1`). This used to dereference the entry's block header to find
            // its chunk — a random access per entry, **8.2 ns each over 1.62 M entries a
            // build**, which was the whole cost of this pass.
            let ci = entries[r].slot.chunk();
            if self.pool_epoch.get(ci).copied() == Some(entries[r].epoch) {
                entries[w] = entries[r];
                w += 1;
            }
        }
        entries.truncate(w);
        // Deliberately **not** `shrink_to_fit`: these lists are multi-megabyte, so handing the
        // capacity back means allocating the smaller buffer while the larger one is still
        // live. Measured, that spike costs more peak RSS than the slack it returns.
        self.stale_free = self.stale_free.saturating_sub(self.stale_per_class[sc]);
        self.stale_per_class[sc] = 0;
    }

    /// **lazy-var-free-list **: compact every class that now holds more stale
    /// entries than live ones. Called at the tail of a chunk reclaim, which is the only thing
    /// that creates stale entries.
    pub(super) fn compact_stale_classes(&mut self) {
        for sc in 0..self.free_lists.len() {
            // Half the class stale is the knee: tighter (a quarter, an eighth) walks the
            // lists more often for progressively less memory back — measured 34.4 / 30.4 /
            // 29.6 MB of free-list footprint at a half / a quarter / an eighth, against a
            // total pause of 381 / 402 / 453 ms. The memory is nearly flat; the pause is not.
            if self.stale_per_class[sc] * 2 > self.free_lists[sc].len() {
                self.compact_class(sc);
            }
        }
    }

    /// Re-initialize a recycled slot of chunk `ci` in place: write a fresh header whose
    /// generation is the one right after the tombstone's (odd again — alive), zero the payload.
    /// Returns the fresh handle.
    fn reinit_slot(&mut self, slot: NonNull<GcBlockHeader>, ci: usize, payload: usize, block_type: BlockType) -> VarGcRef {
        // SAFETY: `slot` came from this region's free list → it points at a valid, chunk-
        // owned, tombstoned header whose generation was bumped at tombstone time.
        let dead = unsafe { slot.as_ref() }.generation();
        debug_assert_eq!(dead & ALIVE_GEN_BIT, 0, "a free-listed slot is dead");
        let generation = dead.wrapping_add(1);
        // The recycled block is young again (gen_age 0); its tombstone took it out of the
        // young set, so it goes back in exactly once.
        self.write_fresh_header(slot, payload, block_type, generation);
        self.add_young(slot, ci);
        self.live_count += 1;
        // A recycled slot never moves, so only the live count changes.
        self.live_per_chunk[ci] += 1;
        VarGcRef::pack(slot, generation)
    }

    /// Write a fresh header at `ptr` (given generation, unmarked, age 0) and zero its payload.
    fn write_fresh_header(&self, ptr: NonNull<GcBlockHeader>, payload: usize, block_type: BlockType, generation: u16) {
        // SAFETY: `ptr` addresses freshly-carved (bump) or recycled (free-list) space large
        // enough for the header + `payload` bytes; we own exclusive access (`&mut self`).
        unsafe {
            ptr.as_ptr().write(GcBlockHeader::fresh(generation, payload, block_type));
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
        // The snapshot is odd (a live generation), so a match also says "alive".
        (header.generation() == handle.gen16()).then_some(header)
    }

    /// Tombstone the block behind `handle`: alive=false, bump generation, take it out of the
    /// young set, push its slot to the size-class free list. No-op (returns `false`) on a
    /// stale/already-dead handle or one outside this region. Finds the block's chunk by address
    /// ([`Self::chunk_of`]); sweeps, which know it, call [`Self::tombstone_in`].
    pub fn tombstone(&mut self, handle: VarGcRef) -> bool {
        match self.chunk_of(handle.addr()) {
            Some(ci) => self.tombstone_in(handle.header_ptr(), handle.gen16(), ci),
            None => false,
        }
    }

    /// [`Self::tombstone`] for the block at `ptr` in chunk `ci` whose live generation is
    /// `generation`. The generation CAS (odd → even) is both the stale-handle check and the
    /// race-winner test: exactly one caller tombstones a block.
    pub(super) fn tombstone_in(&mut self, ptr: NonNull<GcBlockHeader>, generation: u16, ci: usize) -> bool {
        // SAFETY: a handle / walk of this region → valid chunk-owned header.
        let header = unsafe { ptr.as_ref() };
        if generation & ALIVE_GEN_BIT == 0 {
            return false;
        }
        let dead = generation.wrapping_add(1);
        if header.generation.compare_exchange(generation, dead, Ordering::AcqRel, Ordering::Relaxed).is_err() {
            return false;
        }
        // Object model R6: the heap keeps entries keyed by this address (side tables, the
        // remembered set); tell it before the slot can be recycled or its chunk pooled.
        if header.needs_dead_notice() {
            self.flagged_dead.push(ptr.as_ptr() as usize);
        }
        // Run the payload finalizer (e.g. drop an object's type descriptor) exactly once, now
        // that this call won the alive 1→0 race, before the slot can be recycled.
        // SAFETY: the block is freshly reclaimed and still points at its initialized payload.
        unsafe { self.finalize_payload(ptr) };
        self.live_count -= 1;
        // add-incremental-chunk-reclaim: O(1) — the caller names the chunk, so the census stays
        // current without a lookup. The span is tracked here because tombstone is the only thing
        // that ever raises a block's generation past its chunk's epoch base.
        self.live_per_chunk[ci] -= 1;
        let span = dead.wrapping_sub(self.reuse_gen[ci]);
        if span > self.max_span_per_chunk[ci] {
            self.max_span_per_chunk[ci] = span;
        }
        self.remove_young(ptr, ci);
        // M12: a slot whose generation has run a whole 16-bit span past its chunk's epoch floor
        // stays tombstoned until the chunk is pooled (`holes::GEN_REUSE_SPAN`). A dedicated
        // chunk's block is never free-listed (its chunk is freed as soon as it is dead).
        if !self.chunks[ci].dedicated && self.reusable_gen(ci, dead) {
            // lazy-var-free-list: stamp the chunk's pooling count so a pop after the chunk is
            // recycled can tell this entry is stale.
            // perf-free-slot-encoding: the entry records where the slot is rather than pointing
            // at it, so nothing downstream has to read the header back to find its chunk.
            let off = ptr.as_ptr() as usize - self.chunks[ci].base.as_ptr() as usize;
            if let Some(slot) = FreeSlot::pack(ci, off) {
                let sc = payload_class(header.size()) as usize;
                self.free_lists[sc].push(FreeEntry { slot, epoch: self.pool_epoch[ci] });
            }
        }
        true
    }

    /// Iterate every currently-alive block, passing its handle + header to `visit`. Skips
    /// tombstoned slots. Order: chunk order, address order within a chunk.
    pub fn iterate_alive(&self, mut visit: impl FnMut(VarGcRef, &GcBlockHeader)) {
        for ci in 0..self.chunks.len() {
            self.for_each_block_in(ci, |ptr| {
                // SAFETY: `for_each_block_in` yields carved headers of a chunk this region owns.
                let header = unsafe { ptr.as_ref() };
                if header.is_alive() {
                    visit(VarGcRef::pack(ptr, header.generation()), header);
                }
            });
        }
    }

    /// STW sweep: tombstone every unmarked live block, clear the mark on survivors. Returns
    /// `(blocks reclaimed, bytes credited)`. (v1 = STW only; generational minor sweep is a
    /// later PR.)
    ///
    /// **fix-var-sweep-accounting**: the byte figure must mirror exactly what `used_bytes`
    /// was *charged* at alloc, or the auto-collect budget reads a number that drifts from
    /// the heap. See [`Self::alloc_charge_bytes`] for the per-`BlockType` rule.
    pub fn sweep(&mut self, major: crate::gc::refs::MarkKind) -> (usize, u64) {
        let (reclaimed, credited, _) = self.sweep_buckets(major, 0, usize::MAX);
        (reclaimed, credited)
    }

    /// **add-incremental-major-gc M2b**: [`Self::sweep`] over at most `max_buckets` chunks
    /// starting at chunk `from`. Returns `(reclaimed, credited bytes, next chunk)`; `next ==
    /// bucket_count()` means done. Resumable across STW slices for the same reason as
    /// the cycle's epoch: blocks allocated between slices carry it.
    pub fn sweep_buckets(&mut self, major: crate::gc::refs::MarkKind, from: usize, max_buckets: usize) -> (usize, u64, usize) {
        let mut reclaimed = 0;
        let mut credited: u64 = 0;
        let end = from.saturating_add(max_buckets).min(self.chunks.len());
        for ci in from.min(end)..end {
            // `for_each_block_in_mut` hands `&mut self` to the callback, so the dead are
            // tombstoned where they are judged — no staging list.
            self.for_each_block_in_mut(ci, |r, ptr| {
                // SAFETY: a carved header of a chunk this region owns.
                let header = unsafe { ptr.as_ref() };
                if !header.is_alive() {
                    return;
                }
                if header.is_marked(major) {
                    header.clear_minor_mark();
                    return;
                }
                let charge = Self::alloc_charge_bytes(header);
                if r.tombstone_in(ptr, header.generation(), ci) {
                    reclaimed += 1;
                    credited += charge;
                }
            });
        }
        (reclaimed, credited, end.max(from.min(self.chunks.len())))
    }

    /// Number of chunks a major sweep walks ([`Self::sweep_buckets`] cursor bound).
    #[inline]
    pub fn bucket_count(&self) -> usize {
        self.chunks.len()
    }

    /// The number of `used_bytes` a block added when it was allocated — the only figure sweep
    /// may credit back: `DATA_OFFSET + payload` for every kind (each allocator — strings,
    /// closures, objects, arrays — charges exactly its block's header and payload).
    #[inline]
    pub(crate) fn alloc_charge_bytes(header: &GcBlockHeader) -> u64 {
        (GcBlockHeader::DATA_OFFSET + header.size()) as u64
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

    /// The per-chunk census (tests: reconciled against a full walk of the blocks).
    #[cfg(test)]
    pub(crate) fn live_per_chunk_for_test(&self) -> Vec<u32> {
        self.live_per_chunk.clone()
    }

    /// Every carved block, live or dead (tests: what the region still tracks).
    #[cfg(test)]
    pub(crate) fn tracked_blocks_for_test(&self) -> Vec<NonNull<GcBlockHeader>> {
        let mut out = Vec::new();
        for ci in 0..self.chunks.len() {
            self.for_each_block_in(ci, |p| out.push(p));
        }
        out
    }

    /// Every block of the young set (tests).
    #[cfg(test)]
    pub(crate) fn young_blocks_for_test(&self) -> Vec<NonNull<GcBlockHeader>> {
        let mut out = Vec::new();
        self.iterate_young(|h, _| out.push(h.header_ptr()));
        out
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
            for ci in 0..self.chunks.len() {
                self.for_each_block_in(ci, |ptr| {
                    // SAFETY: chunk-owned header valid until the dealloc below.
                    let alive = unsafe { ptr.as_ref() }.is_alive();
                    if alive {
                        // SAFETY: alive block still owns its initialized payload; finalize once.
                        unsafe { self.finalize_payload(ptr) };
                    }
                });
            }
        }
        self.recommit_pool_for_drop();
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
