//! Chunk layer: size classes, the owned raw chunk, the lock-free TLAB claim
//! ([`VarChunkClaim`]) and the [`VarRegion`] methods that grow / borrow / retire / reclaim
//! chunks. Allocation, resolve and sweep stay in the parent module.

use std::alloc::{alloc, dealloc, handle_alloc_error, Layout};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, AtomicUsize, Ordering};

use super::block::{payload_ptr_of, BlockType, GcBlockHeader};
use super::{VarGcRef, VarRegion};

/// `size_class` sentinel for a block that exceeds the largest in-chunk class and got its own
/// dedicated, exactly-sized chunk.
pub(crate) const OVERSIZED_CLASS: u8 = u8::MAX;

/// Smallest total block footprint (header + payload), a power of two. 32 = 16 B header + up
/// to 16 B payload.
pub(super) const MIN_BLOCK: usize = 32;

/// Byte capacity of a bump chunk (payloads larger than this get a dedicated chunk).
pub(super) const CHUNK_BYTES: usize = 64 * 1024;

/// Chunk alignment — 16 so every block start (bumped to 8) and the header (align 8) are
/// satisfied with margin.
pub(super) const CHUNK_ALIGN: usize = 16;

/// Sub-classes per octave, as a log2. `0` would be plain powers of two; `2` splits every
/// octave into four (32/40/48/56, 64/80/96/112, …) — the mimalloc / jemalloc shape.
///
/// **shrink-var-size-classes (2026-09-07)**: plain powers of two rounded the average live
/// block from 118 B of payload+header up to 188 B of footprint. Measured over a
/// `z42c.semantics --release --no-incremental` build (2.74 M live var blocks): 516.4 MB of
/// footprint against 323.4 MB of logical bytes — 193.0 MB of pure rounding waste, 17% of the
/// process RSS. Quarter-octave classes cut that to 57.9 MB. The waste was concentrated in a
/// few shapes the octave boundaries straddled badly, above all the 293 849 blocks of 257..320
/// total bytes that each took a 512-byte slot (~59 MB on their own).
pub(super) const SUB_LOG2: u32 = 2;

/// The largest in-chunk size class index. Class indices pack as `octave << SUB_LOG2 | sub`,
/// so the top one is the class of a `CHUNK_BYTES` footprint (`sub == 0`).
const MAX_CLASS: u8 = (CHUNK_BYTES.trailing_zeros() << SUB_LOG2) as u8;

/// Round a requested payload size up to its total block footprint + size class.
/// Returns `(total_footprint_bytes, size_class)`. `size_class == OVERSIZED_CLASS` when the
/// block needs a dedicated chunk.
///
/// Each class holds exactly one footprint, which is what lets `alloc` hand a free-list slot
/// straight to a new block of any payload in the class without re-checking capacity.
/// **add-loh-bytes-knob (2026-09-08)**: the block-footprint threshold above which a block gets
/// its own exactly-sized chunk instead of a size class inside a bump chunk (`Z42_GC_LOH_BYTES`).
///
/// **Process-global on purpose.** `class_for` is called from the lock-free TLAB fast path
/// (`arc_heap/alloc.rs`), which has no region and no heap reference at hand, so a per-heap
/// field is not reachable there. A process with several VMs shares one setting — the same as
/// before this knob existed, when it was a `const`.
///
/// Never above [`CHUNK_BYTES`]: a block larger than a bump chunk cannot be bump-allocated at
/// all, so raising the threshold past it would route blocks nowhere. Lowering it sends more
/// blocks down the dedicated-chunk path, where death returns the memory to the allocator
/// immediately (see `reclaim_dead_var_chunks`) at the cost of a `malloc` per block.
static LOH_BYTES: AtomicUsize = AtomicUsize::new(CHUNK_BYTES);

/// Set the large-object threshold for this process. Called once from VM construction; clamped
/// to `[MIN_BLOCK, CHUNK_BYTES]`.
pub fn set_loh_bytes(bytes: usize) {
    LOH_BYTES.store(clamp_loh_bytes(bytes), Ordering::Relaxed);
}

