//! Atomic cell views over heap memory (object model R1–R3).
//!
//! Every mutable cell on the heap — an object field, a struct leaf, an array element — is at most
//! 8 B and is read and written with one same-width atomic access: primitives `Relaxed` at their
//! natural width, reference words `Release` stores / `Acquire` loads. The two views here are the
//! only place those accesses are spelled out; object storage (`ObjStorage`), array element blocks
//! and heap-resident struct blobs all go through them.
//!
//! - [`Cells`] — loads. Built from a shared borrow of the owner.
//! - [`CellsMut`] — loads and stores. Built from an exclusive borrow of the owner. Stores still
//!   need `&mut` while the per-object lock exists and `bytes()` hands out plain slices of the
//!   same memory; removing the lock (R4) folds the stores into [`Cells`] together with retiring
//!   those slice views.
//!
//! Every access is bounds-checked against the view's length, so a malformed layout cannot reach
//! memory outside the block; the compiler aligns every leaf to its own width, which the debug
//! build asserts.

use std::marker::PhantomData;
use std::sync::atomic::{AtomicU16, AtomicU32, AtomicU64, AtomicU8, Ordering};

use super::Value;

/// Read-only atomic view of `len` bytes at an 8-aligned base.
#[derive(Clone, Copy)]
pub struct Cells<'a> {
    base: *mut u8,
    len: usize,
    _owner: PhantomData<&'a [u8]>,
}

/// Read-write atomic view (see the module docs for why stores need it).
pub struct CellsMut<'a>(Cells<'a>);

impl<'a> std::ops::Deref for CellsMut<'a> {
    type Target = Cells<'a>;
    #[inline(always)]
    fn deref(&self) -> &Cells<'a> {
        &self.0
    }
}

impl<'a> Cells<'a> {
    /// # Safety
    /// `base..base + len` is valid, 8-aligned memory for `'a`, and every concurrent access to it
    /// during `'a` is an atomic access through a view like this one.
    #[inline(always)]
    pub unsafe fn from_raw(base: *mut u8, len: usize) -> Self {
        debug_assert!(len == 0 || base as usize % 8 == 0, "cell view base {base:p} is not 8-aligned");
        Self { base, len, _owner: PhantomData }
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline(always)]
    pub fn base_ptr(&self) -> *const u8 {
        self.base
    }

    #[inline(always)]
    fn at(&self, off: usize, width: usize) -> Option<*mut u8> {
        if off.checked_add(width)? > self.len {
            return None;
        }
        debug_assert_eq!(off % width, 0, "cell at {off} is not {width}-aligned");
        // SAFETY: in bounds of the view (checked above).
        Some(unsafe { self.base.add(off) })
    }

    /// Load the 8 B word at `off` (`0` when out of bounds).
    #[inline(always)]
    pub fn load_word(&self, off: usize, ord: Ordering) -> u64 {
        match self.at(off, 8) {
            // SAFETY: aligned, in bounds, only ever accessed atomically (view contract).
            Some(p) => unsafe { AtomicU64::from_ptr(p.cast()) }.load(ord),
            None => 0,
        }
    }

    /// Acquire-load the 8 B reference word at `off` (`0` = `null` when out of bounds).
    #[inline(always)]
    pub fn load_ref_word(&self, off: usize) -> u64 {
        self.load_word(off, Ordering::Acquire)
    }

    /// The tag word (acquire) and then the payload word (relaxed) of a type-parameter cell; the
    /// payload is meaningful only when the tag word says so (`tparam_cell`). `(0, 0)` = `null`
    /// when out of bounds.
    #[inline(always)]
    pub fn load_tparam(&self, w0: usize, w1: usize) -> (u64, u64) {
        match (self.at(w0, 8), self.at(w1, 8)) {
            // SAFETY: see `load_word`.
            (Some(p0), Some(p1)) => unsafe {
                let tag = AtomicU64::from_ptr(p0.cast()).load(Ordering::Acquire);
                (tag, AtomicU64::from_ptr(p1.cast()).load(Ordering::Relaxed))
            },
            _ => (0, 0),
        }
    }

    /// Relaxed-load the raw bits of the `w`-byte primitive cell at `off`.
    #[inline(always)]
    pub fn load_bits(&self, off: usize, w: usize) -> Option<u64> {
        let p = self.at(off, w)?;
        // SAFETY: aligned + in bounds (`at`), view contract.
        unsafe {
            Some(match w {
                1 => AtomicU8::from_ptr(p).load(Ordering::Relaxed) as u64,
                2 => AtomicU16::from_ptr(p.cast()).load(Ordering::Relaxed) as u64,
                4 => AtomicU32::from_ptr(p.cast()).load(Ordering::Relaxed) as u64,
                8 => AtomicU64::from_ptr(p.cast()).load(Ordering::Relaxed),
                _ => return None,
            })
        }
    }

    /// Plain copy of `dst.len()` bytes starting at `off` (whole-blob snapshots of primitive bytes,
    /// taken under the owner's lock — R4 turns them into per-cell atomic copies). `false` when out
    /// of bounds.
    #[inline]
    pub fn copy_out(&self, off: usize, dst: &mut [u8]) -> bool {
        if off.checked_add(dst.len()).is_none_or(|end| end > self.len) {
            return false;
        }
        // SAFETY: in bounds (checked above); `dst` is a distinct Rust allocation.
        unsafe { std::ptr::copy_nonoverlapping(self.base.add(off), dst.as_mut_ptr(), dst.len()) };
        true
    }

