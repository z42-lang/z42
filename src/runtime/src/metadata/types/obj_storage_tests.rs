//! Unit tests for the single-block object payload (shrink-object-footprint P2).

use super::*;

#[test]
fn empty_storage_allocates_nothing_and_yields_empty_slices() {
    // Regression: the empty stand-in pointer must be `Value`-aligned.
    // `NonNull::<u8>::dangling()` is 1-aligned and `slice::from_raw_parts::<Value>`
    // demands alignment even at length 0 — this aborted under the UB check.
    let s = ObjStorage::new(0, 0);
    assert!(s.bytes().is_empty());
    assert!(s.refs().is_empty());
    assert_eq!(s.refs().as_ptr() as usize % std::mem::align_of::<Value>(), 0);
}

#[test]
fn bytes_start_zeroed_and_are_writable() {
    let mut s = ObjStorage::new(40, 0);
    assert_eq!(s.bytes().len(), 40);
    assert!(s.bytes().iter().all(|&b| b == 0), "zero = every primitive default");
    s.bytes_mut()[7] = 0xAB;
    assert_eq!(s.bytes()[7], 0xAB);
    assert_eq!(s.bytes()[6], 0);
}

#[test]
fn refs_start_null_and_are_writable() {
    let mut s = ObjStorage::new(0, 3);
    assert_eq!(s.refs().len(), 3);
    assert!(s.refs().iter().all(|v| matches!(v, Value::Null)));
    s.refs_mut()[1] = Value::I64(42);
    assert!(matches!(s.refs()[1], Value::I64(42)));
    assert!(matches!(s.refs()[0], Value::Null));
}

#[test]
fn mixed_block_keeps_the_two_regions_disjoint() {
    // The whole risk of one block: a byte write must not corrupt a reference
    // slot and vice versa.
    let mut s = ObjStorage::new(24, 2);
    s.refs_mut()[0] = Value::I64(-1);
    s.refs_mut()[1] = Value::I64(-2);
    for (i, b) in s.bytes_mut().iter_mut().enumerate() {
        *b = (i as u8).wrapping_add(1);
    }
    assert!(matches!(s.refs()[0], Value::I64(-1)));
    assert!(matches!(s.refs()[1], Value::I64(-2)));
    assert_eq!(s.bytes()[0], 1);
    assert_eq!(s.bytes()[23], 24);
}

#[test]
fn byte_region_is_eight_aligned_whatever_the_ref_count() {
    // The composed layout puts i64/f64 leaves at 8-aligned byte offsets, so the
    // byte region's start must be 8-aligned for every reference count.
    for n_refs in 0..5 {
        let s = ObjStorage::new(16, n_refs);
        assert_eq!(
            s.bytes().as_ptr() as usize % 8, 0,
            "byte region misaligned with {n_refs} reference leaves",
        );
    }
}

#[test]
fn value_is_copy_so_drop_can_skip_the_ref_region() {
    // `ObjStorage::drop` deallocates the block WITHOUT running drop glue over the
    // reference slots, which is only sound because every `Value` variant is a
    // GC-managed tagged handle rather than an owning smart pointer. If `Value`
    // ever grows a `Drop`, this stops compiling and `ObjStorage::drop` must add a
    // `drop_in_place` loop — the failure mode otherwise is a silent leak.
    fn assert_copy<T: Copy>() {}
    assert_copy::<Value>();
}

#[test]
fn many_blocks_alloc_and_free_without_tripping_the_allocator() {
    // Exercises the alloc/dealloc pairing across every shape, including the
    // empty stand-in (which must NOT be freed).
    for round in 0..200 {
        let n_bytes = round % 41;
        let n_refs = round % 5;
        let mut s = ObjStorage::new(n_bytes, n_refs);
        if n_bytes > 0 { s.bytes_mut()[n_bytes - 1] = 0xFF; }
        if n_refs > 0 { s.refs_mut()[n_refs - 1] = Value::I64(round as i64); }
        assert_eq!(s.bytes().len(), n_bytes);
        assert_eq!(s.refs().len(), n_refs);
    }
}

#[test]
fn from_bytes_copies_the_scalar_payload() {
    let st = ObjStorage::from_bytes(&[1, 2, 3, 4]);
    assert_eq!(st.bytes(), &[1, 2, 3, 4]);
    assert!(st.refs().is_empty());
}