/// The clamp [`set_loh_bytes`] applies, as a pure function so it can be tested without storing
/// into the process-global threshold (which would race every concurrently running test that
/// allocates a var block).
#[inline]
pub(crate) fn clamp_loh_bytes(bytes: usize) -> usize {
    bytes.clamp(MIN_BLOCK, CHUNK_BYTES)
}

/// The large-object threshold currently in force (diagnostics / tests).
pub fn loh_bytes() -> usize {
    LOH_BYTES.load(Ordering::Relaxed)
}

#[inline]
pub(crate) fn class_for(payload: usize) -> (usize, u8) {
    class_for_with_limit(payload, LOH_BYTES.load(Ordering::Relaxed))
}

/// [`class_for`] with the large-object threshold passed in — the testable core (the live
/// threshold is process-global, so a test that stored into it would race every other test
/// allocating a var block).
#[inline]
pub(crate) fn class_for_with_limit(payload: usize, loh_bytes: usize) -> (usize, u8) {
    let total = GcBlockHeader::DATA_OFFSET + payload;
    let t = total.max(MIN_BLOCK);
    // Round up to the next quarter-octave step. `t >= MIN_BLOCK` puts `oct` at 5 or more, so
    // `step` is at least 8 and every footprint stays 8-aligned — the bump and TLAB offsets
    // are only ever advanced by a footprint, and their alignment rests on that.
    let oct = usize::BITS - 1 - t.leading_zeros();
    let step = (1usize << oct) >> SUB_LOG2;
    let footprint = (t + step - 1) & !(step - 1);
    if footprint > loh_bytes {
        // Oversized: dedicated chunk sized to exactly hold header + payload, 16-aligned.
        let dedicated = (total + CHUNK_ALIGN - 1) & !(CHUNK_ALIGN - 1);
        return (dedicated, OVERSIZED_CLASS);
    }
    // Rounding can carry into the next octave (57..64 → 64), so take the octave off the
    // rounded footprint rather than reusing `oct`.
    let oct = usize::BITS - 1 - footprint.leading_zeros();
    let sub = (footprint - (1usize << oct)) >> (oct - SUB_LOG2);
    (footprint, ((oct << SUB_LOG2) | sub as u32) as u8)
}

/// Number of size-class free-list buckets (indices `0..=MAX_CLASS`). The bottom
/// `MIN_BLOCK.trailing_zeros() << SUB_LOG2` buckets are unreachable (no footprint is smaller
/// than `MIN_BLOCK`) and stay empty — indexing directly by the packed class beats folding the
/// range down on every alloc, and an empty `Vec` costs 24 bytes.
pub(super) const NUM_CLASSES: usize = MAX_CLASS as usize + 1;

/// A raw, owned chunk of GC block memory. Freed in [`VarRegion::drop`], or earlier by
/// [`Chunk::free_in_place`] when a dedicated (oversized) chunk's only block dies.
pub(super) struct Chunk {
    /// 16-aligned base pointer from the global allocator. Dangling once the chunk is freed.
    pub(super) base: NonNull<u8>,
    /// Total byte capacity of this chunk (`CHUNK_BYTES` for bump chunks, exact size for
    /// dedicated oversized chunks). **`0` marks a freed slot** — see [`Chunk::free_in_place`].
    pub(super) cap: usize,
}

impl Chunk {
    /// Allocate a fresh `cap`-byte, 16-aligned chunk. Aborts on OOM (a partly-built region
    /// can't recover a null chunk).
    pub(super) fn new(cap: usize) -> Self {
        let layout = Layout::from_size_align(cap, CHUNK_ALIGN).expect("chunk layout");
        // SAFETY: `cap` is non-zero (>= MIN_BLOCK). On OOM abort rather than return a
        // dangling base.
        let raw = unsafe { alloc(layout) };
        let base = NonNull::new(raw).unwrap_or_else(|| handle_alloc_error(layout));
        Chunk { base, cap }
    }