    /// Relaxed-load the primitive leaf at `off` (width `w`, `ty::TAG_*` `kind`).
    #[inline(always)]
    pub fn load_prim(&self, off: usize, w: usize, kind: u8) -> anyhow::Result<Value> {
        match self.load_bits(off, w) {
            Some(bits) => Ok(super::prim_from_bits(kind, bits)),
            None => anyhow::bail!("cell read out of bounds (off={off}, w={w}, len={})", self.len),
        }
    }
}

impl<'a> CellsMut<'a> {
    /// # Safety
    /// As [`Cells::from_raw`], and the caller holds the owner exclusively for `'a`.
    #[inline(always)]
    pub unsafe fn from_raw(base: *mut u8, len: usize) -> Self {
        // SAFETY: forwarded contract.
        Self(unsafe { Cells::from_raw(base, len) })
    }

    /// Store the 8 B word at `off` (no-op when out of bounds).
    #[inline(always)]
    pub fn store_word(&self, off: usize, w: u64, ord: Ordering) {
        if let Some(p) = self.0.at(off, 8) {
            // SAFETY: see `Cells::load_word`.
            unsafe { AtomicU64::from_ptr(p.cast()) }.store(w, ord);
        }
    }

    /// Swap the 8 B word at `off` (`AcqRel`), returning the word it replaced (`0` when out of
    /// bounds).
    #[inline(always)]
    pub fn swap_word(&self, off: usize, w: u64) -> u64 {
        match self.0.at(off, 8) {
            // SAFETY: see `Cells::load_word`.
            Some(p) => unsafe { AtomicU64::from_ptr(p.cast()) }.swap(w, Ordering::AcqRel),
            None => 0,
        }
    }

    /// Compare-and-swap the 8 B word at `off` from `cur` to `new` (`AcqRel` / `Acquire`):
    /// `Err(actual)` when the word was not `cur`. Out of bounds it is a no-op that reports
    /// success, so a caller's retry loop always terminates.
    #[inline(always)]
    pub fn cas_word(&self, off: usize, cur: u64, new: u64) -> Result<u64, u64> {
        let Some(p) = self.0.at(off, 8) else { return Ok(cur) };
        // SAFETY: see `Cells::load_word`.
        unsafe { AtomicU64::from_ptr(p.cast()) }
            .compare_exchange(cur, new, Ordering::AcqRel, Ordering::Acquire)
    }

    /// Release-store the 8 B reference word at `off`. While a major mark is running the store is
    /// a `swap` and returns the word it replaced (the SATB barrier must record the true old value
    /// even under racing writers); otherwise it is a plain release store and returns `0` — there
    /// is nothing to record.
    #[inline(always)]
    pub fn store_ref_word(&self, off: usize, w: u64) -> u64 {
        if crate::gc::satb::marking_any() {
            return self.swap_word(off, w);
        }
        self.store_word(off, w, Ordering::Release);
        0
    }

    /// Fast-path store into a type-parameter cell: when the tag word is `tag`, relaxed-store
    /// `bits` into the payload word and return `true`; otherwise touch nothing.
    #[inline(always)]
    pub fn store_tparam_payload_if(&self, w0: usize, w1: usize, tag: u64, bits: u64) -> bool {
        if self.0.load_word(w0, Ordering::Relaxed) != tag || self.0.at(w1, 8).is_none() {
            return false;
        }
        self.store_word(w1, bits, Ordering::Relaxed);
        true
    }

    /// Plain copy of `src` into the view at `off` — primitive bytes only (a reference word written
    /// this way would bypass the SATB barrier). Taken under the owner's lock, like [`Cells::copy_out`].
    #[inline]
    pub fn copy_in(&self, off: usize, src: &[u8]) -> bool {
        if off.checked_add(src.len()).is_none_or(|end| end > self.0.len) {
            return false;
        }
        // SAFETY: in bounds (checked above); `src` is a distinct Rust allocation; `CellsMut` is
        // built from an exclusive borrow of the owner.
        unsafe { std::ptr::copy_nonoverlapping(src.as_ptr(), self.0.base.add(off), src.len()) };
        true
    }

    /// Relaxed-store raw `bits` into the `w`-byte primitive cell at `off`; `false` when out of
    /// bounds or `w` is not a cell width.
    #[inline(always)]
    pub fn store_bits(&self, off: usize, w: usize, bits: u64) -> bool {
        let Some(p) = self.0.at(off, w) else { return false };
        // SAFETY: aligned + in bounds (`at`), view contract.
        unsafe {
            match w {
                1 => AtomicU8::from_ptr(p).store(bits as u8, Ordering::Relaxed),
                2 => AtomicU16::from_ptr(p.cast()).store(bits as u16, Ordering::Relaxed),
                4 => AtomicU32::from_ptr(p.cast()).store(bits as u32, Ordering::Relaxed),
                8 => AtomicU64::from_ptr(p.cast()).store(bits, Ordering::Relaxed),
                _ => return false,
            }
        }
        true
    }

    /// Relaxed-store `v` into the primitive leaf at `off`. Rejects a value that does not fit the
    /// leaf's kind (e.g. `null` into an `int`), like `encode_prim`.
    #[inline(always)]
    pub fn store_prim(&self, off: usize, w: usize, kind: u8, v: &Value) -> anyhow::Result<()> {
        let bits = super::prim_to_bits(kind, v)?;
        if !self.store_bits(off, w, bits) {
            anyhow::bail!("cell write out of bounds (off={off}, w={w}, len={})", self.0.len);
        }
        Ok(())
    }
}
