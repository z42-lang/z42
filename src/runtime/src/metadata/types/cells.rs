//! Atomic cell views over heap memory (object model R1–R4).
//!
//! Every mutable cell on the heap — an object field, a struct leaf, an array element — is at most
//! 8 B and is read and written with one same-width atomic access: primitives `Relaxed` at their
//! natural width, reference words `Release` stores / `Acquire` loads. The view here is the
//! only place those accesses are spelled out; an object's field area (`ScriptObject::cells`), array element blocks
//! and heap-resident struct blobs all go through them.
//!
//! [`Cells`] is the view: loads and stores alike take `&self` (object model R4 removed the
//! per-object lock, so no access to heap cells ever needs an exclusive borrow). Bulk copies
//! ([`Cells::copy_out`] / [`Cells::copy_in`] / [`Cells::copy_prims`]) are atomic too — never a
//! plain `memcpy` over memory another thread may be writing.
//!
//! Every access is bounds-checked against the view's length, so a malformed layout cannot reach
//! memory outside the block; the compiler aligns every leaf to its own width, which the debug
//! build asserts.

use std::marker::PhantomData;
use std::sync::atomic::{AtomicU16, AtomicU32, AtomicU64, AtomicU8, Ordering};

use super::Value;

/// Atomic view (loads and stores) of `len` bytes at an 8-aligned base.
#[derive(Clone, Copy)]
pub struct Cells<'a> {
    base: *mut u8,
    len: usize,
    _owner: PhantomData<&'a [u8]>,
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

    /// Copy `dst.len()` bytes starting at `off` out of the view (whole-blob snapshots: a boxed
    /// struct, an inline struct's primitive bytes, a `byte[]` handed to FFI). `false` when out of
    /// bounds.
    ///
    /// The copy is a sequence of relaxed atomic loads, each the widest of 8 / 4 / 2 / 1 bytes that
    /// the current position's alignment and the remaining length allow ([`chunk_width`]). Every
    /// cell is naturally aligned and at most 8 B wide, so a chunk never splits a cell: each cell in
    /// the snapshot is one complete value some writer stored, never half of two. (A chunk may
    /// cover several narrower cells at once — a mixed-width access, single-copy atomic on every
    /// target the VM runs on, like the JIT's plain-width inline accesses.) 8 B chunks are
    /// `Acquire` loads: a chunk may be a reference word, and a copy that carries it on (an array
    /// clone) must see the referent's construction like any other reader of that word.
    #[inline]
    pub fn copy_out(&self, off: usize, dst: &mut [u8]) -> bool {
        if off.checked_add(dst.len()).is_none_or(|end| end > self.len) {
            return false;
        }
        let mut k = 0usize;
        while k < dst.len() {
            let w = chunk_width(self.base as usize + off + k, dst.len() - k);
            // SAFETY: in bounds (checked above), aligned to `w` (`chunk_width`); view contract.
            let bits = unsafe { load_raw(self.base.add(off + k), w, Ordering::Acquire) };
            dst[k..k + w].copy_from_slice(&bits.to_ne_bytes()[low_bytes(w)]);
            k += w;
        }
        true
    }

    /// Copy `src` into the view at `off` — **primitive bytes only** (a reference word written this
    /// way would bypass the SATB barrier). Relaxed atomic stores chunked like [`Self::copy_out`].
    /// `false` when out of bounds.
    #[inline]
    pub fn copy_in(&self, off: usize, src: &[u8]) -> bool {
        if off.checked_add(src.len()).is_none_or(|end| end > self.len) {
            return false;
        }
        let mut k = 0usize;
        while k < src.len() {
            let w = chunk_width(self.base as usize + off + k, src.len() - k);
            let mut buf = [0u8; 8];
            buf[low_bytes(w)].copy_from_slice(&src[k..k + w]);
            // SAFETY: in bounds (checked above), aligned to `w` (`chunk_width`); view contract.
            unsafe { store_raw(self.base.add(off + k), w, u64::from_ne_bytes(buf)) };
            k += w;
        }
        true
    }

    /// Element-wise move of `n` packed `w`-byte primitives from `src` (element `si` on) into this
    /// view (element `di` on): one relaxed load and one relaxed store of exactly the element width
    /// per element, so no element is ever torn. `src` may be this very view (overlapping ranges
    /// copy in the direction that reads every source element before overwriting it — `memmove`).
    /// `false` (nothing copied) when either range is out of bounds or `w` is not a cell width.
    pub fn copy_prims(&self, di: usize, src: &Cells, si: usize, n: usize, w: usize) -> bool {
        if !matches!(w, 1 | 2 | 4 | 8) {
            return false;
        }
        let fits = |c: &Cells, i: usize| i.checked_add(n).and_then(|e| e.checked_mul(w)).is_some_and(|end| end <= c.len);
        if !fits(self, di) || !fits(src, si) {
            return false;
        }
        // SAFETY (both loops): every index is in bounds (checked above) and `w`-aligned (the
        // blocks are 8-aligned and element `i` sits at `i * w`); view contract.
        let mv = |k: usize| unsafe {
            let bits = load_raw(src.base.add((si + k) * w), w, Ordering::Relaxed);
            store_raw(self.base.add((di + k) * w), w, bits);
        };
        if std::ptr::eq(self.base, src.base) && di > si {
            (0..n).rev().for_each(mv);
        } else {
            (0..n).for_each(mv);
        }
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

/// Stores (relaxed for primitives, release / swap for reference words, CAS for type-parameter
/// tag words). All `&self`: concurrent writers race cell by cell, never tear a cell.
impl<'a> Cells<'a> {
    /// Store the 8 B word at `off` (no-op when out of bounds).
    #[inline(always)]
    pub fn store_word(&self, off: usize, w: u64, ord: Ordering) {
        if let Some(p) = self.at(off, 8) {
            // SAFETY: see `Cells::load_word`.
            unsafe { AtomicU64::from_ptr(p.cast()) }.store(w, ord);
        }
    }

    /// Swap the 8 B word at `off` (`AcqRel`), returning the word it replaced (`0` when out of
    /// bounds).
    #[inline(always)]
    pub fn swap_word(&self, off: usize, w: u64) -> u64 {
        match self.at(off, 8) {
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
        let Some(p) = self.at(off, 8) else { return Ok(cur) };
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
        if self.load_word(w0, Ordering::Relaxed) != tag || self.at(w1, 8).is_none() {
            return false;
        }
        self.store_word(w1, bits, Ordering::Relaxed);
        true
    }

    /// Relaxed-store raw `bits` into the `w`-byte primitive cell at `off`; `false` when out of
    /// bounds or `w` is not a cell width.
    #[inline(always)]
    pub fn store_bits(&self, off: usize, w: usize, bits: u64) -> bool {
        let Some(p) = self.at(off, w) else { return false };
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
            anyhow::bail!("cell write out of bounds (off={off}, w={w}, len={})", self.len);
        }
        Ok(())
    }
}

/// The widest cell width (8 / 4 / 2 / 1) that address `addr` is aligned to and that fits in
/// `remaining` bytes — the chunk a bulk copy moves in one atomic access without splitting a
/// naturally aligned cell.
#[inline(always)]
fn chunk_width(addr: usize, remaining: usize) -> usize {
    let mut w = 8usize;
    while w > 1 && (addr % w != 0 || w > remaining) {
        w /= 2;
    }
    w
}

/// Where the low `w` bytes of a `u64` sit in its native-endian byte image.
#[inline(always)]
fn low_bytes(w: usize) -> std::ops::Range<usize> {
    if cfg!(target_endian = "big") { 8 - w..8 } else { 0..w }
}

/// Load the `w`-byte cell at `p` (zero-extended): relaxed below 8 B, `wide` (relaxed or acquire)
/// for an 8 B cell.
///
/// # Safety
/// `p` is valid for `w` bytes, aligned to `w`, and only ever accessed atomically.
#[inline(always)]
unsafe fn load_raw(p: *mut u8, w: usize, wide: Ordering) -> u64 {
    // SAFETY: forwarded contract.
    unsafe {
        match w {
            1 => AtomicU8::from_ptr(p).load(Ordering::Relaxed) as u64,
            2 => AtomicU16::from_ptr(p.cast()).load(Ordering::Relaxed) as u64,
            4 => AtomicU32::from_ptr(p.cast()).load(Ordering::Relaxed) as u64,
            _ => AtomicU64::from_ptr(p.cast()).load(wide),
        }
    }
}

/// Relaxed-store the low `w` bytes of `bits` into the `w`-byte cell at `p`.
///
/// # Safety
/// As [`load_raw`].
#[inline(always)]
unsafe fn store_raw(p: *mut u8, w: usize, bits: u64) {
    // SAFETY: forwarded contract.
    unsafe {
        match w {
            1 => AtomicU8::from_ptr(p).store(bits as u8, Ordering::Relaxed),
            2 => AtomicU16::from_ptr(p.cast()).store(bits as u16, Ordering::Relaxed),
            4 => AtomicU32::from_ptr(p.cast()).store(bits as u32, Ordering::Relaxed),
            _ => AtomicU64::from_ptr(p.cast()).store(bits, Ordering::Relaxed),
        }
    }
}
