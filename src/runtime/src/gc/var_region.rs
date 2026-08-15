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

use std::alloc::{alloc, dealloc, handle_alloc_error, Layout};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

/// Payload kind of a variable-length GC block. Since a `VarRegion` mixes payload types in
/// one allocator, the block header records which kind it holds so the GC tracer knows how to
/// scan the payload (leaf bytes vs. inline `Value`s vs. closure fields). PR-1 only tags them;
/// the actual per-kind tracing lands with each payload migration (PR-2…PR-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BlockType {
    /// UTF-8 string bytes (immutable leaf — no outgoing references). PR-2.
    Str = 0,
    /// `[Value; n]` reference-array elements (each element is a traced edge). PR-4.
    ArrayValue = 1,
    /// Packed primitive array bytes (leaf — no references). PR-4.
    ArrayPrim = 2,
    /// `struct[]` inline bytes + reference-leaf bitmap (mixed). PR-4.
    ArrayStruct = 3,
    /// `ClosureData` fields (env edge + fn_name string edge). PR-3.
    Closure = 4,
}

impl BlockType {
    /// Reconstruct from the raw `u8` stored in a header. Returns `None` on an unknown tag
    /// (corruption guard — a valid block always carries one of the variants above).
    #[inline]
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Str),
            1 => Some(Self::ArrayValue),
            2 => Some(Self::ArrayPrim),
            3 => Some(Self::ArrayStruct),
            4 => Some(Self::Closure),
            _ => None,
        }
    }
}

/// Fixed header preceding a variable-length block's inline payload. `#[repr(C, align(8))]`
/// pins the field order and pads to 16 bytes so the payload always starts 8-aligned at
/// `DATA_OFFSET` (a `Value` element needs 8-alignment).
#[repr(C, align(8))]
pub struct GcBlockHeader {
    /// Generation counter (ABA guard). Bumped on every tombstone. A [`VarGcRef`] snapshots
    /// the low 16 bits at construction; a mismatch on resolve means the slot was reclaimed +
    /// reused → stale handle. Mirrors `RegionEntry::generation`.
    generation: AtomicU32,
    /// Payload byte length (immutable after alloc). Note this is the *requested* payload
    /// size; the slot's physical capacity is `size_class`'s power-of-two footprint, which
    /// may be larger.
    size: u32,
    /// Mark bit (0 = unmarked). CAS 0→1 by the mark phase; reset by sweep on survivors.
    marked: AtomicU8,
    /// Tombstone flag: `true` while live, `false` after sweep reclaims the slot.
    alive: AtomicBool,
    /// Payload kind ([`BlockType`] as `u8`) — tells the tracer how to scan the payload.
    type_tag: u8,
    /// Size-class index (`log2(total_footprint)`), or [`OVERSIZED_CLASS`] for a dedicated
    /// chunk. Lets tombstone return the slot to the right free list and lets iteration know
    /// the slot's footprint.
    size_class: u8,
}

// The header is exactly 16 bytes so the inline payload begins 8-aligned. This mirrors
// `vstr::StrHeader` (also 16 B) — deliberately, so a GC string block and the current
// thin-Arc string have identical payload offsets, easing the PR-2 migration.
const _: () = assert!(std::mem::size_of::<GcBlockHeader>() == 16);
const _: () = assert!(std::mem::align_of::<GcBlockHeader>() == 8);

impl GcBlockHeader {
    /// Byte offset of the inline payload within the allocation = the (padded) header size.
    pub const DATA_OFFSET: usize = std::mem::size_of::<GcBlockHeader>();

    /// Payload byte length (as requested at alloc).
    #[inline]
    pub fn size(&self) -> usize {
        self.size as usize
    }

    /// Payload kind.
    #[inline]
    pub fn block_type(&self) -> BlockType {
        // A live block always carries a valid tag (set at alloc); fall back to `Str` only to
        // avoid a panic on a corrupted read (debug builds assert instead).
        debug_assert!(BlockType::from_u8(self.type_tag).is_some(), "corrupt block type_tag");
        BlockType::from_u8(self.type_tag).unwrap_or(BlockType::Str)
    }