    /// The `Layout` this chunk was allocated with (for `dealloc`).
    #[inline]
    pub(super) fn layout(&self) -> Layout {
        Layout::from_size_align(self.cap, CHUNK_ALIGN).expect("chunk layout")
    }

    /// **fix-loh-never-freed (2026-09-08)**: hand this chunk's memory back to the global
    /// allocator, leaving the slot behind as a tombstone (`cap == 0`, dangling base).
    ///
    /// The slot has to stay: `bump_chunk`, `borrowed`, `reuse_gen` and `var_free_chunk_pool`
    /// all address chunks **by index**, so removing an element would silently renumber every
    /// chunk after it. [`VarRegion::push_chunk`] recycles tombstoned slots and
    /// [`VarRegion::drop`] skips them.
    ///
    /// Caller must have proven no block in this chunk is still tracked (see
    /// [`VarRegion::reclaim_dead_var_chunks`], which is the only caller).
    pub(super) fn free_in_place(&mut self) {
        if self.is_freed() {
            return;
        }
        // SAFETY: allocated in `new` with exactly this layout, and freed exactly once — the
        // `cap = 0` below makes a second call and the `Drop` pass no-ops.
        unsafe { dealloc(self.base.as_ptr(), self.layout()) };
        self.base = NonNull::dangling();
        self.cap = 0;
    }

    /// Whether this slot is a tombstone left by [`Chunk::free_in_place`] (owns no memory).
    #[inline]
    pub(super) fn is_freed(&self) -> bool {
        self.cap == 0
    }
}

/// **add-gc-tlab stage 3 (2026-08-29)**: a mutator thread's exclusive write claim on one
/// `VarRegion` bump chunk (design D4). Produced by [`VarRegion::borrow_chunk`] (under the
/// region lock), then filled **lock-free** by the owning thread via [`VarChunkClaim::fill`]
/// until a block doesn't fit (`fill` returns `None`); [`VarRegion::retire_chunk`] then appends
/// the filled blocks into `all_blocks`.
///
/// # Safety / invariants
/// - `base` is the raw pointer to `Region`-owned chunk memory (a separate `malloc`, never moved
///   until the region drops), valid for the region's lifetime.
/// - The chunk is `borrowed` while a claim is live, so only the owning thread touches it; its
///   blocks are absent from `all_blocks` until retire, so no GC scan reads them. That single
///   writer / no reader discipline makes the un-synchronized `fill` writes sound.
/// - Only non-oversized blocks (`footprint ≤ CHUNK_BYTES`) go through the TLAB; oversized and
///   free-list reuse stay on the locked `VarRegion::alloc` path.
pub struct VarChunkClaim {
    chunk_idx: usize,
    base: *mut u8,
    cap: usize,
    off: usize,
    /// Generation stamped on every block filled from this claim (the chunk's `reuse_gen` at
    /// borrow time). Fresh chunks → 0; recycled chunks → a value above every prior occupant's
    /// generation (ABA guard).
    base_gen: u32,
    /// Blocks filled from this claim, appended to `all_blocks` at retire.
    local_blocks: Vec<NonNull<GcBlockHeader>>,
}

