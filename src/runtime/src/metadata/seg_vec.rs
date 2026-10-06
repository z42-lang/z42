//! Append-only, lock-free-read tables indexed by a dense `u32`-ish id.
//!
//! Two shapes share one segment layout:
//!
//! - [`SegVec<T>`] — append-only vector. Appends are serialized by a writer
//!   mutex; `get(i)` never blocks and never takes a lock.
//! - [`SparseSegTable<T>`] — fixed-index side table (`T: Default`). A segment
//!   is allocated on first touch (CAS); untouched ranges cost nothing.
//!
//! # Segment layout
//!
//! Segment `k` holds `FIRST_SEG << k` entries (1024, 2048, 4096, …), so
//! segment `k` covers indices `[FIRST_SEG·(2^k − 1), FIRST_SEG·(2^(k+1) − 1))`.
//! The directory is a fixed `[AtomicPtr<T>; NSEG]`; **segments never move and are
//! never freed before the table is dropped**, so a `&T` handed out by `get`
//! stays valid for the table's lifetime — no reallocation, no epoch, no lock.
//! `NSEG = 21` gives `FIRST_SEG · (2^21 − 1)` entries — just under
//! `tokens::IMPORT_BASE`, so every index a table hands out is a valid
//! intra-module token value.
//!
//! # Memory ordering (`SegVec`)
//!
//! The writer, holding the mutex: allocates the segment if needed (stores the
//! directory pointer `Relaxed`), writes the element, then publishes
//! `len = i + 1` with **Release**. A reader loads `len` with **Acquire** and only
//! touches indices below it — so the element write *and* the directory pointer
//! store both happen-before the read, and the directory load can be `Relaxed`.
//!
//! # Memory ordering (`SparseSegTable`)
//!
//! A segment is fully initialized (every entry `T::default()`) before it is
//! installed with a **Release** CAS (`AcqRel` on success); readers load the
//! directory pointer with **Acquire**. A losing racer frees its own segment.
//! Entries are typically atomics / `OnceLock`s, so all further mutation goes
//! through `&T`.

use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ptr;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

/// Entries in segment 0; every later segment doubles.
pub const FIRST_SEG: usize = 1024;
const FIRST_SEG_BITS: u32 = FIRST_SEG.trailing_zeros();
/// Directory size. See the module doc for the resulting capacity.
const NSEG: usize = 21;
/// Total addressable entries: `FIRST_SEG · (2^NSEG − 1)`.
pub const SEG_CAPACITY: usize = FIRST_SEG * ((1usize << NSEG) - 1);

/// `(segment, offset)` for index `i`, or `None` past [`SEG_CAPACITY`].
#[inline]
fn locate(i: usize) -> Option<(usize, usize)> {
    if i >= SEG_CAPACITY {
        return None;
    }
    let j = i + FIRST_SEG;
    let k = (usize::BITS - 1 - j.leading_zeros() - FIRST_SEG_BITS) as usize;
    Some((k, j - (FIRST_SEG << k)))
}

#[inline]
fn seg_len(k: usize) -> usize {
    FIRST_SEG << k
}

fn empty_dir<T>() -> [AtomicPtr<T>; NSEG] {
    std::array::from_fn(|_| AtomicPtr::new(ptr::null_mut()))
}

// ── SegVec ───────────────────────────────────────────────────────────────────

/// Append-only vector with lock-free reads. See the module doc.
pub struct SegVec<T> {
    segs:  [AtomicPtr<T>; NSEG],
    len:   AtomicUsize,
    write: parking_lot::Mutex<()>,
    _own:  PhantomData<T>,
}

// SAFETY: elements are written once (under `write`) and afterwards only shared
// by `&T`; they may be dropped on whichever thread drops the table. So sharing
// the table needs `T: Send + Sync`, sending it needs `T: Send`.
unsafe impl<T: Send> Send for SegVec<T> {}
unsafe impl<T: Send + Sync> Sync for SegVec<T> {}

impl<T> Default for SegVec<T> {
    fn default() -> Self { Self::new() }
}

impl<T> SegVec<T> {
    pub fn new() -> Self {
        Self { segs: empty_dir(), len: AtomicUsize::new(0), write: parking_lot::Mutex::new(()), _own: PhantomData }
    }

