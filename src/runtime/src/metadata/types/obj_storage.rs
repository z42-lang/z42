//! `ObjStorage` — a managed object's field payload: one zero-initialised byte block.
//!
//! Every field cell lives in it (object model R1–R3): primitives at their natural width,
//! 8 B self-describing reference words (`ref_word`), 16 B type-parameter cells
//! (`tparam_cell`) and inline value structs whose reference leaves are reference words too.
//! There is no side table of 16 B `Value`s: a fresh block of zeros is already every field's
//! default (`0` / `false` / `'\0'` / `null`).
//!
//! The block is 8-aligned, so the composed layout's 8-aligned `i64` / `f64` / reference cells
//! are aligned. Every access after construction goes through [`Cells`] (same-width atomic loads
//! and stores, `&self`; whole-blob snapshots are atomic chunked copies). There is no plain byte
//! view: objects are shared between threads without a lock (object model R4), so a plain slice
//! over the block would race with concurrent cell stores.
//!
//! # Safety
//!
//! Every `unsafe` for this representation lives in this file. `ObjStorage` owns its allocation
//! exclusively — exactly like a `Box<[u8]>` — so `Send` / `Sync` are sound.

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ptr::NonNull;

use super::Cells;

/// Alignment of the block: 8, for the composed layout's 8-aligned cells.
const BLOCK_ALIGN: usize = 8;

/// One managed object's byte payload.
pub struct ObjStorage {
    /// Block start, or an **aligned** dangling pointer when the object has no fields at all (a
    /// field-less class allocates nothing).
    ptr: NonNull<u8>,
    n_bytes: u32,
}

impl ObjStorage {
    /// Allocate a fresh, zeroed payload of `n_bytes` (zero = every field's default).
    pub fn new(n_bytes: usize) -> Self {
        assert!(n_bytes <= u32::MAX as usize, "object payload too large: {n_bytes} bytes");
        let Some(layout) = Self::layout_of(n_bytes) else {
            return Self { ptr: Self::aligned_dangling(), n_bytes: 0 };
        };
        // SAFETY: layout has non-zero size (`layout_of` returns None otherwise).
        let raw = unsafe { alloc_zeroed(layout) };
        let Some(ptr) = NonNull::new(raw) else { std::alloc::handle_alloc_error(layout) };
        Self { ptr, n_bytes: n_bytes as u32 }
    }

    /// Wrap a pre-filled byte payload — the boxed-primitive path
    /// (`corelib::convert::box_prim_to_heap`), whose scalar bytes are already laid out by the
    /// caller.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let this = Self::new(bytes.len());
        if !bytes.is_empty() {
            // SAFETY: a fresh block of exactly `bytes.len()` bytes that nothing else can see yet
            // (construction, before the object is published); `bytes` is a distinct allocation.
            unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), this.ptr.as_ptr(), bytes.len()) };
        }
        this
    }

    /// A non-null, `BLOCK_ALIGN`-aligned pointer that is never dereferenced — the
    /// empty-payload stand-in.
    #[inline]
    fn aligned_dangling() -> NonNull<u8> {
        // SAFETY: `BLOCK_ALIGN` is a non-zero power of two.
        unsafe { NonNull::new_unchecked(BLOCK_ALIGN as *mut u8) }
    }

    /// `None` when the object has no payload at all (nothing to allocate).
    fn layout_of(n_bytes: usize) -> Option<Layout> {
        if n_bytes == 0 {
            return None;
        }
        Some(Layout::from_size_align(n_bytes, BLOCK_ALIGN).expect("object payload layout"))
    }

    /// Bytes this payload asked the allocator for (`0` for a field-less object, which allocates
    /// nothing) — the GC's footprint accounting rounds it to a size class.
    #[inline]
    pub fn alloc_bytes(&self) -> usize {
        self.n_bytes as usize
    }

    /// Payload length in bytes (the composed layout's `bytes_len`, or a boxed primitive's width).
    #[inline]
    pub fn len(&self) -> usize {
        self.n_bytes as usize
    }

    /// Whether the object has no payload at all.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.n_bytes == 0
    }

    /// Atomic loads and stores of the field cells.
    #[inline(always)]
    pub fn cells(&self) -> Cells<'_> {
        // SAFETY: the block is 8-aligned, `n_bytes` long and owned by `self`; every access to it
        // after construction is an atomic cell access (JIT inline loads / stores are plain
        // same-width accesses, the relaxed atomics of x86-64 / AArch64).
        unsafe { Cells::from_raw(self.ptr.as_ptr(), self.n_bytes as usize) }
    }

    /// An atomic snapshot of the whole payload ([`Cells::copy_out`]).
    pub fn snapshot(&self) -> Vec<u8> {
        let mut out = vec![0u8; self.len()];
        self.cells().copy_out(0, &mut out);
        out
    }
}

impl Drop for ObjStorage {
    fn drop(&mut self) {
        let Some(layout) = Self::layout_of(self.n_bytes as usize) else { return };
        // SAFETY: `ptr` came from `alloc_zeroed` with this exact layout.
        unsafe { dealloc(self.ptr.as_ptr(), layout) };
    }
}

// SAFETY: `ObjStorage` owns its allocation exclusively (like a `Box<[u8]>`) and hands it out
// only through atomic [`Cells`] views, so shared access from several threads is race-free.
unsafe impl Send for ObjStorage {}
unsafe impl Sync for ObjStorage {}

impl std::fmt::Debug for ObjStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObjStorage").field("len", &self.n_bytes).finish()
    }
}

#[cfg(test)]
#[path = "obj_storage_tests.rs"]
mod obj_storage_tests;
