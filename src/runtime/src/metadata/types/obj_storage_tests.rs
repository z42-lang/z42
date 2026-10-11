//! Unit tests for the object byte payload and the atomic cell view's bulk copies.

use super::*;

#[test]
fn empty_storage_allocates_nothing_and_yields_an_empty_snapshot() {
    let s = ObjStorage::new(0);
    assert!(s.is_empty());
    assert!(s.snapshot().is_empty());
    assert_eq!(s.alloc_bytes(), 0);
    assert_eq!(s.cells().load_ref_word(0), 0, "an out-of-bounds reference word reads as null");
}

#[test]
fn bytes_start_zeroed_and_are_writable() {
    let s = ObjStorage::new(40);
    assert_eq!(s.len(), 40);
    assert!(s.snapshot().iter().all(|&b| b == 0), "zero = every field's default, null included");
    assert!(s.cells().store_bits(7, 1, 0xAB));
    let snap = s.snapshot();
    assert_eq!(snap[7], 0xAB);
    assert_eq!(snap[6], 0);
}

#[test]
fn block_is_eight_aligned() {
    for n in [1usize, 8, 13, 16, 40] {
        let s = ObjStorage::new(n);
        assert_eq!(s.cells().base_ptr() as usize % 8, 0, "payload of {n} bytes misaligned");
    }
}

#[test]
fn cells_round_trip_every_width() {
    let s = ObjStorage::new(32);
    {
        let c = s.cells();
        assert!(c.store_bits(0, 1, 0xAB));
        assert!(c.store_bits(2, 2, 0xBEEF));
        assert!(c.store_bits(4, 4, 0xDEAD_BEEF));
        assert!(c.store_bits(8, 8, u64::MAX - 1));
        assert!(!c.store_bits(32, 8, 1), "out of bounds is refused");
        c.store_word(16, 0x1234, std::sync::atomic::Ordering::Release);
    }
    let c = s.cells();
    assert_eq!(c.load_bits(0, 1), Some(0xAB));
    assert_eq!(c.load_bits(2, 2), Some(0xBEEF));
    assert_eq!(c.load_bits(4, 4), Some(0xDEAD_BEEF));
    assert_eq!(c.load_bits(8, 8), Some(u64::MAX - 1));
    assert_eq!(c.load_ref_word(16), 0x1234);
    assert_eq!(c.load_bits(32, 8), None);
}

#[test]
fn many_blocks_alloc_and_free_without_tripping_the_allocator() {
    for round in 0..200usize {
        let n = round % 41;
        let s = ObjStorage::new(n);
        if n > 0 { assert!(s.cells().store_bits(n - 1, 1, 0xFF)); }
        assert_eq!(s.len(), n);
    }
}

#[test]
fn from_bytes_copies_the_scalar_payload() {
    let st = ObjStorage::from_bytes(&[1, 2, 3, 4]);
    assert_eq!(st.snapshot(), vec![1, 2, 3, 4]);
}

#[test]
fn chunked_copies_round_trip_at_every_offset_and_length() {
    // Every (offset, length) pair inside a 24-byte block: the chunking (8 / 4 / 2 / 1 by
    // alignment) must reproduce the bytes exactly, and never touch a byte outside the range.
    for off in 0..24usize {
        for len in 0..=(24 - off) {
            let s = ObjStorage::new(24);
            let c = s.cells();
            let src: Vec<u8> = (0..len as u8).map(|b| b.wrapping_mul(37).wrapping_add(1)).collect();
            assert!(c.copy_in(off, &src));
            let snap = s.snapshot();
            assert_eq!(&snap[off..off + len], &src[..], "copy_in off={off} len={len}");
            assert!(snap[..off].iter().chain(&snap[off + len..]).all(|&b| b == 0),
                "copy_in off={off} len={len} wrote outside its range");
            let mut out = vec![0u8; len];
            assert!(c.copy_out(off, &mut out));
            assert_eq!(out, src, "copy_out off={off} len={len}");
        }
    }
    let s = ObjStorage::new(8);
    assert!(!s.cells().copy_in(4, &[0; 5]), "out of bounds is refused");
    assert!(!s.cells().copy_out(4, &mut [0; 5]), "out of bounds is refused");
}

#[test]
fn copy_prims_moves_whole_elements_and_handles_overlap() {
    for w in [1usize, 2, 4, 8] {
        let s = ObjStorage::new(16 * w);
        let c = s.cells();
        for i in 0..16 { assert!(c.store_bits(i * w, w, i as u64 + 1)); }
        // Overlapping, destination above source: memmove semantics.
        assert!(c.copy_prims(4, &c, 2, 8, w));
        let got: Vec<u64> = (0..16).map(|i| c.load_bits(i * w, w).unwrap()).collect();
        let mut want: Vec<u64> = (1..=16).collect();
        want.copy_within(2..10, 4);
        assert_eq!(got, want, "w={w} upward overlap");
        // Overlapping, destination below source.
        assert!(c.copy_prims(1, &c, 3, 8, w));
        want.copy_within(3..11, 1);
        let got: Vec<u64> = (0..16).map(|i| c.load_bits(i * w, w).unwrap()).collect();
        assert_eq!(got, want, "w={w} downward overlap");
        // Distinct views.
        let d = ObjStorage::new(16 * w);
        assert!(d.cells().copy_prims(0, &c, 0, 16, w));
        assert!((0..16).all(|i| d.cells().load_bits(i * w, w) == c.load_bits(i * w, w)));
        assert!(!d.cells().copy_prims(10, &c, 0, 7, w), "out of bounds is refused");
    }
}