    /// Published length (Acquire). Every index below it is readable.
    #[inline]
    pub fn len(&self) -> usize {
        self.len.load(Ordering::Acquire)
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Lock-free read. `None` for an index not yet published.
    #[inline]
    pub fn get(&self, i: usize) -> Option<&T> {
        if i >= self.len.load(Ordering::Acquire) {
            return None;
        }
        // `i < len < SEG_CAPACITY`, so `locate` succeeds; the segment pointer was
        // stored before the Release that published `len` (see module doc).
        let (k, off) = locate(i)?;
        let seg = self.segs[k].load(Ordering::Relaxed);
        // SAFETY: index `i` was initialized before `len` was published past it,
        // and segments never move or free while `self` lives.
        Some(unsafe { &*seg.add(off) })
    }

    /// Append `make(index)` and return its index; `None` when full.
    ///
    /// `make` runs under the writer lock with the index it will occupy, so the
    /// element can carry its own id. It must not append to this same table.
    pub fn push_with(&self, make: impl FnOnce(usize) -> T) -> Option<usize> {
        let _g = self.write.lock();
        let i = self.len.load(Ordering::Relaxed);
        self.write_slot(i, make(i))?;
        self.len.store(i + 1, Ordering::Release);
        Some(i)
    }

    /// Append `n` elements under one lock acquisition; returns the first index.
    /// Elements are published together (one Release store). `None` (and nothing
    /// published) when `n` would not fit.
    pub fn extend_with(&self, n: usize, mut make: impl FnMut(usize) -> T) -> Option<usize> {
        let _g = self.write.lock();
        let first = self.len.load(Ordering::Relaxed);
        if first.checked_add(n)? > SEG_CAPACITY {
            return None;
        }
        for i in first..first + n {
            // Fits by the check above. On a panic in `make` the written-but-unpublished
            // prefix leaks (never dropped) — safe, merely not reclaimed.
            self.write_slot(i, make(i))?;
        }
        self.len.store(first + n, Ordering::Release);
        Some(first)
    }

    /// Write `v` at unpublished index `i` (caller holds `write`).
    fn write_slot(&self, i: usize, v: T) -> Option<()> {
        let (k, off) = locate(i)?;
        let mut seg = self.segs[k].load(Ordering::Relaxed);
        if seg.is_null() {
            seg = alloc_uninit::<T>(seg_len(k));
            self.segs[k].store(seg, Ordering::Relaxed);
        }
        // SAFETY: `off < seg_len(k)`; the slot is unpublished and we hold the
        // writer lock, so nobody else reads or writes it.
        unsafe { seg.add(off).write(v) };
        Some(())
    }
}

impl<T> Drop for SegVec<T> {
    fn drop(&mut self) {
        let len = *self.len.get_mut();
        for (k, slot) in self.segs.iter_mut().enumerate() {
            let seg = *slot.get_mut();
            if seg.is_null() {
                continue;
            }
            let start = FIRST_SEG * ((1usize << k) - 1);
            let live = len.saturating_sub(start).min(seg_len(k));
            // SAFETY: exactly `live` leading entries of this segment were written.
            unsafe {
                ptr::drop_in_place(ptr::slice_from_raw_parts_mut(seg, live));
                free_uninit(seg, seg_len(k));
            }
        }
    }
}

// ── SparseSegTable ───────────────────────────────────────────────────────────

/// Id-indexed side table whose segments are allocated on first touch.
/// See the module doc.
pub struct SparseSegTable<T> {
    segs: [AtomicPtr<T>; NSEG],
    _own: PhantomData<T>,
}

// SAFETY: same reasoning as `SegVec` — entries are shared by `&T` and dropped
// on the dropping thread.
unsafe impl<T: Send> Send for SparseSegTable<T> {}
unsafe impl<T: Send + Sync> Sync for SparseSegTable<T> {}

impl<T: Default> Default for SparseSegTable<T> {
    fn default() -> Self { Self::new() }
}

impl<T: Default> SparseSegTable<T> {
    pub fn new() -> Self {
        Self { segs: empty_dir(), _own: PhantomData }
    }

    /// Lock-free read; `None` when the covering segment was never touched (or
    /// `i` is past capacity). An untouched entry is semantically `T::default()`.
    #[inline]
    pub fn get(&self, i: usize) -> Option<&T> {
        let (k, off) = locate(i)?;
        let seg = self.segs[k].load(Ordering::Acquire);
        if seg.is_null() {
            return None;
        }
        // SAFETY: an installed segment is fully initialized and never freed
        // while `self` lives.
        Some(unsafe { &*seg.add(off) })
    }

    /// The entry at `i`, allocating its segment (filled with `T::default()`) on
    /// first touch. `None` only past capacity.
    pub fn get_or_init(&self, i: usize) -> Option<&T> {
        let (k, off) = locate(i)?;
        let mut seg = self.segs[k].load(Ordering::Acquire);
        if seg.is_null() {
            seg = self.install(k);
        }
        // SAFETY: as in `get`.
        Some(unsafe { &*seg.add(off) })
    }

    #[cold]
    fn install(&self, k: usize) -> *mut T {
        let n = seg_len(k);
        let fresh = alloc_uninit::<T>(n);
        for off in 0..n {
            // SAFETY: `off < n`; `fresh` is private to this thread.
            unsafe { fresh.add(off).write(T::default()) };
        }
        match self.segs[k].compare_exchange(ptr::null_mut(), fresh, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => fresh,
            Err(winner) => {
                // SAFETY: lost the race — `fresh` was never shared; all `n` entries are initialized.
                unsafe {
                    ptr::drop_in_place(ptr::slice_from_raw_parts_mut(fresh, n));
                    free_uninit(fresh, n);
                }
                winner
            }
        }
    }
}

impl<T> Drop for SparseSegTable<T> {
    fn drop(&mut self) {
        for (k, slot) in self.segs.iter_mut().enumerate() {
            let seg = *slot.get_mut();
            if seg.is_null() {
                continue;
            }
            // SAFETY: an installed segment has all `seg_len(k)` entries initialized.
            unsafe {
                ptr::drop_in_place(ptr::slice_from_raw_parts_mut(seg, seg_len(k)));
                free_uninit(seg, seg_len(k));
            }
        }
    }
}

// ── raw segment allocation ───────────────────────────────────────────────────

fn alloc_uninit<T>(n: usize) -> *mut T {
    let boxed: Box<[MaybeUninit<T>]> = Box::new_uninit_slice(n);
    Box::into_raw(boxed) as *mut MaybeUninit<T> as *mut T
}

/// Free a segment from [`alloc_uninit`] **without** dropping its entries.
///
/// # Safety
/// `seg` came from `alloc_uninit::<T>(n)` with this same `n`, and is not used again.
unsafe fn free_uninit<T>(seg: *mut T, n: usize) {
    drop(Box::from_raw(ptr::slice_from_raw_parts_mut(seg as *mut MaybeUninit<T>, n)));
}

#[cfg(test)]
#[path = "seg_vec_tests.rs"]
mod seg_vec_tests;
