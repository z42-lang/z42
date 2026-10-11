//! Chunk storage for `Region<T>`: chunks carved back to back out of page-aligned slabs.
//!
//! The point is **page-granular decommit** (`region/decommit.rs`): a chunk is
//! `CHUNK_SIZE × size_of::<RegionEntry<T>>()` bytes — 16 384 for objects, 24 576 for arrays —
//! which in general is not a whole number of 16 KB pages (the slot size follows `T`). Boxed one by one at allocator-chosen addresses,
//! most chunks contain no complete page at all, so handing an empty one back to the OS freed
//! nothing. In a slab, chunk `ci` sits at a known offset, and a run of adjacent empty chunks
//! gives back every page inside the run.
//!
//! A slab is a whole number of pages for any entry size that is a multiple of 8: one chunk per
//! 8-byte entry stride is `256 × 8 = 2 048` bytes, and `CHUNKS_PER_SLAB` of them is a multiple
//! of 16 KB (the largest page size z42 runs on). Pages of a slab no chunk has been carved from
//! yet are never touched, so (with demand paging) they cost address space, not RSS. (Measured
//! on its own the layout is neutral: 1M small objects 140.2 → 138.9 MB RSS.)
//!
//! Chunks are carved in index order and never moved or unmapped before the region drops, so
//! chunk `ci` is slab `ci / CHUNKS_PER_SLAB`, slot `ci % CHUNKS_PER_SLAB`. Weak references rely
//! on the memory staying mapped (`refs.rs`, `WeakGcRef::upgrade` reads a dead slot's header);
//! decommitted pages stay mapped and read as zeroes or their old bytes.

use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;

use super::{RegionEntry, CHUNK_SIZE};
use crate::gc::os_mem;

/// Chunks per slab. 32 keeps the mapping count low (a 1 GB object region is ~1 800 slabs — far
/// under Linux's default `vm.max_map_count`) while an untouched tail stays free.
pub(crate) const CHUNKS_PER_SLAB: usize = 32;

/// The slot array of one chunk.
pub(crate) type ChunkSlots<T> = [MaybeUninit<RegionEntry<T>>; CHUNK_SIZE];

/// Bytes of one chunk of `T` entries.
#[inline]
pub(crate) const fn chunk_bytes<T>() -> usize {
    std::mem::size_of::<ChunkSlots<T>>()
}

/// A chunk's slot array, owned by the region's [`Slabs`]. Derefs to the array, so
/// `chunks[ci][ei]` reads exactly as it did when chunks were boxed.
pub(crate) struct ChunkPtr<T>(NonNull<ChunkSlots<T>>);

impl<T> Deref for ChunkPtr<T> {
    type Target = ChunkSlots<T>;
    #[inline]
    fn deref(&self) -> &ChunkSlots<T> {
        // SAFETY: carved from a live slab, which outlives every `ChunkPtr` (both are owned by
        // the same `Region`, and slabs are unmapped only when it drops).
        unsafe { self.0.as_ref() }
    }
}

impl<T> DerefMut for ChunkPtr<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut ChunkSlots<T> {
        // SAFETY: as `deref`; `&mut self` on the owning `Region` gives exclusive access.
        unsafe { self.0.as_mut() }
    }
}

// SAFETY: a `ChunkPtr` is the unique owner of its chunk's memory, exactly like the
// `Box<[MaybeUninit<RegionEntry<T>>; CHUNK_SIZE]>` it stands for.
unsafe impl<T: Send> Send for ChunkPtr<T> {}
unsafe impl<T: Sync> Sync for ChunkPtr<T> {}

/// The slabs a region's chunks are carved from.
pub(crate) struct Slabs<T> {
    /// Base of every slab, oldest first. All are `CHUNKS_PER_SLAB × chunk_bytes::<T>()` bytes.
    bases: Vec<NonNull<u8>>,
    /// Chunks already carved out of the last slab.
    carved: usize,
    _phantom: PhantomData<T>,
}

impl<T> Default for Slabs<T> {
    fn default() -> Self {
        Self { bases: Vec::new(), carved: CHUNKS_PER_SLAB, _phantom: PhantomData }
    }
}

impl<T> Slabs<T> {
    const SLAB_BYTES: usize = CHUNKS_PER_SLAB * chunk_bytes::<T>();

    /// Address range `[start, end)` of chunks `first..=last`, which must share a slab.
    pub(crate) fn span(&self, first: usize, last: usize) -> (usize, usize) {
        debug_assert_eq!(first / CHUNKS_PER_SLAB, last / CHUNKS_PER_SLAB, "a span stays in one slab");
        let base = self.bases[first / CHUNKS_PER_SLAB].as_ptr() as usize;
        let cb = chunk_bytes::<T>();
        (base + first % CHUNKS_PER_SLAB * cb, base + (last % CHUNKS_PER_SLAB + 1) * cb)
    }

    /// Cut the next chunk, mapping a fresh slab when the last one is used up. The memory is
    /// uninitialized as far as the region is concerned (it reads `initialized` before any slot).
    pub(crate) fn carve(&mut self) -> ChunkPtr<T> {
        if self.carved == CHUNKS_PER_SLAB {
            self.bases.push(os_mem::alloc_pages(Self::SLAB_BYTES));
            self.carved = 0;
        }
        let base = *self.bases.last().expect("a slab was just mapped");
        // SAFETY: `carved < CHUNKS_PER_SLAB`, so the chunk lies inside the slab.
        let p = unsafe { base.as_ptr().add(self.carved * chunk_bytes::<T>()) };
        self.carved += 1;
        ChunkPtr(NonNull::new(p.cast()).expect("slab base is non-null"))
    }
}

impl<T> Drop for Slabs<T> {
    fn drop(&mut self) {
        for &base in &self.bases {
            // SAFETY: each base came from `alloc_pages(SLAB_BYTES)`; the owning region dropped
            // every entry in it before its fields (this one included) are dropped.
            unsafe { os_mem::free_pages(base, Self::SLAB_BYTES) };
        }
    }
}

// SAFETY: the slabs are plain owned memory; access is mediated by the owning `Region`.
unsafe impl<T: Send> Send for Slabs<T> {}
unsafe impl<T: Sync> Sync for Slabs<T> {}

const _: () = assert!(
    (CHUNKS_PER_SLAB * CHUNK_SIZE * 8) % (16 * 1024) == 0,
    "a slab must be a whole number of 16K pages for every 8-byte-multiple entry size"
);