impl VarChunkClaim {
    /// Bump-fill one block of `payload` bytes / `block_type` into the claimed chunk **without
    /// any lock**. Returns the stable [`VarGcRef`], or `None` when the chunk can't fit the block
    /// (caller retires + borrows a fresh one, or — for an oversized block — takes the locked
    /// path). Caller must only pass non-oversized payloads (checked via `class_for`).
    ///
    /// # Safety
    /// Owner-thread-exclusive (see the type's safety contract); the chunk is `borrowed` so no
    /// concurrent reader exists. `footprint`/`size_class` come from `class_for(payload)`.
    #[inline]
    pub(crate) fn fill(&mut self, payload: usize, footprint: usize, size_class: u8, block_type: BlockType) -> Option<VarGcRef> {
        if self.off + footprint > self.cap {
            return None;
        }
        let off = self.off;
        debug_assert_eq!(off % 8, 0, "var TLAB bump offset must stay 8-aligned");
        // SAFETY: off + footprint <= cap, chunk base is 16-aligned; owner-exclusive.
        let raw = unsafe { self.base.add(off) };
        let header_ptr = unsafe { NonNull::new_unchecked(raw as *mut GcBlockHeader) };
        // SAFETY: fresh space large enough for header + payload; write header + zero payload.
        unsafe {
            header_ptr.as_ptr().write(GcBlockHeader {
                generation: AtomicU32::new(self.base_gen),
                size: payload as u32,
                marked: AtomicU8::new(0),
                alive: AtomicBool::new(true),
                type_tag: AtomicU8::new(GcBlockHeader::pack_tag(block_type, 0, true)),
                size_class,
            });
            let data = payload_ptr_of(header_ptr);
            std::ptr::write_bytes(data, 0, payload);
        }
        self.off += footprint;
        self.local_blocks.push(header_ptr);
        Some(VarGcRef::pack(header_ptr, self.base_gen))
    }

    /// True while the claim can still fit a block of `footprint` bytes.
    #[inline]
    pub(crate) fn has_room(&self, footprint: usize) -> bool {
        self.off + footprint <= self.cap
    }
}

impl VarRegion {
    /// Bump-allocate `footprint` bytes (already rounded to a size class ≤ CHUNK_BYTES) from the
    /// current chunk, growing a new chunk when it doesn't fit. Returns the block header ptr.
    pub(super) fn bump(&mut self, footprint: usize) -> NonNull<GcBlockHeader> {
        let need_new = match self.bump_chunk {
            None => true,
            Some(ci) => self.bump_off + footprint > self.chunks[ci].cap,
        };
        if need_new {
            let ci = self.push_chunk(CHUNK_BYTES);
            self.bump_chunk = Some(ci);
            self.bump_off = 0;
        }
        let ci = self.bump_chunk.expect("bump chunk set above");
        let off = self.bump_off;
        // Every footprint is a multiple of 8 and ≥ 32 (see `class_for`) and the chunk base is
        // 16-aligned, so `base + off` stays at least 8-aligned.
        debug_assert_eq!(off % 8, 0, "bump offset must stay 8-aligned");
        self.bump_off += footprint;
        // SAFETY: `off + footprint <= cap` (ensured above), so `base + off` is in-bounds and
        // has room for the whole block.
        let raw = unsafe { self.chunks[ci].base.as_ptr().add(off) };
        // SAFETY: `raw` is non-null (offset into a non-null chunk base) and 8-aligned.
        unsafe { NonNull::new_unchecked(raw as *mut GcBlockHeader) }
    }

    /// Allocate a dedicated, exactly-sized chunk for an oversized block. Returns the header
    /// ptr at the chunk base.
    pub(super) fn alloc_dedicated(&mut self, footprint: usize) -> NonNull<GcBlockHeader> {
        let ci = self.push_chunk(footprint);
        let base = self.chunks[ci].base;
        // SAFETY: chunk base is 16-aligned (≥ header align 8) and non-null.
        unsafe { NonNull::new_unchecked(base.as_ptr() as *mut GcBlockHeader) }
    }

    /// **add-gc-tlab stage 3**: append a fresh chunk of `cap` bytes and grow every parallel
    /// per-chunk table (`borrowed` false, `reuse_gen` 0). Returns the new chunk index. Single
    /// growth point so `chunks` / `borrowed` / `reuse_gen` stay length-consistent.
    fn push_chunk(&mut self, cap: usize) -> usize {
        let chunk = Chunk::new(cap);
        // fix-loh-never-freed: reuse a slot left behind by a freed dedicated chunk first.
        // Without this, a workload that churns large objects grows `chunks` / `borrowed` /
        // `reuse_gen` forever (~29 B per dead oversized block) — RSS would still creep up,
        // just 3000× slower, and this change's whole point is that it stops creeping.
        if let Some(ci) = self.free_chunk_slots.pop() {
            debug_assert!(self.chunks[ci].is_freed(), "free_chunk_slots must hold only tombstones");
            // `Chunk` has no `Drop` (the region frees chunk memory), so overwriting the
            // tombstone can't double-free.
            self.chunks[ci] = chunk;
            self.borrowed[ci] = false;
            self.reuse_gen[ci] = 0;
            return ci;
        }
        self.chunks.push(chunk);
        self.borrowed.push(false);
        self.reuse_gen.push(0);
        self.chunks.len() - 1
    }

