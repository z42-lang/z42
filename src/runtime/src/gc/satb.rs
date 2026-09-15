//! SATB (snapshot-at-the-beginning) deletion barrier — add-incremental-major-gc M2a (2026-09-16).
//!
//! # What it guarantees
//!
//! A major mark that runs while mutators keep running must still mark **everything reachable at
//! the moment the cycle took its root snapshot**. The classic way to lose an object: a mutator
//! reads a still-white object `X` out of a field of an object the marker has not scanned yet, then
//! overwrites that field. `X` now lives only in a register — which was scanned at the snapshot,
//! *before* `X` got there — so the marker never meets it and the sweep frees it while it is in use.
//!
//! The deletion barrier closes exactly that: **before a heap reference slot is overwritten while a
//! mark is in progress, its old value is recorded**, and the marker greys every recorded value
//! before it finishes. Together with allocate-black (objects born during the cycle are marked)
//! this is the whole Yuasa argument: every object reachable at the snapshot is either reached
//! through the graph as it was, or through the recorded old value of the first edge on its path
//! that was cut. Roots need no barrier — they were all greyed at the snapshot.
//!
//! One more way back from the dead is a **weak / soft reference read**: an object that was only
//! weakly reachable at the snapshot can be handed to a register. Those read paths shade their
//! result (see `ArcMagrGC::shade_if_marking`).
//!
//! # Where the barrier sits
//!
//! Inside the **write primitives** (`ScriptObject::set_field_value` / `set_ref_slot`,
//! `ArrayObj::set_boxed` / `write_struct_elem` / `set_struct_ref`), not at the ~40 call sites. The
//! primitives have no heap in hand, which is why this module is keyed off thread-local state
//! rather than a `&ArcMagrGC`. The raw accessors (`refs_mut_raw`, …) skip it and are reserved for
//! objects that were just allocated (their old values are all `Null`) and for the GC itself
//! (breaking edges of dead objects must not record them).
//!
//! # Multiple heaps
//!
//! A process can host several VM heaps. A process-global queue would let one heap's marker grey
//! another heap's objects with the wrong epoch, so every record lands in the **recording thread's
//! own buffer**, tagged with the heap that thread is bound to (`bind_thread`, from
//! `VmContext::new*`). A thread hands its buffer to its heap when it retires its TLAB — which every
//! park path already does on the owning thread before the collector may proceed.
//!
//! # Cost
//!
//! Outside a mark the barrier is one relaxed load of [`MARKING_HEAPS`] and a not-taken branch.
//! During a mark the slow path consults a thread-local cache that is refreshed only when the set of
//! marking heaps changes ([`MARKING_GEN`]).

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use parking_lot::Mutex;

use crate::gc::refs::MarkKind;
use crate::metadata::Value;

/// Heaps with a major mark in progress. The hot path reads only this.
static MARKING_HEAPS: AtomicUsize = AtomicUsize::new(0);
/// Bumped whenever [`MARKING`] changes, so threads know to refresh their cached view.
static MARKING_GEN: AtomicU64 = AtomicU64::new(1);
/// `(heap id, epoch)` of every heap currently marking.
static MARKING: Mutex<Vec<(u64, u8)>> = Mutex::new(Vec::new());

struct ThreadSatb {
    /// Heap this thread records for (`MagrGC::heap_epoch`); 0 = unbound, records nothing.
    heap: u64,
    /// Outer bindings, restored LIFO as nested `VmContext`s on this thread are dropped.
    outer: Vec<u64>,
    /// `MARKING_GEN` value `epoch` was computed at.
    seen_gen: u64,
    /// The bound heap's mark epoch while it is marking, else 0.
    epoch: u8,
    /// Recorded old values, not yet handed to the heap.
    buf: Vec<Value>,
    /// Test hook: record nothing (the negative control for the barrier's own tests).
    #[cfg(test)]
    disabled: bool,
}

thread_local! {
    // `UnsafeCell`: owner-thread-exclusive, and no path re-enters the barrier while holding the
    // `&mut` (recording only clones a `Value` and pushes it).
    static SATB: UnsafeCell<ThreadSatb> = const {
        UnsafeCell::new(ThreadSatb {
            heap: 0, outer: Vec::new(), seen_gen: 0, epoch: 0, buf: Vec::new(),
            #[cfg(test)]
            disabled: false,
        })
    };
}

