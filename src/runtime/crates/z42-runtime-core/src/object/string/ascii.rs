//! Byte search for an ASCII set; matching bytes are UTF-8 scalar boundaries.

pub fn find(bytes: &[u8], mask: u128) -> Option<usize> {
    #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
    if bytes.len() >= 128 {
        let prefix = mask.trailing_ones();
        let mut rest = if prefix == 128 {
            0
        } else {
            (mask >> prefix) << prefix
        };
        if rest.count_ones() <= 3 {
            let mut needles = [255; 3];
            for needle in &mut needles {
                if rest == 0 {
                    break;
                }
                *needle = rest.trailing_zeros() as u8;
                rest &= rest - 1;
            }
            // SSE2 is enabled for this target. The helper only loads complete
            // 16-byte blocks and uses the scalar path for its remaining tail.
            return unsafe { find_sse2(bytes, prefix as u8, needles) };
        }
    }
    scalar(bytes, mask)
}

fn scalar(bytes: &[u8], mask: u128) -> Option<usize> {
    bytes
        .iter()
        .position(|&byte| byte.is_ascii() && mask & (1u128 << byte) != 0)
}

#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
#[target_feature(enable = "sse2")]
unsafe fn find_sse2(bytes: &[u8], prefix: u8, needles: [u8; 3]) -> Option<usize> {
    use std::arch::x86_64::*;
    // Unsigned max + equality tests bytes below prefix. A zero prefix disables
    // that range. Three equality tests cover the remaining isolated ASCII bytes.
    let bound = _mm_set1_epi8(prefix.saturating_sub(1) as i8);
    let enabled = _mm_set1_epi8(if prefix == 0 { 0 } else { -1 });
    let a = _mm_set1_epi8(needles[0] as i8);
    let b = _mm_set1_epi8(needles[1] as i8);
    let c = _mm_set1_epi8(needles[2] as i8);
    let mut index = 0;
    while bytes.len() - index >= 16 {
        // SAFETY: the length check covers the unaligned 16-byte load.
        let input = unsafe { _mm_loadu_si128(bytes.as_ptr().add(index).cast()) };
        let range = _mm_and_si128(_mm_cmpeq_epi8(_mm_max_epu8(input, bound), bound), enabled);
        let singles = _mm_or_si128(
            _mm_cmpeq_epi8(input, a),
            _mm_or_si128(_mm_cmpeq_epi8(input, b), _mm_cmpeq_epi8(input, c)),
        );
        // Non-ASCII bytes never match, including unused 255 needle sentinels.
        let matches =
            _mm_movemask_epi8(_mm_andnot_si128(input, _mm_or_si128(range, singles))) as u32;
        if matches != 0 {
            return Some(index + matches.trailing_zeros() as usize);
        }
        index += 16;
    }
    bytes[index..]
        .iter()
        .position(|&byte| byte.is_ascii() && (byte < prefix || needles.contains(&byte)))
        .map(|tail| index + tail)
}

#[cfg(test)]
#[path = "ascii_tests.rs"]
mod tests;