    /// **add-gc-tlab stage 3**: hand a whole bump chunk's write ownership to a mutator's TLAB
    /// (design D4). Recycles a fully-dead chunk from `var_free_chunk_pool` (its `reuse_gen`
    /// already bumped past every prior occupant) or grows a fresh `CHUNK_BYTES` one. Marks the
    /// chunk `borrowed`; the returned claim carries the chunk base pointer (stable — chunk
    /// memory never moves) and the generation to stamp on filled blocks.
    pub fn borrow_chunk(&mut self) -> VarChunkClaim {
        let ci = match self.var_free_chunk_pool.pop() {
            Some(ci) => ci,
            None => self.push_chunk(CHUNK_BYTES),
        };
        self.borrowed[ci] = true;
        VarChunkClaim {
            chunk_idx: ci,
            base: self.chunks[ci].base.as_ptr(),
            cap: self.chunks[ci].cap,
            off: 0,
            base_gen: self.reuse_gen[ci],
            local_blocks: Vec::new(),
        }
    }

    /// **add-gc-tlab stage 3**: merge a TLAB's filled blocks back into the region (design D4):
    /// append them to `all_blocks`, bump `live_count`, and clear the chunk's `borrowed` flag so
    /// it rejoins sweep/reclaim. The chunk's unused tail is abandoned until the whole chunk dies
    /// and is reclaimed (bounded ≤ CHUNK_BYTES per safepoint retire).
    pub fn retire_chunk(&mut self, claim: &mut VarChunkClaim) {
        let n = claim.local_blocks.len();
        // fix-minor-gc-skips-var-region: TLAB-filled blocks are freshly allocated, so they
        // are young by definition (`fill` stamps `gen_age = 0` and the young bit). They join
        // `young_list` here for the same reason they join `all_blocks` here — until retire,
        // the region cannot see them at all.
        if self.generational {
            self.young_list.extend(claim.local_blocks.iter().copied());
        }
        self.all_blocks.extend(claim.local_blocks.drain(..));
        self.live_count += n;
        self.borrowed[claim.chunk_idx] = false;
    }

