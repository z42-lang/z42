//! Unit tests for the per-context stack-allocation arena (add-escape-analysis-
//! stack-alloc). These exercise the arena's core logic — allocation, validated
//! access, frame_id staleness diagnostics, and LIFO truncation — directly,
//! without needing full z42 e2e (which is CI-gated behind the format bump).
//! Arrays (`ArrayObj::typed`) suffice: the arena logic is element-type-agnostic.

use super::StackArena;
use crate::metadata::types::{ArrayObj, Value};

fn arr(vals: &[i64]) -> ArrayObj {
    ArrayObj::typed("Std.Int64", vals.iter().map(|&n| Value::I64(n)).collect())
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