    /// True while the block is live (not yet swept).
    #[inline]
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    /// Attempt to mark this block (0 → 1). Returns `true` if this call won the CAS.
    #[inline]
    pub fn mark(&self) -> bool {
        self.marked
            .compare_exchange(0, 1, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    }

    /// Read the mark bit.
    #[inline]
    pub fn is_marked(&self) -> bool {
        self.marked.load(Ordering::Relaxed) != 0
    }

    /// Reset the mark bit (sweep on survivors).
    #[inline]
    pub fn clear_mark(&self) {
        self.marked.store(0, Ordering::Relaxed);
    }

    /// Current generation (full 32 bits — for tests / the handle guard).
    #[inline]
    fn generation(&self) -> u32 {
        self.generation.load(Ordering::Acquire)
    }
}

/// Raw pointer to a block's inline payload bytes, derived from the **header pointer**
/// (whole-allocation provenance) — deliberately NOT from a `&GcBlockHeader` reference.
///
/// A `&GcBlockHeader` reborrow narrows provenance to the 16-byte header, so deriving the
/// payload pointer (at offset 16) through it and then accessing the payload is out-of-bounds
/// under Stacked Borrows (Miri UB — caught during PR-1). The header + payload are one
/// allocation, and `header` still carries the chunk-allocation provenance, so `.add(16)`
/// stays in-bounds of that allocation.
///
/// # Safety
/// `header` must point at a live block whose backing chunk outlives the access; the payload
/// is valid for the block's `size` bytes.
#[inline]
unsafe fn payload_ptr_of(header: NonNull<GcBlockHeader>) -> *mut u8 {
    // SAFETY: the payload occupies `[DATA_OFFSET, DATA_OFFSET + size)` in the same allocation
    // as the header; deriving from the raw header pointer keeps whole-allocation provenance.
    unsafe { header.as_ptr().cast::<u8>().add(GcBlockHeader::DATA_OFFSET) }
}

/// `size_class` sentinel for a block that exceeds the largest in-chunk class and got its own
/// dedicated, exactly-sized chunk.
const OVERSIZED_CLASS: u8 = u8::MAX;

/// Smallest total block footprint (header + payload), a power of two. 32 = 16 B header + up
/// to 16 B payload.
const MIN_BLOCK: usize = 32;

/// Byte capacity of a bump chunk (payloads larger than this get a dedicated chunk).
const CHUNK_BYTES: usize = 64 * 1024;

/// Chunk alignment — 16 so every block start (bumped to 8) and the header (align 8) are
/// satisfied with margin.
const CHUNK_ALIGN: usize = 16;

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
fn class_for(payload: usize) -> (usize, u8) {
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
const NUM_CLASSES: usize = MAX_CLASS as usize + 1;

/// A raw, owned chunk of GC block memory. Freed in [`VarRegion::drop`].
struct Chunk {
    /// 16-aligned base pointer from the global allocator.
    base: NonNull<u8>,
    /// Total byte capacity of this chunk (`CHUNK_BYTES` for bump chunks, exact size for
    /// dedicated oversized chunks).
    cap: usize,
}

impl Chunk {
    /// Allocate a fresh `cap`-byte, 16-aligned chunk. Aborts on OOM (a partly-built region
    /// can't recover a null chunk).
    fn new(cap: usize) -> Self {
        let layout = Layout::from_size_align(cap, CHUNK_ALIGN).expect("chunk layout");
        // SAFETY: `cap` is non-zero (>= MIN_BLOCK). On OOM abort rather than return a
        // dangling base.
        let raw = unsafe { alloc(layout) };
        let base = NonNull::new(raw).unwrap_or_else(|| handle_alloc_error(layout));
        Chunk { base, cap }
    }

    /// The `Layout` this chunk was allocated with (for `dealloc`).
    #[inline]
    fn layout(&self) -> Layout {
        Layout::from_size_align(self.cap, CHUNK_ALIGN).expect("chunk layout")
    }
}

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
        }
    }
}