#[inline]
fn with_thread<R>(f: impl FnOnce(&mut ThreadSatb) -> R) -> R {
    // SAFETY: see the `SATB` docs.
    SATB.with(|c| f(unsafe { &mut *c.get() }))
}

/// Start recording for `heap`'s mark of `epoch`. Called with the world stopped, at cycle start.
pub(crate) fn begin_marking(heap: u64, epoch: u8) {
    let mut m = MARKING.lock();
    debug_assert!(!m.iter().any(|(h, _)| *h == heap), "heap {heap} began marking twice");
    m.push((heap, epoch));
    MARKING_HEAPS.fetch_add(1, Ordering::Relaxed);
    MARKING_GEN.fetch_add(1, Ordering::Relaxed);
}

/// Stop recording for `heap`. Called with the world stopped, after its mark is complete.
pub(crate) fn end_marking(heap: u64) {
    let mut m = MARKING.lock();
    let before = m.len();
    m.retain(|(h, _)| *h != heap);
    if m.len() != before {
        MARKING_HEAPS.fetch_sub(1, Ordering::Relaxed);
        MARKING_GEN.fetch_add(1, Ordering::Relaxed);
    }
}

/// Whether `heap` has a major mark in progress (the weak / soft read barrier asks this).
#[inline]
pub(crate) fn heap_is_marking(heap: u64) -> Option<MarkKind> {
    if MARKING_HEAPS.load(Ordering::Relaxed) == 0 {
        return None;
    }
    MARKING.lock().iter().find(|(h, _)| *h == heap).map(|(_, e)| MarkKind::Major(*e))
}

/// Bind the calling thread to `heap` (from `VmContext::new*`). Balanced by [`unbind_thread`] (from
/// `VmContext::drop`); nested contexts on one thread unwind LIFO.
pub(crate) fn bind_thread(heap: u64) {
    with_thread(|t| {
        let outer = t.heap;
        t.outer.push(outer);
        t.heap = heap;
        t.seen_gen = 0;
    });
}

/// Undo the innermost [`bind_thread`]. The caller has already handed this thread's buffer to its
/// heap (`retire_thread_tlab`), so anything left is for a heap that is no longer bound here —
/// dropping it is only possible if that heap is not marking, in which case it is empty anyway.
pub(crate) fn unbind_thread() {
    with_thread(|t| {
        debug_assert!(t.buf.is_empty(), "SATB buffer not handed over before unbinding");
        t.heap = t.outer.pop().unwrap_or(0);
        t.seen_gen = 0;
    });
}

/// Hand the calling thread's recorded values to `heap` — empty unless the thread is bound to it.
pub(crate) fn take_thread_buffer(heap: u64) -> Vec<Value> {
    with_thread(|t| if t.heap == heap && !t.buf.is_empty() { std::mem::take(&mut t.buf) } else { Vec::new() })
}

/// The barrier: call **before** a heap reference slot holding `old` is overwritten.
#[inline]
pub fn record_overwrite(old: &Value) {
    if MARKING_HEAPS.load(Ordering::Relaxed) == 0 {
        return;
    }
    if !old.is_heap_ref() {
        return;
    }
    record_slow(old);
}

/// [`record_overwrite`] for a whole slice about to be overwritten.
#[inline]
pub fn record_overwrite_all(old: &[Value]) {
    if MARKING_HEAPS.load(Ordering::Relaxed) == 0 {
        return;
    }
    for v in old {
        if v.is_heap_ref() {
            record_slow(v);
        }
    }
}

#[cold]
#[inline(never)]
fn record_slow(old: &Value) {
    with_thread(|t| {
        #[cfg(test)]
        if t.disabled {
            return;
        }
        if t.heap == 0 {
            return;
        }
        let gen = MARKING_GEN.load(Ordering::Relaxed);
        if t.seen_gen != gen {
            let heap = t.heap;
            t.epoch = MARKING.lock().iter().find(|(h, _)| *h == heap).map_or(0, |(_, e)| *e);
            t.seen_gen = gen;
        }
        if t.epoch == 0 {
            return;
        }
        // Already marked this cycle ⇒ the marker has it; recording would only be extra work.
        if crate::gc::arc_heap::ArcMagrGC::is_marked_value(old, MarkKind::Major(t.epoch)) {
            return;
        }
        t.buf.push(old.clone());
    });
}

/// Test hook: turn the barrier off on this thread (negative controls).
#[cfg(test)]
pub(crate) fn set_disabled_for_test(disabled: bool) {
    with_thread(|t| t.disabled = disabled);
}
