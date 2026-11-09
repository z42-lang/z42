use super::find;

fn check(bytes: &[u8], candidates: &[u8]) {
    let mask = candidates
        .iter()
        .fold(0u128, |mask, &byte| mask | (1u128 << byte));
    assert_eq!(
        find(bytes, mask),
        bytes.iter().position(|byte| candidates.contains(byte)),
        "len={}, candidates={candidates:?}",
        bytes.len()
    );
}

#[test]
fn ascii_scan_matches_scalar_sets_at_vector_and_tail_boundaries() {
    let mut sets = vec![
        vec![],
        vec![0],
        vec![10, 34, 92],
        vec![0, 31, 127],
        vec![1, 3, 5, 7],
    ];
    for prefix in [1, 16, 32, 64, 127, 128] {
        let mut set: Vec<u8> = (0..prefix).map(|byte| byte as u8).collect();
        if prefix == 32 {
            set.extend([34, 92]);
        }
        sets.push(set);
    }
    for start in 0..16 {
        for length in 0..273 {
            let mut storage = vec![255; start + length];
            for set in &sets {
                check(&storage[start..], set);
            }
            for hit in [0, length / 2, length.saturating_sub(1)] {
                if hit >= length {
                    continue;
                }
                for needle in [0, 10, 31, 34, 92, 127] {
                    storage[start + hit] = needle;
                    for set in &sets {
                        check(&storage[start..], set);
                    }
                }
                storage[start + hit] = 255;
            }
        }
    }
}

#[test]
fn ascii_scan_handles_dense_ascii_and_arbitrary_fallback_sets() {
    let bytes: Vec<u8> = (0..1024).map(|index| (index % 256) as u8).collect();
    for candidates in [vec![34, 92, 10], vec![1, 3, 5, 7, 9], (0..128).collect()] {
        for start in 0..256 {
            check(&bytes[start..], &candidates);
        }
    }
}
