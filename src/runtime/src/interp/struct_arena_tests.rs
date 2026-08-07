//! Unit tests for the value-struct byte arena (add-struct-value-semantics Phase A).
use super::*;
use std::sync::Arc;

#[test]
fn alloc_zero_initializes() {
    let mut a = StructArena::default();
    let idx = a.alloc(1, Arc::from("P"), 8);
    let all_zero = a.with(idx, 1, |s| s.bytes.iter().all(|&b| b == 0)).unwrap();
    assert!(all_zero);
    assert_eq!(a.allocs, 1);
}

#[test]
fn copy_into_produces_independent_blob() {
    let mut a = StructArena::default();
    let ty: Arc<str> = Arc::from("P");
    let src = a.alloc(1, ty.clone(), 8);
    let dst = a.alloc(1, ty, 8);
    a.with_mut(src, 1, |s| s.bytes[0] = 42).unwrap();
    a.copy_into(dst, 1, src, 1, 8).unwrap();
    assert_eq!(a.with(dst, 1, |s| s.bytes[0]).unwrap(), 42);
    // Mutating the copy must not touch the source (value semantics at byte level).
    a.with_mut(dst, 1, |s| s.bytes[0] = 99).unwrap();
    assert_eq!(a.with(src, 1, |s| s.bytes[0]).unwrap(), 42);
    assert_eq!(a.with(dst, 1, |s| s.bytes[0]).unwrap(), 99);
}

#[test]
fn stale_or_out_of_range_handle_is_rejected() {
    let mut a = StructArena::default();
    let idx = a.alloc(1, Arc::from("P"), 4);
    assert!(a.with(idx, 2, |_| ()).is_err(), "wrong frame_id must error");
    assert!(a.with(999, 1, |_| ()).is_err(), "out-of-range idx must error");
    assert!(a.with(idx, 1, |_| ()).is_ok(), "correct handle resolves");
}

#[test]
fn truncate_frees_lifo() {
    let mut a = StructArena::default();
    let base = a.base();
    let _ = a.alloc(1, Arc::from("P"), 4);
    let _ = a.alloc(1, Arc::from("P"), 4);
    assert_eq!(a.base(), base + 2);
    a.truncate(base);
    assert_eq!(a.base(), base);
}
