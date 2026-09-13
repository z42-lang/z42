//! Unit tests for the per-context stack-allocation arena (add-escape-analysis-
//! stack-alloc). These exercise the arena's core logic — allocation, validated
//! access, frame_id staleness diagnostics, and LIFO truncation — directly,
//! without needing full z42 e2e (which is CI-gated behind the format bump).
//! Arrays (`ArrayObj::typed`) suffice: the arena logic is element-type-agnostic.

use super::StackArena;
use crate::metadata::types::{ArrayObj, Value};

fn arr(vals: &[i64]) -> ArrayObj {
    ArrayObj::stack_typed("Std.Int64", vals.iter().map(|&n| Value::I64(n)).collect())
}

#[test]
fn alloc_and_read_back() {
    let mut a = StackArena::default();
    let idx = a.alloc_arr(7, arr(&[10, 20, 30]));
    assert_eq!(idx, 0);
    let len = a.with_arr(idx, 7, |x| x.len()).unwrap();
    assert_eq!(len, 3);
    let v = a.with_arr(idx, 7, |x| x.get_boxed(1)).unwrap();
    assert_eq!(v, Value::I64(20));
    assert_eq!(a.arr_allocs, 1);
}

#[test]
fn mutate_through_arena() {
    let mut a = StackArena::default();
    let idx = a.alloc_arr(3, arr(&[1, 2, 3]));
    a.with_arr_mut(idx, 3, |x| x.set_boxed(0, Value::I64(99))).unwrap();
    let v = a.with_arr(idx, 3, |x| x.get_boxed(0)).unwrap();
    assert_eq!(v, Value::I64(99));
}

#[test]
fn frame_id_mismatch_is_stale_error() {
    // Diagnostic #1: a handle whose frame_id doesn't match the slot's = a stale
    // handle that outlived its creating frame. Must be a clear error, not UB.
    let mut a = StackArena::default();
    let idx = a.alloc_arr(5, arr(&[1]));
    let err = a.with_arr(idx, 6 /* wrong frame_id */, |x| x.len()).unwrap_err();
    assert!(err.to_string().contains("creating frame exited"),
            "expected stale diagnostic, got: {err}");
}

#[test]
fn out_of_range_idx_is_error() {
    let a = StackArena::default();
    let err = a.with_arr(99, 1, |x| x.len()).unwrap_err();
    assert!(err.to_string().contains("stack-alloc"), "got: {err}");
}

#[test]
fn truncate_frees_and_invalidates() {
    // LIFO free: after a frame truncates back to its base, its slots are gone and
    // a surviving handle to them is caught (idx out of range).
    let mut a = StackArena::default();
    let (obj_base, arr_base) = a.bases();
    assert_eq!((obj_base, arr_base), (0, 0));
    let idx = a.alloc_arr(1, arr(&[1, 2]));
    assert_eq!(a.with_arr(idx, 1, |x| x.len()).unwrap(), 2);
    a.truncate(obj_base, arr_base); // frame exit
    assert!(a.with_arr(idx, 1, |x| x.len()).is_err(), "stale handle must be caught");
}

#[test]
fn lifo_nested_frames() {
    // Frame A allocs, then (nested) frame B allocs; B truncates to its base →
    // A's allocation survives with its own frame_id. Mirrors an object ctor
    // (child frame) allocating its own stack temporaries.
    let mut a = StackArena::default();
    // frame A enters: base (0,0); allocs one array (idx 0, frame_id 100)
    let a_idx = a.alloc_arr(100, arr(&[7]));
    // frame B enters: base = current lens
    let (b_obj_base, b_arr_base) = a.bases();
    let b_idx = a.alloc_arr(200, arr(&[8, 9]));
    assert_eq!(a.with_arr(b_idx, 200, |x| x.len()).unwrap(), 2);
    // frame B exits → truncate to B's base
    a.truncate(b_obj_base, b_arr_base);
    // A's allocation still valid; B's gone.
    assert_eq!(a.with_arr(a_idx, 100, |x| x.get_boxed(0)).unwrap(), Value::I64(7));
    assert!(a.with_arr(b_idx, 200, |x| x.len()).is_err());
}

