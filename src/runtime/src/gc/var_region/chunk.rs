//! Chunk layer: size classes, the owned raw chunk, the lock-free TLAB claim
//! ([`VarChunkClaim`]) and the [`VarRegion`] methods that grow / borrow / retire / reclaim
//! chunks. Allocation, resolve and sweep stay in the parent module.

use std::alloc::{alloc, handle_alloc_error, Layout};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8};

use super::block::{payload_ptr_of, BlockType, GcBlockHeader};
use super::{VarGcRef, VarRegion};

/// `size_class` sentinel for a block that exceeds the largest in-chunk class and got its own
/// dedicated, exactly-sized chunk.
pub(crate) const OVERSIZED_CLASS: u8 = u8::MAX;

/// Smallest total block footprint (header + payload), a power of two. 32 = 16 B header + up
/// to 16 B payload.
const MIN_BLOCK: usize = 32;

/// Byte capacity of a bump chunk (payloads larger than this get a dedicated chunk).
const CHUNK_BYTES: usize = 64 * 1024;

/// Chunk alignment — 16 so every block start (bumped to 8) and the header (align 8) are
/// satisfied with margin.
pub(super) const CHUNK_ALIGN: usize = 16;

/// The largest in-chunk size class (index). `1 << MAX_CLASS <= CHUNK_BYTES`.
const MAX_CLASS: u8 = {
    // trailing_zeros of the largest power of two that fits a chunk.
    let mut c = MIN_BLOCK;
    let mut idx = MIN_BLOCK.trailing_zeros();
    while c << 1 <= CHUNK_BYTES {
        c <<= 1;
        idx += 1;
    }
    idx as u8
};

/// Round a requested payload size up to its total block footprint + size class.
/// Returns `(total_footprint_bytes, size_class)`. `size_class == OVERSIZED_CLASS` when the
/// block needs a dedicated chunk.
#[inline]
pub(crate) fn class_for(payload: usize) -> (usize, u8) {
    let total = GcBlockHeader::DATA_OFFSET + payload;
    let footprint = total.max(MIN_BLOCK).next_power_of_two();
    if footprint > CHUNK_BYTES {
        // Oversized: dedicated chunk sized to exactly hold header + payload, 16-aligned.
        let dedicated = (total + CHUNK_ALIGN - 1) & !(CHUNK_ALIGN - 1);
        (dedicated, OVERSIZED_CLASS)
    } else {
        (footprint, footprint.trailing_zeros() as u8)
    }
}

/// Number of size-class free-list buckets (indices `0..=MAX_CLASS`).
pub(super) const NUM_CLASSES: usize = MAX_CLASS as usize + 1;

/// A raw, owned chunk of GC block memory. Freed in [`VarRegion::drop`].
pub(super) struct Chunk {
    /// 16-aligned base pointer from the global allocator.
    pub(super) base: NonNull<u8>,
    /// Total byte capacity of this chunk (`CHUNK_BYTES` for bump chunks, exact size for
    /// dedicated oversized chunks).
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
                type_tag: block_type as u8,
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
    /// Bump-allocate `footprint` bytes (already a power of two ≤ CHUNK_BYTES) from the
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
        // Footprint is a power of two ≥ 32 and the chunk base is 16-aligned, so `base + off`
        // is at least 8-aligned (off is a multiple of the footprint, itself a multiple of 8).
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
        self.chunks.push(Chunk::new(cap));
        self.borrowed.push(false);
        self.reuse_gen.push(0);
        self.chunks.len() - 1
    }

    /// **add-gc-tlab stage 3**: the chunk index owning `ptr` (address ∈ `[base, base+cap)`),
    /// or `None` for an oversized/dedicated chunk not eligible for pooling. Linear over chunks
    /// (typically few tens); used only at reclaim (STW, off the hot path).
    fn chunk_of(&self, ptr: NonNull<GcBlockHeader>) -> Option<usize> {
        let addr = ptr.as_ptr() as usize;
        for (ci, c) in self.chunks.iter().enumerate() {
            let base = c.base.as_ptr() as usize;
            if addr >= base && addr < base + c.cap {
                return Some(ci);
            }
        }
        None
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
        self.all_blocks.extend(claim.local_blocks.drain(..));
        self.live_count += n;
        self.borrowed[claim.chunk_idx] = false;
    }

    /// **add-gc-tlab stage 3 (D7 for var)**: after a sweep, recycle every fully-dead **bump**
    /// chunk (all its blocks tombstoned) into `var_free_chunk_pool`. Because var blocks are
    /// variable-size and don't re-align on reuse, ABA is prevented by bumping the chunk's
    /// `reuse_gen` **above every generation any block in it reached** before re-bumping. Purges
    /// the reclaimed chunk's dead blocks from `all_blocks` and `free_lists`. Skips borrowed
    /// chunks, the current ambient bump chunk, dedicated (oversized) chunks, and already-pooled
    /// chunks. Runs under STW at the sweep tail. Returns the count reclaimed.
    pub fn reclaim_dead_var_chunks(&mut self) -> usize {
        let n_chunks = self.chunks.len();
        // Per-chunk: has any live block? and the max generation seen (for the reuse_gen bump).
        let mut has_live = vec![false; n_chunks];
        let mut max_gen = vec![0u32; n_chunks];
        let mut any_block = vec![false; n_chunks];
        for &ptr in &self.all_blocks {
            let Some(ci) = self.chunk_of(ptr) else { continue };
            any_block[ci] = true;
            // SAFETY: all_blocks pointers are chunk-owned, valid for the region's lifetime.
            let header = unsafe { ptr.as_ref() };
            let g = header.generation();
            if g > max_gen[ci] {
                max_gen[ci] = g;
            }
            if header.is_alive() {
                has_live[ci] = true;
            }
        }
        let ambient = self.bump_chunk;
        let already: std::collections::HashSet<usize> =
            self.var_free_chunk_pool.iter().copied().collect();
        let mut reclaim: Vec<usize> = Vec::new();
        for ci in 0..n_chunks {
            if self.borrowed[ci]
                || Some(ci) == ambient
                || already.contains(&ci)
                || self.chunks[ci].cap != CHUNK_BYTES   // dedicated/oversized → not pooled
                || !any_block[ci]                        // never-used → nothing to recycle
                || has_live[ci]
            {
                continue;
            }
            reclaim.push(ci);
        }
        if reclaim.is_empty() {
            return 0;
        }
        // Snapshot reclaimed chunks' address ranges so the retain closures don't borrow `self`.
        let ranges: Vec<(usize, usize)> = reclaim
            .iter()
            .map(|&ci| {
                let base = self.chunks[ci].base.as_ptr() as usize;
                (base, base + self.chunks[ci].cap)
            })
            .collect();
        let in_reclaimed = |p: NonNull<GcBlockHeader>| {
            let addr = p.as_ptr() as usize;
            ranges.iter().any(|&(lo, hi)| addr >= lo && addr < hi)
        };
        // Purge reclaimed chunks' blocks from all_blocks + free_lists (they're being recycled).
        self.all_blocks.retain(|&p| !in_reclaimed(p));
        for fl in &mut self.free_lists {
            fl.retain(|&p| !in_reclaimed(p));
        }
        for &ci in &reclaim {
            // Bump reuse_gen above every generation this chunk's blocks reached, so a fresh
            // re-bump can't mint an (address, generation) pair matching a stale VarGcRef.
            self.reuse_gen[ci] = max_gen[ci].wrapping_add(1);
            self.var_free_chunk_pool.push(ci);
        }
        reclaim.len()
    }
}
