//! Unit tests for the object byte payload.

use super::*;

#[test]
fn empty_storage_allocates_nothing_and_yields_an_empty_slice() {
    let s = ObjStorage::new(0);
    assert!(s.bytes().is_empty());
    assert_eq!(s.alloc_bytes(), 0);
    assert_eq!(s.cells().load_ref_word(0), 0, "an out-of-bounds reference word reads as null");
}

#[test]
fn bytes_start_zeroed_and_are_writable() {
    let mut s = ObjStorage::new(40);
    assert_eq!(s.bytes().len(), 40);
    assert!(s.bytes().iter().all(|&b| b == 0), "zero = every field's default, null included");
    s.bytes_mut()[7] = 0xAB;
    assert_eq!(s.bytes()[7], 0xAB);
    assert_eq!(s.bytes()[6], 0);
}

#[test]
fn block_is_eight_aligned() {
    for n in [1usize, 8, 13, 16, 40] {
        let s = ObjStorage::new(n);
        assert_eq!(s.bytes().as_ptr() as usize % 8, 0, "payload of {n} bytes misaligned");
    }
}

#[test]
fn cells_round_trip_every_width() {
    let mut s = ObjStorage::new(32);
    {
        let c = s.cells_mut();
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
        let mut s = ObjStorage::new(n);
        if n > 0 { s.bytes_mut()[n - 1] = 0xFF; }
        assert_eq!(s.bytes().len(), n);
    }
}

#[test]
fn from_bytes_copies_the_scalar_payload() {
    let st = ObjStorage::from_bytes(&[1, 2, 3, 4]);
    assert_eq!(st.bytes(), &[1, 2, 3, 4]);
}