#[test]
fn reused_slot_rejects_old_handle() {
    // The deadly case: frame A allocs idx 0 (frame_id 1), exits (truncate), then
    // frame C reuses idx 0 (frame_id 2). A stale handle {idx:0, frame_id:1} must
    // NOT silently read frame C's object — frame_id mismatch catches it.
    let mut a = StackArena::default();
    let idx_a = a.alloc_arr(1, arr(&[111]));
    a.truncate(0, 0); // frame A exits
    let idx_c = a.alloc_arr(2, arr(&[222]));
    assert_eq!(idx_a, idx_c); // slot reused
    // fresh handle (frame_id 2) works:
    assert_eq!(a.with_arr(idx_c, 2, |x| x.get_boxed(0)).unwrap(), Value::I64(222));
    // stale handle (frame_id 1) is rejected, NOT silently reading 222:
    assert!(a.with_arr(idx_a, 1, |x| x.get_boxed(0)).is_err());
}

/// **fix-stackalloc-misses-inlined-refs (2026-09-13)**: the object half of the root scan.
///
/// `scan_roots_visits_elements` below covers stack *arrays* and has since the arena landed;
/// stack **objects** had no root-scan test at all, which is why the gap survived
/// `unify-object-byte-layout` PR-3 chunk 2b. That chunk moved every direct object/array
/// field out of the `refs` side-table and into an 8B inlined pointer in `bytes`; the heap
/// traversal learned to read both halves, this arena's did not — so a non-escaping object's
/// array fields were reachable from no GC root and got swept while their owner was live.
///
/// The assertion is deliberately about the **inlined** half only (`refs` is left empty), so
/// reverting the `trace_inline_refs` call in `scan_roots` turns it red rather than merely
/// weakening the count.
#[test]
fn scan_roots_visits_object_inlined_refs() {
    use std::sync::Arc;
    use crate::gc::GcRef;
    use crate::metadata::types::{
        FieldAccess, InlineRef, ObjStorage, ObjectLayout, ScriptObject, TypeDesc, TypeDescCold,
        STRUCT_LEAF_GCREF, TAG_OBJECT,
    };

    // `class Holder { object child; }` — the one field is byte-inlined, side-table empty.
    let layout = Arc::new(ObjectLayout {
        size: 8,
        field_offsets: Box::new([0]),
        field_sizes:   Box::new([8]),
        field_kinds:   Box::new([STRUCT_LEAF_GCREF]),
        ref_offsets:   Box::new([]),
        ref_kinds:     Box::new([]),
        inline_refs:   Box::new([InlineRef { offset: 0, is_array: false }]),
        field_access:  Box::new([FieldAccess { offset: 0, width: 8, tag: TAG_OBJECT, ref_slot: -1 }]),
    });
    let holder_td = Arc::new(TypeDesc {
        class_flags: 0,
        visibility: 0,
        name: "Holder".to_string(),
        base_name: None,
        fields: Vec::new(),
        field_index: crate::metadata::NameIndex::new(),
        vtable: Vec::new(),
        vtable_index: crate::metadata::NameIndex::new(),
        cold: Some(Box::new(TypeDescCold { composed_object_layout: Some(layout), ..Default::default() })),
        id: crate::metadata::tokens::TypeId::UNRESOLVED,
    });
    let leafless = Arc::new(TypeDesc {
        name: "Leaf".to_string(), base_name: None, fields: Vec::new(),
        field_index: crate::metadata::NameIndex::new(), vtable: Vec::new(),
        vtable_index: crate::metadata::NameIndex::new(), cold: None,
        class_flags: 0, visibility: 0, id: crate::metadata::tokens::TypeId::UNRESOLVED,
    });
    let leaf = Value::Object(GcRef::new(ScriptObject::new(leafless, ObjStorage::new(0, 0))));

    let mut holder = ScriptObject::new(holder_td, ObjStorage::new(8, 0));
    assert!(holder.set_field_value(0, &leaf), "the inlined field is a reference slot");
    assert!(holder.refs().is_empty(), "precondition: the edge lives ONLY in `bytes`");

    let mut a = StackArena::default();
    a.alloc_obj(1, holder);

    let mut seen = 0usize;
    a.scan_roots(&mut |v| if matches!(v, Value::Object(_)) { seen += 1 });
    assert_eq!(seen, 1, "the byte-inlined object field must be a GC root of the arena");
}

#[test]
fn scan_roots_visits_elements() {
    // GC root scan must visit every live stack array's elements (they may hold
    // heap GcRefs). Here elements are primitives, but the visit count proves the
    // traversal reaches them.
    let mut a = StackArena::default();
    a.alloc_arr(1, arr(&[1, 2, 3]));
    a.alloc_arr(1, arr(&[4, 5]));
    let mut count = 0;
    a.scan_roots(&mut |_v| count += 1);
    assert_eq!(count, 5);
}