    /// **add-gc-tlab stage 3 (D7 for var)** + **fix-loh-never-freed (2026-09-08)**: after a
    /// sweep, deal with every chunk whose blocks are now all tombstoned, by kind:
    ///
    /// - **bump chunk** (`cap == CHUNK_BYTES`) → into `var_free_chunk_pool` for
    ///   [`Self::borrow_chunk`] to re-bump. Because var blocks are variable-size and don't
    ///   re-align on reuse, ABA is prevented by bumping the chunk's `reuse_gen` **above every
    ///   generation any block in it reached** before re-bumping.
    /// - **dedicated chunk** (an oversized block's exactly-sized chunk) → `dealloc`, straight
    ///   back to the global allocator, and its slot goes on `free_chunk_slots`. These are never
    ///   pooled: their sizes are far too scattered (measured 80 KB–1.3 MB over 173 live blocks)
    ///   for a size-bucketed pool to be anything but memory that never comes back — design
    ///   decision D-3. Before this, `tombstone` didn't free-list `OVERSIZED_CLASS` and this
    ///   function skipped `cap != CHUNK_BYTES`, so **a dead large object held its memory until
    ///   the VM exited**.
    ///
    /// Either way the chunk's blocks are purged from `all_blocks`, `free_lists` and
    /// `young_list` first — for a pooled chunk because the memory is about to be re-bumped
    /// under a fresh generation, for a freed one because the memory is about to stop existing.
    /// Skips borrowed chunks, the current ambient bump chunk, and already-pooled chunks.
    /// Runs under STW at the sweep tail.
    ///
    /// # Freeing vs. the generation guard
    /// A pooled chunk keeps answering `resolve` on a stale [`VarGcRef`] — it reads a live
    /// header whose `reuse_gen` no longer matches, so the handle degrades to `None`. A freed
    /// chunk cannot: its memory is gone, and a stale handle into it is a use-after-free. That
    /// is sound only because a block reaches here **tombstoned by the sweep that just ran**,
    /// i.e. unreachable from every root — no live structure can still hold a handle to it. It
    /// does mean a marking bug that used to surface as a `None` (a `Null` where a `Str` was
    /// expected) now surfaces as memory corruption, for oversized blocks only.
    pub fn reclaim_dead_var_chunks(&mut self) -> VarChunkReclaim {
        let index = ChunkIndex::build(&self.chunks);
        let survey = self.survey_chunks(&index);
        let (pool, free) = self.partition_dead_chunks(&survey);
        if pool.is_empty() && free.is_empty() {
            return VarChunkReclaim::default();
        }
        self.purge_blocks(&index, &pool, &free);
        for &ci in &pool {
            // Bump reuse_gen above every generation this chunk's blocks reached, so a fresh
            // re-bump can't mint an (address, generation) pair matching a stale VarGcRef.
            self.reuse_gen[ci] = survey.max_gen[ci].wrapping_add(1);
            self.var_free_chunk_pool.push(ci);
        }
        let mut freed_bytes = 0u64;
        for &ci in &free {
            freed_bytes += self.chunks[ci].cap as u64;
            self.chunks[ci].free_in_place();
            self.free_chunk_slots.push(ci);
        }
        VarChunkReclaim { pooled: pool.len(), freed_chunks: free.len(), freed_bytes }
    }

    /// Per-chunk liveness census, read off `all_blocks` in one pass. Split out of
    /// [`Self::reclaim_dead_var_chunks`] so that function stays readable.
    fn survey_chunks(&self, index: &ChunkIndex) -> ChunkSurvey {
        let n = self.chunks.len();
        let mut survey = ChunkSurvey {
            has_live: vec![false; n],
            max_gen: vec![0u32; n],
            any_block: vec![false; n],
        };
        for &ptr in &self.all_blocks {
            let Some(ci) = index.locate(ptr.as_ptr() as usize) else { continue };
            survey.any_block[ci] = true;
            // SAFETY: all_blocks pointers are chunk-owned, valid for the region's lifetime.
            let header = unsafe { ptr.as_ref() };
            let g = header.generation();
            if g > survey.max_gen[ci] {
                survey.max_gen[ci] = g;
            }
            if header.is_alive() {
                survey.has_live[ci] = true;
            }
        }
        survey
    }

    /// Split the fully-dead chunks into `(to pool, to free)`. A chunk qualifies only if it
    /// held at least one block (a never-used one has nothing to recycle) and holds no live
    /// one; borrowed chunks, the ambient bump chunk and already-pooled chunks are excluded.
    fn partition_dead_chunks(&self, survey: &ChunkSurvey) -> (Vec<usize>, Vec<usize>) {
        let ambient = self.bump_chunk;
        let already: std::collections::HashSet<usize> =
            self.var_free_chunk_pool.iter().copied().collect();
        let mut pool = Vec::new();
        let mut free = Vec::new();
        for ci in 0..self.chunks.len() {
            if self.borrowed[ci]
                || Some(ci) == ambient
                || already.contains(&ci)
                || self.chunks[ci].is_freed()
                || !survey.any_block[ci]
                || survey.has_live[ci]
            {
                continue;
            }
            if self.chunks[ci].cap == CHUNK_BYTES {
                pool.push(ci);
            } else {
                free.push(ci); // dedicated / oversized — dealloc, never pooled (D-3)
            }
        }
        (pool, free)
    }

    /// Whether `addr` falls inside a chunk this region still owns (tests: proves nothing
    /// tracked points into memory that was handed back to the allocator).
    #[cfg(test)]
    pub(crate) fn owns_addr(&self, addr: usize) -> bool {
        ChunkIndex::build(&self.chunks).locate(addr).is_some()
    }

