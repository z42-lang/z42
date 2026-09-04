//! add-mark-sweep-collector P1 (2026-05-21): tests for the new
//! mark phase BFS. These run alongside the existing trial-deletion path
//! (which remains the default until P3). The mark_phase function is
//! `#[allow(dead_code)]` in production until P2 wires it side-by-side
//! and P3 makes it the primary path.

use super::*;
use crate::gc::{GcRef, ArcMagrGC};

/// Mark phase visits every object pinned as a root + their transitive
/// children. Sibling allocations not reachable from any root stay
/// unmarked.
#[test]
fn mark_phase_visits_reachable_only() {
    let heap = ArcMagrGC::new();

    // Build a reachable chain: root_a → b → c (via Object slot chain).
    let c = heap.alloc_object(dummy_type_desc("C"), vec![Value::Null], NativeData::None);
    let b = heap.alloc_object(dummy_type_desc("B"), vec![c.clone()], NativeData::None);
    let a = heap.alloc_object(dummy_type_desc("A"), vec![b.clone()], NativeData::None);

    // Unreachable sibling.
    let d = heap.alloc_object(dummy_type_desc("D"), vec![Value::Null], NativeData::None);

    let root_handle = heap.pin_root(a.clone());

    // Reset marks (defensive — earlier tests don't run mark_phase but
    // we want this test idempotent).
    heap.reset_marks_for_test();

    let marked_count = heap.mark_phase();

    // Expect 3 (a, b, c).
    assert_eq!(marked_count, 3, "mark phase should visit a/b/c only");

    let Value::Object(a_gc) = &a else { panic!() };
    let Value::Object(b_gc) = &b else { panic!() };
    let Value::Object(c_gc) = &c else { panic!() };
    let Value::Object(d_gc) = &d else { panic!() };
    assert!(GcRef::is_marked(a_gc), "a (root) must be marked");
    assert!(GcRef::is_marked(b_gc), "b (reachable via a.refs[0]) must be marked");
    assert!(GcRef::is_marked(c_gc), "c (reachable via b.refs[0]) must be marked");
    assert!(!GcRef::is_marked(d_gc), "d (unreachable) must stay unmarked");

    // Cleanup so other tests are unaffected.
    heap.unpin_root(root_handle);
    heap.reset_marks_for_test();
    drop((a, b, c, d));
}

#[test]
fn mark_phase_idempotent_within_one_cycle() {
    // Calling mark_phase twice in a row without reset should mark 0 the
    // second time (already-marked CAS fails → no new work).
    let heap = ArcMagrGC::new();
    let a = heap.alloc_object(dummy_type_desc("A"), vec![Value::Null], NativeData::None);
    let _root = heap.pin_root(a.clone());

    heap.reset_marks_for_test();
    let first = heap.mark_phase();
    let second = heap.mark_phase();

    assert_eq!(first, 1, "first call marks 1 object");
    assert_eq!(second, 0, "second call finds nothing new (idempotent within cycle)");
    heap.reset_marks_for_test();
}

#[test]
fn mark_phase_handles_array_children() {
    // Array<Value> with Object elements — mark must trace into the array.
    let heap = ArcMagrGC::new();
    let elem1 = heap.alloc_object(dummy_type_desc("E1"), vec![Value::Null], NativeData::None);
    let elem2 = heap.alloc_object(dummy_type_desc("E2"), vec![Value::Null], NativeData::None);
    let arr = heap.alloc_array(vec![elem1.clone(), elem2.clone()]);
    let _root = heap.pin_root(arr.clone());

    heap.reset_marks_for_test();
    let marked = heap.mark_phase();

    // arr + elem1 + elem2 = 3
    assert_eq!(marked, 3, "mark must trace through Array elements");

    let Value::Array(arr_gc)  = &arr  else { panic!() };
    let Value::Object(e1_gc) = &elem1 else { panic!() };
    let Value::Object(e2_gc) = &elem2 else { panic!() };
    assert!(GcRef::is_marked(arr_gc));
    assert!(GcRef::is_marked(e1_gc));
    assert!(GcRef::is_marked(e2_gc));
    heap.reset_marks_for_test();
}

