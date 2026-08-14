//! Unit tests for the value-struct byte arena (add-struct-value-semantics).
use super::*;
use crate::metadata::types::{StructTypeLayout, STRUCT_REF_ARC_STRING};
use std::sync::Arc;

/// Pure-primitive layout of `size` bytes (no reference leaves).
fn prim_layout(size: usize) -> Arc<StructTypeLayout> {
    Arc::new(StructTypeLayout { size, ref_offsets: Box::new([]), ref_kinds: Box::new([]) })
}

#[test]
fn alloc_zero_initializes() {
    let mut a = StructArena::default();
    let idx = a.alloc(1, Arc::from("P"), prim_layout(8));
    let all_zero = a.with(idx, 1, |s| s.bytes.iter().all(|&b| b == 0)).unwrap();
    assert!(all_zero);
    assert_eq!(a.allocs, 1);
}

#[test]
fn copy_into_produces_independent_blob() {
    let mut a = StructArena::default();
    let ty: Arc<str> = Arc::from("P");
    let src = a.alloc(1, ty.clone(), prim_layout(8));
    let dst = a.alloc(1, ty, prim_layout(8));
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
    let idx = a.alloc(1, Arc::from("P"), prim_layout(4));
    assert!(a.with(idx, 2, |_| ()).is_err(), "wrong frame_id must error");
    assert!(a.with(999, 1, |_| ()).is_err(), "out-of-range idx must error");
    assert!(a.with(idx, 1, |_| ()).is_ok(), "correct handle resolves");
}

#[test]
fn truncate_frees_lifo() {
    let mut a = StructArena::default();
    let base = a.base();
    let _ = a.alloc(1, Arc::from("P"), prim_layout(4));
    let _ = a.alloc(1, Arc::from("P"), prim_layout(4));
    assert_eq!(a.base(), base + 2);
    a.truncate(base);
    assert_eq!(a.base(), base);
}

/// Reference-leaf value semantics: a `struct { s: string }` blob holds its string
/// in the `refs` side-slice. Copy clones the reference (independent), overwriting
/// one side's leaf leaves the other's intact, and the GC scan visits every leaf.
#[test]
fn ref_leaf_copy_is_independent_and_scanned() {
    let mut a = StructArena::default();
    // struct R { s: string @0 } → 16-byte blob, one reference leaf at offset 0.
    let layout = Arc::new(StructTypeLayout {
        size: 16,
        ref_offsets: Box::new([0]),
        ref_kinds: Box::new([STRUCT_REF_ARC_STRING]),
    });
    let src = a.alloc(1, Arc::from("R"), layout.clone());
    let dst = a.alloc(1, Arc::from("R"), layout);
    // src.s = "hi"; the leaf lives in `refs`, not in `bytes`.
    a.set_ref(src, 1, 0, Value::Str("hi".into())).unwrap();
    // dst = src  (StructCopy → clones the reference leaf).
    a.copy_into(dst, 1, src, 1, 16).unwrap();
    match a.get_ref(dst, 1, 0).unwrap() {
        Value::Str(s) => assert_eq!(&*s, "hi"),
        o => panic!("expected copied string, got {o:?}"),
    }
    // dst.s = "bye" → src.s must stay "hi" (independent reference slots).
    a.set_ref(dst, 1, 0, Value::Str("bye".into())).unwrap();
    match a.get_ref(src, 1, 0).unwrap() {
        Value::Str(s) => assert_eq!(&*s, "hi"),
        o => panic!("src ref leaf must be unchanged, got {o:?}"),
    }
    // GC root scan visits each live blob's reference leaf (2 total).
    let mut n = 0;
    a.scan_roots(&mut |_v| n += 1);
    assert_eq!(n, 2, "scan_roots must visit each live blob's reference leaf");
}

/// A reference-leaf write at an offset not in the layout is rejected (no silent
/// out-of-slice write).
#[test]
fn ref_leaf_bad_offset_errors() {
    let mut a = StructArena::default();
    let layout = Arc::new(StructTypeLayout {
        size: 16,
        ref_offsets: Box::new([0]),
        ref_kinds: Box::new([STRUCT_REF_ARC_STRING]),
    });
    let idx = a.alloc(1, Arc::from("R"), layout);
    assert!(a.set_ref(idx, 1, 8, Value::Null).is_err(), "unknown ref offset must error");
    assert!(a.get_ref(idx, 1, 8).is_err(), "unknown ref offset must error");
}