    /// Drop every block that lives in a chunk being pooled or freed from the three lists that
    /// hold raw block pointers. Flagging the chunks by index keeps each closure at a binary
    /// search + one lookup per block, rather than a scan of every reclaimed range (which was
    /// the second `O(blocks × chunks)` term in this function).
    ///
    /// `young_list` is purged for the same reason as the other two: a pooled chunk is
    /// re-bumped from offset 0, so a surviving entry would dangle onto whatever lands at that
    /// address next — and the minor sweep would happily age or tombstone the new occupant.
    fn purge_blocks(&mut self, index: &ChunkIndex, pool: &[usize], free: &[usize]) {
        let mut is_reclaimed = vec![false; self.chunks.len()];
        for &ci in pool.iter().chain(free) {
            is_reclaimed[ci] = true;
        }
        let in_reclaimed =
            |p: NonNull<GcBlockHeader>| index.locate(p.as_ptr() as usize).is_some_and(|ci| is_reclaimed[ci]);
        self.all_blocks.retain(|&p| !in_reclaimed(p));
        for fl in &mut self.free_lists {
            fl.retain(|&p| !in_reclaimed(p));
        }
        self.young_list.retain(|&p| !in_reclaimed(p));
    }
}

/// What one [`VarRegion::reclaim_dead_var_chunks`] pass did. `pooled` chunks keep their
/// memory (it is handed back to a TLAB); `freed_*` counts memory actually returned to the
/// global allocator.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VarChunkReclaim {
    /// Fully-dead bump chunks moved into `var_free_chunk_pool`.
    pub pooled: usize,
    /// Dedicated (oversized) chunks `dealloc`'d.
    pub freed_chunks: usize,
    /// Bytes those dedicated chunks gave back to the allocator.
    pub freed_bytes: u64,
}

/// Per-chunk census produced by [`VarRegion::survey_chunks`], indexed by chunk index.
struct ChunkSurvey {
    /// Chunk holds at least one still-alive block.
    has_live: Vec<bool>,
    /// Highest `generation` any block in the chunk reached (the ABA floor for a re-bump).
    max_gen: Vec<u32>,
    /// Chunk holds at least one block at all (a never-used chunk has nothing to recycle).
    any_block: Vec<bool>,
}

/// Address → chunk index in `O(log C)`.
///
/// This used to be a linear scan over `chunks` run once per block in `all_blocks`, making
/// the reclaim pass `O(blocks × chunks)` — measured on `z42c.semantics --release
/// --no-incremental` with a 128MB budget, that one call was 92–98% of every GC pause and
/// doubled each cycle (494ms → 975ms → 1490ms → 3072ms out of pauses of 536ms → 3143ms).
/// Same shape as the `young_list` scan fixed by #519, different region.
struct ChunkIndex {
    /// `(lo, hi, chunk index)` sorted by `lo`. Freed slots are left out — they own no memory,
    /// so no block address can fall in them.
    by_addr: Vec<(usize, usize, usize)>,
}

impl ChunkIndex {
    fn build(chunks: &[Chunk]) -> Self {
        let mut by_addr: Vec<(usize, usize, usize)> = chunks
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.is_freed())
            .map(|(ci, c)| {
                let base = c.base.as_ptr() as usize;
                (base, base + c.cap, ci)
            })
            .collect();
        by_addr.sort_unstable_by_key(|&(lo, _, _)| lo);
        ChunkIndex { by_addr }
    }

    /// Chunks never overlap, so the only candidate for `addr` is the last entry whose base is
    /// ≤ it; `None` for a block in no chunk at all (never happens for chunk-owned blocks).
    #[inline]
    fn locate(&self, addr: usize) -> Option<usize> {
        let i = self.by_addr.partition_point(|&(lo, _, _)| lo <= addr);
        let (_, hi, ci) = *self.by_addr.get(i.checked_sub(1)?)?;
        (addr < hi).then_some(ci)
    }
}
