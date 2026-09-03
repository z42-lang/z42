//! [`VarGcRef`] — the 8-byte, type-erased tagged handle to a variable-length GC block
//! (path-A tagged pointer, same scheme as `GcRef` in `refs.rs`).

use std::alloc::{alloc, handle_alloc_error, Layout};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8};

use super::block::{payload_ptr_of, BlockType, GcBlockHeader};
use super::chunk::{class_for, CHUNK_ALIGN};

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
    pub(super) fn pack(ptr: NonNull<GcBlockHeader>, generation: u32) -> Self {
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
    pub(super) fn header_ptr(&self) -> NonNull<GcBlockHeader> {
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
    pub(super) fn gen16(&self) -> u16 {
        (self.0.as_ptr().addr() >> ADDR_BITS) as u16
    }

    #[cfg(not(target_pointer_width = "64"))]
    #[inline]
    fn gen16(&self) -> u16 {
        self.generation as u16
    }

    /// The block's stable header address as a `usize` — a heap-identity key (e.g. for the
    /// retention reverse-reference graph or `ptr_eq`-style identity). Masks off the generation
    /// tag; unique per live block for its lifetime.
    #[inline]
    pub fn addr(&self) -> usize {
        self.header_ptr().as_ptr() as usize
    }

    /// True iff the block is still alive and its generation matches this handle (not
    /// tombstoned/reused). Reads only the 16-byte header (always chunk-mapped), so it's
    /// safe even when the payload was reused. unify-gc-heap PR-3 safety guard for array/
    /// string/closure block access.
    #[inline]
    pub fn is_live(&self) -> bool {
        let ptr = self.header_ptr();
        // SAFETY: header address is chunk-owned + mapped for the region's lifetime.
        let h = unsafe { ptr.as_ref() };
        h.is_alive() && h.generation() as u16 == self.gen16()
    }

    /// The block's requested payload byte length (from its header). unify-gc-heap PR-3:
    /// used to bounds-check typed slice views against the actual block size (`slice_of`).
    #[inline]
    pub fn payload_size(&self) -> usize {
        // SAFETY: header address is chunk-owned + mapped for the region's lifetime.
        unsafe { self.header_ptr().as_ref() }.size()
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

    /// Typed payload pointer `*mut T`, derived from the raw header pointer (whole-allocation
    /// provenance, D8). Does **not** check the generation guard — callers that already hold a
    /// live handle (mark/trace/access under the heap lock) use this for `T`-typed access when
    /// the block payload stores exactly one `T` (e.g. `ClosureData`). The payload was allocated
    /// with `size >= size_of::<T>()` and 8-aligned (`DATA_OFFSET = 16`, T's align ≤ 8).
    ///
    /// # Safety
    /// The block must be live and store a valid `T` at its payload; the region must outlive the
    /// pointer's use. For construction, write through this pointer *before* any typed read.
    #[inline]
    pub unsafe fn payload_as_ptr<T>(&self) -> *mut T {
        // SAFETY: whole-allocation provenance from the raw header; T fits the block payload.
        unsafe { payload_ptr_of(self.header_ptr()).cast::<T>() }
    }

    /// Test-only: allocate a **standalone, leaked** block holding one `T` (never in any
    /// `VarRegion`, never swept/freed). For unit tests that need a `Value::Closure`-style handle
    /// without wiring a heap. The intentional leak is fine for the tests that use it (they never
    /// run under Miri's leak checker — the Miri gate is `gc::var_region`).
    /// Test-only: allocate a **standalone, leaked**, zero-initialized block of `payload`
    /// bytes (never in any `VarRegion`, never swept/freed). For unit tests that need an
    /// array element block (`Boxed`/packed/`struct[]`) without wiring a heap. Callers write
    /// the payload through `payload_as_ptr` before reading. Same intentional-leak contract as
    /// [`leak_for_test`](Self::leak_for_test) (never run under Miri's leak checker).
    /// Allocate a **standalone, leaked** block of `payload` zero-initialized bytes,
    /// owned by no `VarRegion` (never marked/swept/freed). Backs the ambient-heap-less
    /// fallback of `Str::new` (unify-gc-heap PR-4): unit tests without a VM and any
    /// mock heap with no variable-length region. The leak is acceptable there — such
    /// contexts are process-scoped or short-lived (a test binary) and allocate a
    /// bounded set. **Production execution always has an active ambient heap**, so
    /// this path is never taken on a hot path. Callers write the payload through
    /// [`payload_as_ptr`](Self::payload_as_ptr) before reading it back.
    pub fn alloc_leaked(payload: usize, block_type: BlockType) -> Self {
        let (footprint, size_class) = class_for(payload);
        let layout = Layout::from_size_align(footprint, CHUNK_ALIGN).expect("leak block layout");
        // SAFETY: non-zero layout (footprint >= MIN_BLOCK); leaked (never freed).
        let raw = unsafe { alloc(layout) };
        let header = NonNull::new(raw as *mut GcBlockHeader).unwrap_or_else(|| handle_alloc_error(layout));
        // SAFETY: fresh 8-aligned allocation large enough for the header + `payload` bytes.
        unsafe {
            header.as_ptr().write(GcBlockHeader {
                generation: AtomicU32::new(0),
                size: payload as u32,
                marked: AtomicU8::new(0),
                alive: AtomicBool::new(true),
                type_tag: block_type as u8,
                size_class,
            });
            std::ptr::write_bytes(payload_ptr_of(header), 0, payload);
        }
        VarGcRef::pack(header, 0)
    }

    #[cfg(test)]
    pub(crate) fn leak_block_for_test(payload: usize, block_type: BlockType) -> Self {
        Self::alloc_leaked(payload, block_type)
    }

    #[cfg(test)]
    pub(crate) fn leak_for_test<T>(value: T, block_type: BlockType) -> Self {
        let payload = std::mem::size_of::<T>();
        let (footprint, size_class) = class_for(payload);
        let layout = Layout::from_size_align(footprint, CHUNK_ALIGN).expect("leak layout");
        // SAFETY: non-zero layout; leaked (never freed) — acceptable for test fixtures.
        let raw = unsafe { alloc(layout) };
        let header = NonNull::new(raw as *mut GcBlockHeader).unwrap_or_else(|| handle_alloc_error(layout));
        // SAFETY: fresh 8-aligned allocation large enough for the header + one `T`.
        unsafe {
            header.as_ptr().write(GcBlockHeader {
                generation: AtomicU32::new(0),
                size: payload as u32,
                marked: AtomicU8::new(0),
                alive: AtomicBool::new(true),
                type_tag: block_type as u8,
                size_class,
            });
            payload_ptr_of(header).cast::<T>().write(value);
        }
        VarGcRef::pack(header, 0)
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
