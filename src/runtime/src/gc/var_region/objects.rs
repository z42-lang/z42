//! Object model R6: what the variable-length region does for **object** blocks
//! (`BlockType::Object`) beyond plain byte allocation — the flagged-dead list the heap drains
//! for its side tables, the per-chunk epoch weak references check, iteration by block type, the
//! minor-sweep report the heap needs (promotion-policy samples, newly-old objects) and detached
//! blocks (an object laid out exactly like a heap one, owned outside any region).
//!
//! The allocator itself stays type-agnostic: nothing here reads an object's payload.

use std::alloc::{alloc, dealloc, handle_alloc_error, Layout};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8};

use super::block::{payload_ptr_of, BlockType, GcBlockHeader};
use super::chunk::{class_for, CHUNK_ALIGN};
use super::{VarGcRef, VarRegion};

/// What a minor sweep of the region ([`VarRegion::sweep_young_with`]) did, with the object
/// details the heap's generational bookkeeping needs.
#[derive(Debug, Default)]
pub struct VarMinorSweep {
    /// Blocks tombstoned.
    pub reclaimed: usize,
    /// `used_bytes` the tombstoned blocks were charged at allocation (see
    /// `VarRegion::alloc_charge_bytes`).
    pub credited: u64,
    /// Object blocks that crossed the promotion age in this sweep — old objects that may still
    /// point at young ones, so the heap decides whether to remember them.
    pub newly_old_objects: Vec<VarGcRef>,
    /// Bytes those objects carry into the old generation (their allocation charge).
    pub promoted_object_bytes: u64,
}

impl VarRegion {
    /// Hand over (and forget) the addresses of every flagged block tombstoned since the last
    /// call — see `block::SIDE_BIT`. The heap must drain this before any chunk is pooled or
    /// freed: until then a listed address still holds the dead block's header.
    pub fn take_flagged_dead(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.flagged_dead)
    }

    /// The reuse epoch of chunk `ci` — bumped whenever the chunk is pooled (its memory is about
    /// to be re-bumped from offset 0) or freed. A weak reference records it at creation: while
    /// it is unchanged, the referent's address is still a block header of this chunk, so the
    /// header's generation guard is meaningful. `None` for an out-of-range index.
    pub fn chunk_epoch(&self, ci: u32) -> Option<u32> {
        let ci = ci as usize;
        (ci < self.chunks.len() && !self.chunks[ci].is_freed()).then(|| self.pool_epoch[ci])
    }

    /// Visit every alive block of type `bt` (diagnostics, heap iteration). Read-only; same order
    /// as [`Self::iterate_alive`].
    pub fn iterate_alive_of(&self, bt: BlockType, mut visit: impl FnMut(VarGcRef, &GcBlockHeader)) {
        self.iterate_alive(|h, header| {
            if header.block_type() == bt {
                visit(h, header);
            }
        });
    }

    /// The object blocks a lowered promotion line has just made old: young-set members at or past
    /// the (already lowered) [`Self::promotion_age`], not yet remembered. Adaptive promotion asks
    /// for them (`ArcMagrGC::remember_lowered_cohort`) — they are the only objects that can hold an
    /// old→young edge no barrier recorded.
    ///
    /// `O(young set)`, not `O(heap)`: the young set is exact (every alive block younger than the
    /// old line, nothing else), so it holds the whole cohort. Outside generational mode there is no
    /// young set; every alive old object is returned instead.
    pub fn objects_past_lowered_line(&self) -> Vec<VarGcRef> {
        let threshold = self.promotion_age;
        let mut out = Vec::new();
        let mut visit = |h: VarGcRef, header: &GcBlockHeader| {
            if header.block_type() == BlockType::Object && header.gen_age() >= threshold && !header.is_remembered() {
                out.push(h);
            }
        };
        if self.generational {
            self.iterate_young(&mut visit);
        } else {
            self.iterate_alive(&mut visit);
        }
        out
    }

    /// Whether chunk `ci` is currently lent to a TLAB (its blocks are invisible to the region).
    pub fn is_chunk_borrowed(&self, ci: u32) -> bool {
        (ci as usize) < self.borrowed.len() && self.borrowed[ci as usize]
    }
}

/// A block laid out exactly like a region block — 16 B [`GcBlockHeader`], then the payload —
/// but owned by the caller instead of a region: never marked, swept or recycled; freed by
/// [`Drop`] (or leaked on purpose by [`DetachedBlock::leak`]). Object model R6 builds objects
/// that live outside the heap in it (stack-allocated objects, objects under construction), so
/// that `&ScriptObject` finds its header and field area at the same offsets wherever it lives.
///
/// The payload is zeroed. Dropping a `DetachedBlock` releases the memory only — whoever wrote a
/// non-POD payload drops it first.
pub struct DetachedBlock {
    header: NonNull<GcBlockHeader>,
    layout: Layout,
}

// SAFETY: the block is plain memory owned by this value; what lives in it is the owner's
// business (`OwnedObject` is `Send + Sync` because `ScriptObject` is).
unsafe impl Send for DetachedBlock {}
unsafe impl Sync for DetachedBlock {}

impl DetachedBlock {
    /// A fresh, zeroed block of `payload` bytes tagged `block_type` (alive, generation 0, no
    /// chunk — `chunk_idx == u32::MAX`).
    pub fn new(payload: usize, block_type: BlockType) -> Self {
        let (footprint, size_class) = class_for(payload);
        let layout = Layout::from_size_align(footprint, CHUNK_ALIGN).expect("detached block layout");
        // SAFETY: non-zero layout (footprint >= MIN_BLOCK).
        let raw = unsafe { alloc(layout) };
        let header = NonNull::new(raw as *mut GcBlockHeader).unwrap_or_else(|| handle_alloc_error(layout));
        // SAFETY: fresh 16-aligned allocation large enough for the header + `payload` bytes.
        unsafe {
            header.as_ptr().write(GcBlockHeader {
                generation: AtomicU32::new(0),
                size: payload as u32,
                marked: AtomicU8::new(0),
                alive: AtomicBool::new(true),
                type_tag: AtomicU8::new(GcBlockHeader::pack_tag(block_type, 0)),
                size_class,
                chunk_idx: u32::MAX,
            });
            std::ptr::write_bytes(payload_ptr_of(header), 0, payload);
        }
        Self { header, layout }
    }

    /// The payload's start (whole-allocation provenance).
    #[inline]
    pub fn payload_ptr(&self) -> *mut u8 {
        // SAFETY: the header heads this block's own allocation.
        unsafe { payload_ptr_of(self.header) }
    }

    /// A handle to the block, valid while `self` lives (no ownership transfer).
    #[cfg(test)]
    pub fn handle(&self) -> VarGcRef {
        VarGcRef::pack(self.header, 0)
    }

    /// The payload's byte length (from the block header).
    #[inline]
    pub fn payload_size(&self) -> usize {
        // SAFETY: the header heads this block's own allocation and was written in `new`.
        unsafe { self.header.as_ref() }.size()
    }

    /// Give the memory up for good and return a handle to it — a block no region knows about,
    /// alive forever (tests and heap-less fallbacks; see `VarGcRef::alloc_leaked`).
    pub fn leak(self) -> VarGcRef {
        let h = VarGcRef::pack(self.header, 0);
        std::mem::forget(self);
        h
    }
}

impl Drop for DetachedBlock {
    fn drop(&mut self) {
        // SAFETY: allocated in `new` with exactly this layout, freed once.
        unsafe { dealloc(self.header.as_ptr().cast(), self.layout) };
    }
}
