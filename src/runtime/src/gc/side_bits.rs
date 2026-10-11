//! Word-array bitsets for the regions' per-chunk side tables (M8).
//!
//! A region answers three per-slot questions besides what the slot itself holds — is it
//! constructed, is it young, is it free for reuse — and a fourth per chunk: does it hold
//! anything young at all. Each answer is one bit in a fixed-size word array per chunk (or per
//! region, for the chunk summary), so the cost is a fraction of a byte per slot whatever the
//! occupancy, and walking the set bits of a sparse set costs one `trailing_zeros` per hit plus
//! one load per 64 slots.
//!
//! Every walker here reads a word, then visits its bits from the copy: a callback may set or
//! clear bits of the same set (the sweeps delist as they go) without disturbing the walk.

/// Whether bit `i` is set.
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
#[inline]
pub(crate) fn is_empty(words: &[u64]) -> bool {
    words.iter().all(|&w| w == 0)
}

/// Number of set bits.
#[inline]
pub(crate) fn count(words: &[u64]) -> usize {
    words.iter().map(|w| w.count_ones() as usize).sum()
}

/// The lowest set bit, if any.
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

/// Bits `[0, n)` of a word array of `N` words.
#[inline]
pub(crate) fn prefix<const N: usize>(n: usize) -> [u64; N] {
    let mut out = [0u64; N];
    for (k, w) in out.iter_mut().enumerate() {
        let lo = k * 64;
        *w = if n >= lo + 64 { u64::MAX } else if n > lo { (1u64 << (n - lo)) - 1 } else { 0 };
    }
    out
}

#[cfg(test)]
#[path = "side_bits_tests.rs"]
mod side_bits_tests;