#[test]
fn mark_phase_cyclic_unreachable_stays_unmarked() {
    // Two-node cycle (a ↔ b) with NO root → both unmarked. This is the
    // case trial-deletion needed elaborate tentative counts for; mark-sweep
    // handles it naturally because BFS never starts.
    let heap = ArcMagrGC::new();
    let a = heap.alloc_object(dummy_type_desc("A"), vec![Value::Null], NativeData::None);
    let b = heap.alloc_object(dummy_type_desc("B"), vec![Value::Null], NativeData::None);
    {
        let Value::Object(a_gc) = &a else { panic!() };
        let Value::Object(b_gc) = &b else { panic!() };
        a_gc.borrow_mut().refs_mut()[0] = b.clone();
        b_gc.borrow_mut().refs_mut()[0] = a.clone();
    }

    heap.reset_marks_for_test();
    // No root pinned for a or b.
    let marked = heap.mark_phase();

    assert_eq!(marked, 0, "no roots → 0 marked, even with internal cycle");

    let Value::Object(a_gc) = &a else { panic!() };
    let Value::Object(b_gc) = &b else { panic!() };
    assert!(!GcRef::is_marked(a_gc));
    assert!(!GcRef::is_marked(b_gc));

    // Clear cycle before drop so trial-deletion (still default) doesn't
    // see stale shapes from other tests.
    let Value::Object(a_gc) = &a else { panic!() };
    let Value::Object(b_gc) = &b else { panic!() };
    a_gc.borrow_mut().refs_mut()[0] = Value::Null;
    b_gc.borrow_mut().refs_mut()[0] = Value::Null;
}

#[test]
fn clear_mark_resets_state() {
    let heap = ArcMagrGC::new();
    let a = heap.alloc_object(dummy_type_desc("A"), vec![Value::Null], NativeData::None);
    let _root = heap.pin_root(a.clone());

    heap.reset_marks_for_test();
    heap.mark_phase();

    let Value::Object(a_gc) = &a else { panic!() };
    assert!(GcRef::is_marked(a_gc));

    GcRef::clear_mark(a_gc);
    assert!(!GcRef::is_marked(a_gc));
}

// ── add-mark-sweep-collector P2 (2026-05-21): sweep + full cycle ────────────

fn alive_count(heap: &ArcMagrGC) -> usize {
    let mut n = 0;
    heap.iterate_live_objects(&mut |_| n += 1);
    n
}

/// Mark+sweep frees a simple two-node cycle (a ↔ b), same as trial-deletion.
#[test]
fn mark_sweep_frees_two_node_cycle() {
    let heap = ArcMagrGC::new();
    let a = heap.alloc_object(dummy_type_desc("A"), vec![Value::Null], NativeData::None);
    let b = heap.alloc_object(dummy_type_desc("B"), vec![Value::Null], NativeData::None);
    {
        let Value::Object(a_gc) = &a else { panic!() };
        let Value::Object(b_gc) = &b else { panic!() };
        a_gc.borrow_mut().refs_mut()[0] = b.clone();
        b_gc.borrow_mut().refs_mut()[0] = a.clone();
    }
    drop(a);
    drop(b);
    assert_eq!(alive_count(&heap), 2, "cycle keeps both alive before sweep");

    let freed = heap.collect_cycles_mark_sweep_for_test();

    assert!(freed > 0, "expected positive freed_bytes (got {freed})");
    assert_eq!(alive_count(&heap), 0, "mark+sweep frees the cycle just like trial-deletion");
}

/// Mark+sweep frees a self-referencing cycle (a → a).
#[test]
fn mark_sweep_frees_self_reference_cycle() {
    let heap = ArcMagrGC::new();
    let a = heap.alloc_object(dummy_type_desc("Self"), vec![Value::Null], NativeData::None);
    {
        let Value::Object(a_gc) = &a else { panic!() };
        a_gc.borrow_mut().refs_mut()[0] = a.clone();
    }
    drop(a);
    assert_eq!(alive_count(&heap), 1);

    heap.collect_cycles_mark_sweep_for_test();

    assert_eq!(alive_count(&heap), 0);
}

