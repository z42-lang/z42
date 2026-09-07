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
//! - **Size classes**: total block size (header + payload) is rounded up to the next power
//!   of two (≥ `MIN_BLOCK` = 32). `free_lists[size_class]` recycles tombstoned slots of the
//!   same class.
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
pub(crate) use chunk::{class_for, OVERSIZED_CLASS};
pub use chunk::VarChunkClaim;
pub use var_ref::VarGcRef;

use chunk::{Chunk, NUM_CLASSES};

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
            live_count: 0,
            drop_glue: None,
            borrowed: Vec::new(),
            var_free_chunk_pool: Vec::new(),
            reuse_gen: Vec::new(),
        }
    }
}

impl VarRegion {
    pub fn new() -> Self {
        Self::default()
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
            live_count: 0,
            drop_glue: Some(glue),
            borrowed: Vec::new(),
            var_free_chunk_pool: Vec::new(),
            reuse_gen: Vec::new(),
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
        let header_ptr = if size_class == OVERSIZED_CLASS {
            self.alloc_dedicated(footprint)
        } else {
            self.bump(footprint)
        };
        self.write_fresh_header(header_ptr, payload, block_type, size_class, 0);
        self.all_blocks.push(header_ptr);
        self.live_count += 1;
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
        let generation = unsafe { slot.as_ref().generation() };
        self.write_fresh_header(slot, payload, block_type, size_class, generation);
        self.live_count += 1;
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
    ) {
        // SAFETY: `ptr` addresses freshly-carved (bump) or recycled (free-list) space large
        // enough for the header + `payload` bytes; we own exclusive access (`&mut self`).
        unsafe {
            ptr.as_ptr().write(GcBlockHeader {
                generation: AtomicU32::new(generation),
                size: payload as u32,
                marked: AtomicU8::new(0),
                alive: AtomicBool::new(true),
                type_tag: block_type as u8,
                size_class,
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
        header.generation.fetch_add(1, Ordering::AcqRel);
        self.live_count -= 1;
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

    /// Total chunk count (tests / diagnostics).
    #[cfg(test)]
    pub(crate) fn chunk_count(&self) -> usize {
        self.chunks.len()
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
            // SAFETY: each chunk was allocated with `chunk.layout()`; freed exactly once here.
            unsafe { dealloc(chunk.base.as_ptr(), chunk.layout()) }
        }
    }
}

#[cfg(test)]
#[path = "var_region_tests.rs"]
mod var_region_tests;
