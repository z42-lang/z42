//! **Prefetching mark ring** — keeps several grey objects' cache lines in flight while marking.
//!
//! # Why
//!
//! Marking is a pointer chase: an object's header has to be read (its mark CAS, its type, its
//! reference fields) before the objects it points at are even known. Done strictly one object at
//! a time — pop, mark, trace, pop the child just pushed — every step waits for one cache miss, and
//! a heap of long chains (`09_alloc_ctorless`: 4096 chains of ~370 objects) marks at memory
//! latency: measured 40–55 ns per object for a full mark of 1 M objects, against ~10 ns when the
//! lines are already cached. How many of those misses the hardware hides depended on accidents
//! of layout (two objects sharing a cache line, an adjacent-line prefetch), so the same marker ran
//! 2–3× faster or slower when only the object size changed.
//!
//! # How
//!
//! Every value bound for marking goes through a small FIFO first: entering it issues a prefetch of
//! the value's header line, and it is marked and traced only once [`RING`] later values have
//! entered behind it. The marker therefore always has up to [`RING`] independent misses in flight,
//! whatever the object graph's shape (for a heap of chains, `RING` chains advance in turn). The
//! standard technique ("prefetch on grey" with a FIFO in front of the mark stack, as in Boehm's collector);
//! the ring holds plain [`Value`]s (`Copy`), so it costs nothing to keep.
//!
//! Order changes, nothing else: every value that enters the ring leaves it (the loops drain it
//! before they finish or hand their stack back), and marking is order-independent.

use crate::metadata::Value;

/// Values in flight. Measured on Apple M-series: a pointer-chase microbenchmark ran 1.5–2× slower
/// per object at 8 than at 16, and the full mark of 1 M chained objects gained nothing at 32 (it is
/// no longer latency-bound at 16).
pub(super) const RING: usize = 16;

/// A FIFO of [`RING`] values whose header lines have been prefetched.
pub(super) struct MarkRing {
    buf: [Value; RING],
    head: usize,
    len: usize,
}

impl MarkRing {
    pub(super) fn new() -> Self {
        MarkRing { buf: [Value::Null; RING], head: 0, len: 0 }
    }

    /// Queue `v` (prefetching its header) and, once the ring is full, hand back the value that has
    /// waited longest — the one to mark / trace now.
    #[inline]
    pub(super) fn push(&mut self, v: Value) -> Option<Value> {
        prefetch(&v);
        if self.len < RING {
            self.buf[(self.head + self.len) % RING] = v;
            self.len += 1;
            return None;
        }
        let out = std::mem::replace(&mut self.buf[self.head], v);
        self.head = (self.head + 1) % RING;
        Some(out)
    }

    /// Take the value that has waited longest (draining the ring when nothing else is left).
    #[inline]
    pub(super) fn pop_oldest(&mut self) -> Option<Value> {
        if self.len == 0 {
            return None;
        }
        let out = self.buf[self.head];
        self.head = (self.head + 1) % RING;
        self.len -= 1;
        Some(out)
    }
}

/// Prefetch the line holding `v`'s mark (its block header / region entry), for writing: marking
/// CASes it.
#[inline(always)]
fn prefetch(v: &Value) {
    let p: *const u8 = match v {
        Value::Object(o) | Value::BoxedStruct(o) => o.var_ref().header_raw().as_ptr(),
        Value::Array(a) => a.entry_ptr().as_ptr().cast(),
        Value::Str(s) => s.as_ptr(),
        Value::Closure(c) => c.header_raw().as_ptr(),
        _ => return,
    };
    crate::gc::var_region::prefetch_write(p);
}

#[cfg(test)]
#[path = "mark_ring_tests.rs"]
mod mark_ring_tests;