/// Mark+sweep preserves rooted (reachable) objects.
#[test]
fn mark_sweep_preserves_rooted_chain() {
    let heap = ArcMagrGC::new();
    let leaf = heap.alloc_object(dummy_type_desc("Leaf"), vec![Value::Null], NativeData::None);
    let mid  = heap.alloc_object(dummy_type_desc("Mid"),  vec![leaf.clone()], NativeData::None);
    let root = heap.alloc_object(dummy_type_desc("Root"), vec![mid.clone()],  NativeData::None);

    let root_handle = heap.pin_root(root.clone());

    // Release user-side refs except the pinned root.
    drop(leaf);
    drop(mid);
    drop(root);
    assert_eq!(alive_count(&heap), 3, "pinned root keeps the chain alive");

    let freed = heap.collect_cycles_mark_sweep_for_test();

    assert_eq!(freed, 0, "no unreachable objects → freed_bytes == 0");
    assert_eq!(alive_count(&heap), 3, "rooted chain survives");

    heap.unpin_root(root_handle);
}

/// Mark+sweep frees a 3-cycle (a → b → c → a) when no root reaches it.
#[test]
fn mark_sweep_frees_three_node_cycle() {
    let heap = ArcMagrGC::new();
    let a = heap.alloc_object(dummy_type_desc("A"), vec![Value::Null], NativeData::None);
    let b = heap.alloc_object(dummy_type_desc("B"), vec![Value::Null], NativeData::None);
    let c = heap.alloc_object(dummy_type_desc("C"), vec![Value::Null], NativeData::None);
    {
        let Value::Object(a_gc) = &a else { panic!() };
        let Value::Object(b_gc) = &b else { panic!() };
        let Value::Object(c_gc) = &c else { panic!() };
        a_gc.borrow_mut().refs_mut()[0] = b.clone();
        b_gc.borrow_mut().refs_mut()[0] = c.clone();
        c_gc.borrow_mut().refs_mut()[0] = a.clone();
    }
    drop(a);
    drop(b);
    drop(c);
    assert_eq!(alive_count(&heap), 3);

    heap.collect_cycles_mark_sweep_for_test();

    assert_eq!(alive_count(&heap), 0, "3-cycle freed by mark+sweep");
}

/// Mark+sweep preserves a cycle that one strong external ref keeps alive.
#[test]
fn mark_sweep_preserves_externally_referenced_cycle() {
    let heap = ArcMagrGC::new();
    let a = heap.alloc_object(dummy_type_desc("A"), vec![Value::Null], NativeData::None);
    let b = heap.alloc_object(dummy_type_desc("B"), vec![Value::Null], NativeData::None);
    {
        let Value::Object(a_gc) = &a else { panic!() };
        let Value::Object(b_gc) = &b else { panic!() };
        a_gc.borrow_mut().refs_mut()[0] = b.clone();
        b_gc.borrow_mut().refs_mut()[0] = a.clone();
    }
    // Pin a as root → both a and b are reachable through the cycle.
    let root_handle = heap.pin_root(a.clone());
    drop(b);

    heap.collect_cycles_mark_sweep_for_test();
    assert_eq!(alive_count(&heap), 2, "rooted cycle survives mark+sweep");

    heap.unpin_root(root_handle);
    // Now nothing roots a or b → next mark+sweep should free both.
    drop(a);
    heap.collect_cycles_mark_sweep_for_test();
    assert_eq!(alive_count(&heap), 0);
}

/// Survivor marks reset after sweep so the next cycle starts clean.
#[test]
fn sweep_resets_marks_on_survivors() {
    let heap = ArcMagrGC::new();
    let a = heap.alloc_object(dummy_type_desc("A"), vec![Value::Null], NativeData::None);
    let root_handle = heap.pin_root(a.clone());

    heap.collect_cycles_mark_sweep_for_test();

    let Value::Object(a_gc) = &a else { panic!() };
    assert!(!GcRef::is_marked(a_gc), "survivor's mark must be reset after sweep");

    // Run again — should still preserve a (mark phase will re-mark; sweep
    // will reset). Verifies the cycle is repeatable.
    heap.collect_cycles_mark_sweep_for_test();
    assert!(!GcRef::is_marked(a_gc));
    assert_eq!(alive_count(&heap), 1);

    heap.unpin_root(root_handle);
}
