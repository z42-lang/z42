//! Word-array bitsets for the region's per-chunk side tables (M8): the young set (one bit per
//! 8-byte granule of a chunk, set at a young block's start) and its per-chunk summary. The cost
//! is a fraction of a byte per block whatever the occupancy, and walking the set bits of a sparse
//! set costs one `trailing_zeros` per hit plus one load per 64 granules.
//!
//! Every walker here reads a word, then visits its bits from the copy: a callback may set or
//! clear bits of the same set (the sweeps delist as they go) without disturbing the walk.

/// Whether bit `i` is set.
#[cfg(test)]
#[inline]
pub(crate) fn test(words: &[u64], i: usize) -> bool {
    words[i >> 6] >> (i & 63) & 1 != 0
}

/// Set bit `i`; `true` when it was clear before.
#[inline]
pub(crate) fn set(words: &mut [u64], i: usize) -> bool {
    let w = &mut words[i >> 6];
    let m = 1u64 << (i & 63);
    let was = *w & m != 0;
    *w |= m;
    !was
}

/// Clear bit `i`; `true` when it was set before.
#[inline]
pub(crate) fn clear(words: &mut [u64], i: usize) -> bool {
    let w = &mut words[i >> 6];
    let m = 1u64 << (i & 63);
    let was = *w & m != 0;
    *w &= !m;
    was
}

/// No bit set.
#[cfg(test)]
#[inline]
pub(crate) fn is_empty(words: &[u64]) -> bool {
    words.iter().all(|&w| w == 0)
}

/// Number of set bits.
#[cfg(test)]
#[inline]
pub(crate) fn count(words: &[u64]) -> usize {
    words.iter().map(|w| w.count_ones() as usize).sum()
}

/// The lowest set bit, if any.
#[cfg(test)]
#[inline]
pub(crate) fn first(words: &[u64]) -> Option<usize> {
    words.iter().enumerate().find(|(_, &w)| w != 0).map(|(k, &w)| k * 64 + w.trailing_zeros() as usize)
}

/// Visit every set bit of `words`, lowest first.
#[inline]
pub(crate) fn for_each(words: &[u64], mut f: impl FnMut(usize)) {
    for (k, &w) in words.iter().enumerate() {
        let mut w = w;
        while w != 0 {
            f(k * 64 + w.trailing_zeros() as usize);
            w &= w - 1;
        }
    }
}

#[cfg(test)]
#[path = "side_bits_tests.rs"]
mod side_bits_tests;
