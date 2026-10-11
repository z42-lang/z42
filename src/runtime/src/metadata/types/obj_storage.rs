//! `ObjStorage` — a managed object's field payload: one zero-initialised byte block.
//!
//! Every field cell lives in it (object model R1–R3): primitives at their natural width,
//! 8 B self-describing reference words (`ref_word`), 16 B type-parameter cells
//! (`tparam_cell`) and inline value structs whose reference leaves are reference words too.
//! There is no side table of 16 B `Value`s: a fresh block of zeros is already every field's
//! default (`0` / `false` / `'\0'` / `null`).
//!
//! The block is 8-aligned, so the composed layout's 8-aligned `i64` / `f64` / reference cells
//! are aligned. Cells are read and written through [`Cells`] / [`CellsMut`] (same-width atomic
//! accesses); [`ObjStorage::bytes`] is the plain byte view for whole-blob snapshots taken under
//! the object lock.
//!
//! # Safety
//!
//! Every `unsafe` for this representation lives in this file. `ObjStorage` owns its allocation
//! exclusively — exactly like a `Box<[u8]>` — so `Send` / `Sync` are sound.

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ptr::NonNull;

use super::{Cells, CellsMut};

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
        let mut this = Self::new(bytes.len());
        this.bytes_mut().copy_from_slice(bytes);
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

    /// The plain byte view (whole-blob snapshots under the object lock; field cells go through
    /// [`Self::cells`]).
    #[inline]
    pub fn bytes(&self) -> &[u8] {
        // SAFETY: the block holds `n_bytes` zero-initialised bytes for its whole life.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.n_bytes as usize) }
    }

    /// Mutable plain byte view — **primitive bytes only**: overwriting a reference word through
    /// it bypasses the SATB barrier. Fresh objects and the boxed-primitive path use it.
    #[inline]
    pub fn bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: see `bytes`; `&mut self` gives exclusive access.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.n_bytes as usize) }
    }

    /// Atomic loads of the field cells.
    #[inline(always)]
    pub fn cells(&self) -> Cells<'_> {
        // SAFETY: the block is 8-aligned, `n_bytes` long and owned by `self`; every concurrent
        // access to it is an atomic cell access (JIT inline loads included).
        unsafe { Cells::from_raw(self.ptr.as_ptr(), self.n_bytes as usize) }
    }

    /// Atomic loads and stores of the field cells.
    #[inline(always)]
    pub fn cells_mut(&mut self) -> CellsMut<'_> {
        // SAFETY: as `cells`, and `&mut self` is the exclusive borrow `CellsMut` asks for.
        unsafe { CellsMut::from_raw(self.ptr.as_ptr(), self.n_bytes as usize) }
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
// only through `&self` / `&mut self` views.
unsafe impl Send for ObjStorage {}
unsafe impl Sync for ObjStorage {}

impl std::fmt::Debug for ObjStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObjStorage").field("bytes", &self.bytes()).finish()
    }
}

#[cfg(test)]
#[path = "obj_storage_tests.rs"]
mod obj_storage_tests;
