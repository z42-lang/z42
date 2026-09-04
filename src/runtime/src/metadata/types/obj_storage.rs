//! `ObjStorage` — a managed object's field payload in **one** allocation.
//!
//! shrink-object-footprint P2: `ScriptObject` used to hold two boxed slices —
//! `bytes: Box<[u8]>` (primitive leaves) and `refs: Box<[Value]>` (reference
//! leaves) — so every `new` paid **two** mallocs plus two 16-byte fat pointers
//! plus two allocator headers. Merging them costs one `unsafe` type and buys a
//! malloc, 16 bytes of `ScriptObject`, and one allocator header per object.
//!
//! # Layout
//!
//! ```text
//!   ┌────────────────────────┬──────────────────────┐
//!   │ refs: [Value; n_refs]  │ bytes: [u8; n_bytes] │
//!   └────────────────────────┴──────────────────────┘
//!     ↑ block start, 8-aligned   ↑ n_refs * 16 — still 8-aligned
//! ```
//!
//! References come **first** for alignment: `Value` needs 8-byte alignment and
//! `size_of::<Value>()` is a multiple of 8, so the byte region's start stays
//! 8-aligned no matter how many reference leaves there are. That matters
//! because the composed object layout places `i64` / `f64` leaves at 8-aligned
//! byte offsets.
//!
//! # Safety
//!
//! Every `unsafe` for this representation lives in this file. `ObjStorage` owns
//! its allocation exclusively — exactly like the `Box`es it replaces — and all
//! access goes through `&self` / `&mut self` slice accessors, so `Send`/`Sync`
//! follow from `Value: Send + Sync`.

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ptr::NonNull;

use super::Value;

/// Alignment of the whole block: `Value`'s alignment, which also keeps the
/// byte region 8-aligned (see the module docs).
const BLOCK_ALIGN: usize = std::mem::align_of::<Value>();

/// One managed object's `[refs][bytes]` payload block.
pub struct ObjStorage {
    /// Block start, or an **aligned** dangling pointer when the object has no
    /// fields at all (a field-less class allocates nothing — same as the old
    /// `Box::from([])`). Aligned, not `NonNull::<u8>::dangling()`: that is
    /// 1-aligned, and `slice::from_raw_parts::<Value>` demands alignment even
    /// for a zero-length slice (caught by `empty_storage_…` under the
    /// debug-assertion UB check).
    ptr: NonNull<u8>,
    n_refs: u32,
    n_bytes: u32,
}

impl ObjStorage {
    /// Allocate a fresh, **default-initialised** payload: primitive bytes zeroed
    /// (zero = every primitive field's default: `0` / `false` / `'\0'`) and every
    /// reference leaf set to `Value::Null`.
    ///
    /// The reference slots are written one by one rather than relying on the
    /// zeroed block: `Value::Null`'s in-memory representation is not guaranteed
    /// to be all-zero, and depending on it would be a silent trap the day the
    /// enum's layout changes.
    pub fn new(n_bytes: usize, n_refs: usize) -> Self {
        assert!(n_bytes <= u32::MAX as usize && n_refs <= u32::MAX as usize,
                "object payload too large: {n_bytes} bytes / {n_refs} refs");
        let Some(layout) = Self::layout_of(n_bytes, n_refs) else {
            return Self { ptr: Self::aligned_dangling(), n_refs: 0, n_bytes: 0 };
        };
        // SAFETY: layout has non-zero size (`layout_of` returns None otherwise).
        let raw = unsafe { alloc_zeroed(layout) };
        let Some(ptr) = NonNull::new(raw) else { std::alloc::handle_alloc_error(layout) };
        let mut this = Self { ptr, n_refs: n_refs as u32, n_bytes: n_bytes as u32 };
        for i in 0..n_refs {
            // SAFETY: `i < n_refs`, the slot is inside the block, and it is
            // uninitialised memory we are initialising exactly once.
            unsafe { this.ref_slot(i).write(Value::Null) };
        }
        this
    }