impl VarRegion {
    pub fn new() -> Self {
        Self::default()
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

    /// Bump-allocate `footprint` bytes (already a power of two ≤ CHUNK_BYTES) from the
    /// current chunk, growing a new chunk when it doesn't fit. Returns the block header ptr.
    fn bump(&mut self, footprint: usize) -> NonNull<GcBlockHeader> {
        let need_new = match self.bump_chunk {
            None => true,
            Some(ci) => self.bump_off + footprint > self.chunks[ci].cap,
        };
        if need_new {
            self.chunks.push(Chunk::new(CHUNK_BYTES));
            self.bump_chunk = Some(self.chunks.len() - 1);
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
    fn alloc_dedicated(&mut self, footprint: usize) -> NonNull<GcBlockHeader> {
        self.chunks.push(Chunk::new(footprint));
        let base = self.chunks.last().expect("just pushed").base;
        // SAFETY: chunk base is 16-aligned (≥ header align 8) and non-null.
        unsafe { NonNull::new_unchecked(base.as_ptr() as *mut GcBlockHeader) }
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
    /// the number of blocks reclaimed. (v1 = STW only; generational minor sweep is a later
    /// PR.)
    pub fn sweep(&mut self) -> usize {
        let mut reclaimed = 0;
        // Collect the slots to reclaim first (can't tombstone while borrowing all_blocks).
        let mut to_reclaim: Vec<VarGcRef> = Vec::new();
        for &ptr in &self.all_blocks {
            // SAFETY: see `iterate_alive`.
            let header = unsafe { ptr.as_ref() };
            if !header.is_alive() {
                continue;
            }
            if header.is_marked() {
                header.clear_mark();
            } else {
                to_reclaim.push(VarGcRef::pack(ptr, header.generation()));
            }
        }
        for h in to_reclaim {
            if self.tombstone(h) {
                reclaimed += 1;
            }
        }
        reclaimed
    }

    /// Number of live blocks (diagnostics).
    #[inline]
    pub fn live_count(&self) -> usize {
        self.live_count
    }

    /// Total chunk count (tests / diagnostics).
    #[cfg(test)]
    fn chunk_count(&self) -> usize {
        self.chunks.len()
    }
}

impl Drop for VarRegion {
    /// Free every owned chunk. Payloads are POD bytes in PR-1 (no per-element Drop);
    /// consumer PRs that store `Value`s inline will add payload finalization here.
    fn drop(&mut self) {
        for chunk in &self.chunks {
            // SAFETY: each chunk was allocated with `chunk.layout()`; freed exactly once here.
            unsafe { dealloc(chunk.base.as_ptr(), chunk.layout()) }
        }
    }
}

// ── VarGcRef: 8-byte type-erased tagged handle ──────────────────────────────────────────

/// Mask isolating the 48-bit header address from a tagged handle (64-bit targets).
#[cfg(target_pointer_width = "64")]
const ADDR_BITS: u32 = 48;
#[cfg(target_pointer_width = "64")]
const ADDR_MASK: usize = (1usize << ADDR_BITS) - 1;

/// An 8-byte, type-erased handle to a variable-length GC block. The low 48 bits are the
/// [`GcBlockHeader`] address, the high 16 bits are a narrow generation snapshot (path-A
/// tagged pointer, same scheme as `GcRef` in `refs.rs`). "Type-erased" because a `VarRegion`
/// mixes payload kinds — the block's `type_tag` (not a static `T`) discriminates.
///
/// On 32-bit targets there are no spare high bits, so the generation rides in a separate
/// `u32` field; the handle is still 8 bytes (4 B pointer + 4 B generation).
#[cfg(target_pointer_width = "64")]
#[derive(Clone, Copy)]
pub struct VarGcRef(NonNull<GcBlockHeader>);

#[cfg(not(target_pointer_width = "64"))]
#[derive(Clone, Copy)]
pub struct VarGcRef {
    ptr: NonNull<GcBlockHeader>,
    generation: u32,
}

// The whole point: one machine word, and `Option<VarGcRef>` stays 8 B via the NonNull niche
// (so a nullable inline reference costs no extra tag byte downstream).
const _: () = assert!(std::mem::size_of::<VarGcRef>() == 8);
const _: () = assert!(std::mem::size_of::<Option<VarGcRef>>() == 8);

// SAFETY: the block memory is immutable-address + atomically ref-counted-by-GC and lives in
// a `Mutex`-guarded region; a handle is safe to send/share exactly like `GcRef`.
unsafe impl Send for VarGcRef {}
unsafe impl Sync for VarGcRef {}

impl VarGcRef {
    /// Fold a clean header pointer + generation into the 8-byte handle.
    #[cfg(target_pointer_width = "64")]
    #[inline]
    fn pack(ptr: NonNull<GcBlockHeader>, generation: u32) -> Self {
        let gen16 = generation as u16 as usize;
        // strict-provenance `map_addr`: the tag lives only in never-dereferenced high bits,
        // so the result keeps `ptr`'s provenance (Miri-clean).
        let tagged = ptr
            .as_ptr()
            .map_addr(|a| (a & ADDR_MASK) | (gen16 << ADDR_BITS));
        // SAFETY: the low 48 bits come from a valid non-null header pointer → non-zero.
        Self(unsafe { NonNull::new_unchecked(tagged) })
    }

    #[cfg(not(target_pointer_width = "64"))]
    #[inline]
    fn pack(ptr: NonNull<GcBlockHeader>, generation: u32) -> Self {
        Self { ptr, generation }
    }

    /// The backing header pointer with the tag masked off.
    #[cfg(target_pointer_width = "64")]
    #[inline]
    fn header_ptr(&self) -> NonNull<GcBlockHeader> {
        let clean = self.0.as_ptr().map_addr(|a| a & ADDR_MASK);
        // SAFETY: masking the tag off a valid tagged pointer yields the original non-null
        // header address.
        unsafe { NonNull::new_unchecked(clean) }
    }

    #[cfg(not(target_pointer_width = "64"))]
    #[inline]
    fn header_ptr(&self) -> NonNull<GcBlockHeader> {
        self.ptr
    }

    /// The 16-bit generation snapshot (matches the `header.generation as u16` guard).
    #[cfg(target_pointer_width = "64")]
    #[inline]
    fn gen16(&self) -> u16 {
        (self.0.as_ptr().addr() >> ADDR_BITS) as u16
    }

    #[cfg(not(target_pointer_width = "64"))]
    #[inline]
    fn gen16(&self) -> u16 {
        self.generation as u16
    }

    /// Mark the pointed-to block (mark phase). Returns `true` if this call won the CAS.
    ///
    /// # Safety
    /// The handle must be live (from a region that still outlives it). Callers in the mark
    /// phase hold the region alive, so this is the mark-phase fast path (no generation check —
    /// a marked-but-stale block is harmless: it'll be swept anyway).
    #[inline]
    pub fn mark(&self) -> bool {
        // SAFETY: mark phase holds the region; the header address is valid.
        unsafe { self.header_ptr().as_ref().mark() }
    }

    /// Identity equality: two handles are equal iff they name the same block *and* generation
    /// (the tagged word covers both on 64-bit).
    #[cfg(target_pointer_width = "64")]
    #[inline]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }

    #[cfg(not(target_pointer_width = "64"))]
    #[inline]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        self.ptr == other.ptr && self.generation == other.generation
    }

    /// Raw payload bytes (shared). Returns `None` if the handle is stale/dead.
    ///
    /// # Safety
    /// The backing region must outlive the returned slice; the caller holds the region
    /// (typically via the heap mutex) for the borrow's duration.
    pub unsafe fn payload(&self) -> Option<&[u8]> {
        let ptr = self.header_ptr();
        // Read the guard metadata through a short-lived shared ref over the header only.
        // SAFETY: caller guarantees the region outlives the borrow.
        let (len, gen, alive) = {
            let header = unsafe { ptr.as_ref() };
            (header.size(), header.generation(), header.is_alive())
        };
        if gen as u16 != self.gen16() || !alive {
            return None;
        }
        // Derive the payload pointer from the raw header pointer (whole-allocation provenance).
        // SAFETY: `len` initialized bytes at DATA_OFFSET, immutable for the borrow.
        let data = unsafe { payload_ptr_of(ptr) };
        unsafe { Some(std::slice::from_raw_parts(data, len)) }
    }

    /// Raw payload bytes (exclusive). Returns `None` if stale/dead.
    ///
    /// # Safety
    /// Same as [`payload`](Self::payload); additionally the caller must hold exclusive access
    /// to the region (the heap mutex) so no other handle aliases these bytes.
    pub unsafe fn payload_mut(&self) -> Option<&mut [u8]> {
        let ptr = self.header_ptr();
        // SAFETY: caller guarantees exclusive region access for the borrow.
        let (len, gen, alive) = {
            let header = unsafe { ptr.as_ref() };
            (header.size(), header.generation(), header.is_alive())
        };
        if gen as u16 != self.gen16() || !alive {
            return None;
        }
        // Derive the payload pointer from the raw header pointer (whole-allocation provenance),
        // NOT through the shared ref above (which is dropped) — that would be OOB under SB.
        // SAFETY: exclusive access + valid `len`-byte payload region.
        let data = unsafe { payload_ptr_of(ptr) };
        unsafe { Some(std::slice::from_raw_parts_mut(data, len)) }
    }
}

impl std::fmt::Debug for VarGcRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VarGcRef")
            .field("addr", &self.header_ptr().as_ptr())
            .field("gen16", &self.gen16())
            .finish()
    }
}

#[cfg(test)]
#[path = "var_region_tests.rs"]
mod var_region_tests;
