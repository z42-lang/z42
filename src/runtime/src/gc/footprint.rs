//! True-footprint accounting: the memory a heap actually holds, kept as it changes.
//!
//! `used_bytes` is a per-object **estimate** of live payload; it leaves out the GC's own
//! per-object metadata (the `RegionEntry` around every object), allocator rounding, dead
//! payloads still waiting for their slot to be reused, empty chunks, and the side tables. On
//! dense workloads RSS ran 1.7–2× `used`, on churn 8×. [`Footprint`] counts the real thing,
//! incrementally, so reading it is one atomic load:
//!
//! | what | charged | credited |
//! |---|---|---|
//! | region chunk (slot array + `initialized` row + per-chunk tables) | chunk grow | never (chunks are not freed) |
//! | var-region chunk (bump or dedicated) | chunk push | dedicated chunk freed |
//! | object / array payload outside the slot (`ObjStorage`, extras, element-type name) | slot filled | slot's dead entry dropped (slot reuse) |
//! | variable-size side tables (`young_list`, free lists, `all_blocks`) | re-measured at every sweep tail | same |
//!
//! `pooled` is the part of `committed` sitting in fully-dead chunks in a region's chunk pool:
//! memory the heap holds but can refill without growing. The soft cap is judged against
//! [`Footprint::occupied`] (= committed − pooled): collecting cannot shrink the pool, only an
//! allocation that misses it grows the footprint.

use std::sync::atomic::{AtomicU64, Ordering};

/// One heap's footprint, shared by its three regions (`Arc`). All `Relaxed`: a heuristic
/// reading for the collection policy and `--stats`, not synchronization.
#[derive(Debug, Default)]
pub struct Footprint {
    committed: AtomicU64,
    pooled: AtomicU64,
}

impl Footprint {
    /// Everything the heap holds (`HeapStats::committed_bytes`).
    #[inline]
    pub fn committed(&self) -> u64 {
        self.committed.load(Ordering::Relaxed)
    }

    /// The part of [`Self::committed`] in pooled (fully-dead, reusable) chunks.
    #[inline]
    pub fn pooled(&self) -> u64 {
        self.pooled.load(Ordering::Relaxed)
    }

    /// What the soft cap is judged against: committed memory the heap cannot refill without
    /// growing.
    #[inline]
    pub fn occupied(&self) -> u64 {
        self.committed().saturating_sub(self.pooled())
    }

    #[inline]
    pub(crate) fn charge(&self, bytes: u64) {
        if bytes != 0 {
            self.committed.fetch_add(bytes, Ordering::Relaxed);
        }
    }

    #[inline]
    pub(crate) fn credit(&self, bytes: u64) {
        if bytes != 0 {
            saturating_sub(&self.committed, bytes);
        }
    }

    /// Charge (`delta > 0`) or credit (`delta < 0`).
    #[inline]
    pub(crate) fn apply(&self, delta: i64) {
        if delta >= 0 {
            self.charge(delta as u64);
        } else {
            self.credit(delta.unsigned_abs());
        }
    }

    /// A chunk of `bytes` entered (`true`) or left (`false`) a chunk pool.
    #[inline]
    pub(crate) fn pool(&self, bytes: u64, entering: bool) {
        if entering {
            self.pooled.fetch_add(bytes, Ordering::Relaxed);
        } else {
            saturating_sub(&self.pooled, bytes);
        }
    }
}

fn saturating_sub(a: &AtomicU64, n: u64) {
    let _ = a.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| Some(v.saturating_sub(n)));
}

/// What a `malloc(n)` really costs: `n` rounded up to the allocator's size class. Modelled on
/// mimalloc's bins (8-byte steps up to 64 B, then four classes per power of two), which is
/// also within a few percent of the system allocators z42 runs on. `0` costs nothing.
#[inline]
pub(crate) fn malloc_size(n: usize) -> u64 {
    if n == 0 {
        return 0;
    }
    if n <= 64 {
        return n.next_multiple_of(8) as u64;
    }
    let top = usize::BITS - 1 - (n - 1).leading_zeros(); // highest set bit of n - 1
    let step = 1usize << (top - 2);
    n.next_multiple_of(step) as u64
}

/// Per-chunk side tables a region grows alongside every chunk: the chunk's `initialized`
/// row is charged at its real length; this covers the `Vec` headers and per-chunk words of
/// the parallel tables (free-slot bucket, census, card word, borrowed flag), rounded up.
pub(crate) const REGION_CHUNK_TABLES: u64 = 64;

/// The same for a var-region chunk: `Chunk` itself, the `all_blocks` bucket header and the
/// five per-chunk words (borrowed / reuse_gen / pool_epoch / census).
pub(crate) const VAR_CHUNK_TABLES: u64 = 64;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malloc_size_follows_the_bins() {
        assert_eq!(malloc_size(0), 0);
        assert_eq!(malloc_size(1), 8);
        assert_eq!(malloc_size(17), 24);
        assert_eq!(malloc_size(64), 64);
        assert_eq!(malloc_size(65), 80);
        assert_eq!(malloc_size(80), 80);
        assert_eq!(malloc_size(129), 160);
        assert_eq!(malloc_size(18432), 20480);
        assert_eq!(malloc_size(65536), 65536);
    }

    #[test]
    fn occupied_excludes_the_pool_and_never_underflows() {
        let f = Footprint::default();
        f.charge(1000);
        f.pool(300, true);
        assert_eq!(f.occupied(), 700);
        f.pool(300, false);
        f.credit(5000);
        assert_eq!(f.committed(), 0);
        f.apply(-1);
        f.apply(42);
        assert_eq!(f.committed(), 42);
    }
}