    /// Wrap a pre-filled byte payload with no reference leaves — the boxed-primitive
    /// path (`corelib::convert::box_prim_to_heap`), whose scalar bytes are already
    /// laid out by the caller.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut this = Self::new(bytes.len(), 0);
        this.bytes_mut().copy_from_slice(bytes);
        this
    }

    /// A non-null, `BLOCK_ALIGN`-aligned pointer that is never dereferenced —
    /// the empty-payload stand-in.
    #[inline]
    fn aligned_dangling() -> NonNull<u8> {
        // SAFETY: `BLOCK_ALIGN` is a non-zero power of two, so the value is
        // non-null and correctly aligned for both regions (both are empty).
        unsafe { NonNull::new_unchecked(BLOCK_ALIGN as *mut u8) }
    }

    /// `None` when the object has no payload at all (nothing to allocate).
    fn layout_of(n_bytes: usize, n_refs: usize) -> Option<Layout> {
        let size = n_refs * std::mem::size_of::<Value>() + n_bytes;
        if size == 0 {
            return None;
        }
        Some(Layout::from_size_align(size, BLOCK_ALIGN).expect("object payload layout"))
    }

    /// SAFETY: caller guarantees `i < self.n_refs`; used only to initialise a
    /// fresh block.
    unsafe fn ref_slot(&mut self, i: usize) -> *mut Value {
        self.ptr.as_ptr().cast::<Value>().add(i)
    }

    /// The object's reference leaves, in composed reference-bitmap order.
    #[inline]
    pub fn refs(&self) -> &[Value] {
        // SAFETY: the first `n_refs * size_of::<Value>()` bytes of the block are
        // initialised `Value`s (written in `new`) and stay so for the block's life.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr().cast::<Value>(), self.n_refs as usize) }
    }

    /// Mutable view of the reference leaves.
    #[inline]
    pub fn refs_mut(&mut self) -> &mut [Value] {
        // SAFETY: see `refs`; `&mut self` gives exclusive access.
        unsafe {
            std::slice::from_raw_parts_mut(self.ptr.as_ptr().cast::<Value>(), self.n_refs as usize)
        }
    }

    /// The object's byte-packed primitive leaves.
    #[inline]
    pub fn bytes(&self) -> &[u8] {
        // SAFETY: the byte region follows the reference region inside the same
        // block and was zero-initialised at allocation.
        unsafe {
            std::slice::from_raw_parts(self.bytes_ptr(), self.n_bytes as usize)
        }
    }

    /// Mutable view of the primitive leaves.
    #[inline]
    pub fn bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: see `bytes`; `&mut self` gives exclusive access.
        unsafe { std::slice::from_raw_parts_mut(self.bytes_ptr(), self.n_bytes as usize) }
    }

    #[inline]
    fn bytes_ptr(&self) -> *mut u8 {
        // SAFETY: the offset is within the block by construction.
        unsafe {
            self.ptr.as_ptr().add(self.n_refs as usize * std::mem::size_of::<Value>())
        }
    }
}

impl Drop for ObjStorage {
    fn drop(&mut self) {
        // No per-slot `drop_in_place`: `Value` is `Copy` (every reference variant
        // is a GC-managed tagged handle, not an owning smart pointer), so the
        // reference region has no drop glue — exactly as the `Box<[Value]>` this
        // replaced had none. `value_is_copy_so_drop_can_skip_the_ref_region` in
        // the tests fails the day that stops being true.
        let Some(layout) = Self::layout_of(self.n_bytes as usize, self.n_refs as usize)
        else { return };
        // SAFETY: `ptr` came from `alloc_zeroed` with this exact layout.
        unsafe { dealloc(self.ptr.as_ptr(), layout) };
    }
}

// SAFETY: `ObjStorage` owns its allocation exclusively (like the `Box`es it
// replaced) and hands it out only through `&self` / `&mut self`, so thread
// safety is exactly that of the `Value`s it stores.
unsafe impl Send for ObjStorage {}
unsafe impl Sync for ObjStorage {}

impl std::fmt::Debug for ObjStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObjStorage")
            .field("bytes", &self.bytes())
            .field("refs", &self.refs())
            .finish()
    }
}

#[cfg(test)]
#[path = "obj_storage_tests.rs"]
mod obj_storage_tests;
