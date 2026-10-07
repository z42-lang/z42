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
use std::sync::atomic::{AtomicU16, AtomicU32, AtomicU64, AtomicU8, Ordering};

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

    /// Bytes this payload asked the allocator for (`0` for a field-less object, which
    /// allocates nothing) — the GC's footprint accounting rounds it to a size class.
    #[inline]
    pub fn alloc_bytes(&self) -> usize {
        self.n_refs as usize * std::mem::size_of::<Value>() + self.n_bytes as usize
    }

    /// The object's reference leaves, in composed reference-bitmap order.
    #[inline]
    pub fn refs(&self) -> &[Value] {
        // SAFETY: the first `n_refs * size_of::<Value>()` bytes of the block are
        // initialised `Value`s (written in `new`) and stay so for the block's life.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr().cast::<Value>(), self.n_refs as usize) }
    }

    /// Mutable view of the reference leaves — **without the SATB barrier** (add-incremental-major-gc
    /// M2a). Only for an object that was just allocated (every old value is `Null`) and for the GC
    /// itself (breaking edges of dead objects). Mutator writes go through
    /// `ScriptObject::set_field_value` / `set_ref_slot`, which record the overwritten value.
    #[inline]
    pub fn refs_mut_raw(&mut self) -> &mut [Value] {
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

// ── Field cells (object model R1) ───────────────────────────────────────────────
//
// Every mutable field cell in the byte region is at most 8 B and is read and written with
// one same-width atomic access: primitives `Relaxed` at their natural width, reference words
// `Release` stores / `Acquire` loads (so publishing a constructed object through a field is
// safe). The compiler aligns every leaf to its own width, so the accesses are aligned; the
// bounds checks keep a malformed layout from touching memory outside the block.
//
// Loads take `&self` and go through the raw block pointer, never through a `&[u8]` view.
// Stores still take `&mut self` while the per-object lock exists and `bytes()` hands out
// slices of the same memory; removing the lock (R4) flips them to `&self` together with
// retiring those slice views.
impl ObjStorage {
    #[inline(always)]
    fn cell(&self, off: usize, width: usize) -> Option<*mut u8> {
        if off + width > self.n_bytes as usize {
            return None;
        }
        debug_assert_eq!(off % width, 0, "field cell at {off} is not {width}-aligned");
        // SAFETY: in bounds of the byte region (checked above).
        Some(unsafe { self.bytes_ptr().add(off) })
    }

    /// Acquire-load the 8 B reference word at byte `off` (`0` when out of bounds = `null`).
    #[inline(always)]
    pub fn load_ref_word(&self, off: usize) -> u64 {
        match self.cell(off, 8) {
            // SAFETY: aligned (`cell`), in bounds, and only ever accessed atomically or under
            // the owner's exclusive borrow.
            Some(p) => unsafe { AtomicU64::from_ptr(p.cast()) }.load(Ordering::Acquire),
            None => 0,
        }
    }

    /// Release-store the 8 B reference word at byte `off`. While a major mark is running the
    /// store is a `swap` and returns the word it replaced (the SATB barrier must record the
    /// true old value even under racing writers); otherwise it is a plain release store and
    /// returns `0` — there is nothing to record.
    #[inline(always)]
    pub fn store_ref_word(&mut self, off: usize, w: u64) -> u64 {
        let Some(p) = self.cell(off, 8) else { return 0 };
        // SAFETY: see `load_ref_word`.
        let a = unsafe { AtomicU64::from_ptr(p.cast()) };
        if crate::gc::satb::marking_any() {
            return a.swap(w, Ordering::AcqRel);
        }
        a.store(w, Ordering::Release);
        0
    }

    /// The two words of a type-parameter cell: tag word at `w0`, payload word at `w1`. The
    /// payload word always lies past the tag word (it is appended after the compiler's
    /// layout), so one bounds check covers both.
    #[inline(always)]
    fn tparam_words(&self, w0: usize, w1: usize) -> Option<(*mut u8, *mut u8)> {
        if w0 >= w1 || w1 + 8 > self.n_bytes as usize {
            return None;
        }
        debug_assert!(w0 % 8 == 0 && w1 % 8 == 0, "type-parameter cell at {w0}/{w1} is not 8-aligned");
        let base = self.bytes_ptr();
        // SAFETY: both words are in bounds of the byte region (checked above).
        Some(unsafe { (base.add(w0), base.add(w1)) })
    }

    /// Load a type-parameter cell: the tag word (acquire), then the payload word (relaxed) —
    /// the payload is only meaningful when the tag word says so (`tparam_cell`). `(0, 0)` =
    /// `null` when out of bounds.
    #[inline(always)]
    pub fn load_tparam(&self, w0: usize, w1: usize) -> (u64, u64) {
        match self.tparam_words(w0, w1) {
            // SAFETY: aligned, in bounds, only ever accessed atomically or under the owner's
            // exclusive borrow.
            Some((p0, p1)) => unsafe {
                let tag = AtomicU64::from_ptr(p0.cast()).load(Ordering::Acquire);
                (tag, AtomicU64::from_ptr(p1.cast()).load(Ordering::Relaxed))
            },
            None => (0, 0),
        }
    }

    /// Fast-path store into a type-parameter cell: when the tag word is `tag`, relaxed-store
    /// `bits` into the payload word and return `true`; otherwise touch nothing.
    #[inline(always)]
    pub fn store_tparam_payload_if(&mut self, w0: usize, w1: usize, tag: u64, bits: u64) -> bool {
        let Some((p0, p1)) = self.tparam_words(w0, w1) else { return false };
        // SAFETY: see `load_tparam`.
        unsafe {
            if AtomicU64::from_ptr(p0.cast()).load(Ordering::Relaxed) != tag {
                return false;
            }
            AtomicU64::from_ptr(p1.cast()).store(bits, Ordering::Relaxed);
        }
        true
    }

    /// Load the 8 B word at `off` with ordering `ord` (`0` when out of bounds). The raw form
    /// behind the type-parameter cells (`tparam_cell`), whose tag word is acquire-loaded and
    /// whose payload word is relaxed.
    #[inline(always)]
    pub fn load_word(&self, off: usize, ord: Ordering) -> u64 {
        match self.cell(off, 8) {
            // SAFETY: see `load_ref_word`.
            Some(p) => unsafe { AtomicU64::from_ptr(p.cast()) }.load(ord),
            None => 0,
        }
    }

    /// Store the 8 B word at `off` with ordering `ord` (no-op when out of bounds).
    #[inline(always)]
    pub fn store_word(&mut self, off: usize, w: u64, ord: Ordering) {
        if let Some(p) = self.cell(off, 8) {
            // SAFETY: see `load_ref_word`.
            unsafe { AtomicU64::from_ptr(p.cast()) }.store(w, ord);
        }
    }

    /// Swap the 8 B word at `off` (`AcqRel`), returning the word it replaced (`0` when out of
    /// bounds).
    #[inline(always)]
    pub fn swap_word(&mut self, off: usize, w: u64) -> u64 {
        match self.cell(off, 8) {
            // SAFETY: see `load_ref_word`.
            Some(p) => unsafe { AtomicU64::from_ptr(p.cast()) }.swap(w, Ordering::AcqRel),
            None => 0,
        }
    }

    /// Compare-and-swap the 8 B word at `off` from `cur` to `new` (`AcqRel` / `Acquire`):
    /// `Err(actual)` when the word was not `cur`. Out of bounds it is a no-op that reports
    /// success, so a caller's retry loop always terminates.
    #[inline(always)]
    pub fn cas_word(&mut self, off: usize, cur: u64, new: u64) -> Result<u64, u64> {
        let Some(p) = self.cell(off, 8) else { return Ok(cur) };
        // SAFETY: see `load_ref_word`.
        unsafe { AtomicU64::from_ptr(p.cast()) }
            .compare_exchange(cur, new, Ordering::AcqRel, Ordering::Acquire)
    }

    /// Zero the reference word at `off` (GC breaking a dead object's edges — no barrier).
    #[inline]
    pub fn clear_ref_word(&mut self, off: usize) {
        if let Some(p) = self.cell(off, 8) {
            // SAFETY: see `load_ref_word`.
            unsafe { AtomicU64::from_ptr(p.cast()) }.store(0, Ordering::Relaxed);
        }
    }

    /// Relaxed-load the primitive leaf at `off` (width `w`, `ty::TAG_*` `kind`).
    #[inline(always)]
    pub fn load_prim(&self, off: usize, w: usize, kind: u8) -> anyhow::Result<Value> {
        let Some(p) = self.cell(off, w) else {
            anyhow::bail!("field read out of bounds (off={off}, w={w}, len={})", self.n_bytes)
        };
        // SAFETY: aligned + in bounds (`cell`).
        let bits = unsafe {
            match w {
                1 => AtomicU8::from_ptr(p).load(Ordering::Relaxed) as u64,
                2 => AtomicU16::from_ptr(p.cast()).load(Ordering::Relaxed) as u64,
                4 => AtomicU32::from_ptr(p.cast()).load(Ordering::Relaxed) as u64,
                8 => AtomicU64::from_ptr(p.cast()).load(Ordering::Relaxed),
                _ => anyhow::bail!("field read: unsupported width {w}"),
            }
        };
        Ok(super::prim_from_bits(kind, bits))
    }

    /// Relaxed-store `v` into the primitive leaf at `off`. Rejects a value that does not fit
    /// the leaf's kind (e.g. `null` into an `int`), like `encode_prim`.
    #[inline(always)]
    pub fn store_prim(&mut self, off: usize, w: usize, kind: u8, v: &Value) -> anyhow::Result<()> {
        let bits = super::prim_to_bits(kind, v)?;
        let Some(p) = self.cell(off, w) else {
            anyhow::bail!("field write out of bounds (off={off}, w={w}, len={})", self.n_bytes)
        };
        // SAFETY: aligned + in bounds (`cell`).
        unsafe {
            match w {
                1 => AtomicU8::from_ptr(p).store(bits as u8, Ordering::Relaxed),
                2 => AtomicU16::from_ptr(p.cast()).store(bits as u16, Ordering::Relaxed),
                4 => AtomicU32::from_ptr(p.cast()).store(bits as u32, Ordering::Relaxed),
                8 => AtomicU64::from_ptr(p.cast()).store(bits, Ordering::Relaxed),
                _ => anyhow::bail!("field write: unsupported width {w}"),
            }
        }
        Ok(())
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
