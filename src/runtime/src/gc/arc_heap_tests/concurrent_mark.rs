//! add-concurrent-gc tests — covers P2 helpers (`mark_if_unmarked`,
//! `snapshot_roots_into_mark_queue`) and grows in P3 (barrier override),
//! P4 (concurrent mark loop), P5 (multi-thread stress).
//!
//! P2 tests verify the queue plumbing in isolation: roots get marked +
//! enqueued, idempotency on re-snapshot, primitive roots ignored.

use super::*;
use crate::gc::{GcMode, GcRef, MagrGC};

#[test]
fn snapshot_roots_marks_and_enqueues_pinned_roots() {
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::ConcurrentMarkSweep);

    let obj = heap.alloc_object(dummy_type_desc("Root"), vec![Value::Null], NativeData::None);
    let _pin = heap.pin_root(obj.clone());

    let count = heap.snapshot_roots_into_mark_queue_for_test();
    assert_eq!(count, 1, "single pinned root → 1 newly-marked");

    let queue = heap.mark_queue_for_test();
    assert_eq!(queue.len(), 1);
    let Value::Object(rc) = &obj else { panic!() };
    assert!(GcRef::is_marked(rc), "root is marked after snapshot");

    // Reset so other tests aren't affected.
    GcRef::clear_mark(rc);
}

#[test]
fn snapshot_roots_idempotent_on_already_marked() {
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::ConcurrentMarkSweep);

    let obj = heap.alloc_object(dummy_type_desc("R"), vec![Value::Null], NativeData::None);
    let _pin = heap.pin_root(obj.clone());

    let first = heap.snapshot_roots_into_mark_queue_for_test();
    assert_eq!(first, 1);

    // Second snapshot without clearing marks → 0 new marks; queue is
    // re-cleared then re-populated (only newly-marked enter).
    let second = heap.snapshot_roots_into_mark_queue_for_test();
    assert_eq!(second, 0, "already-marked roots not counted again");
    assert_eq!(heap.mark_queue_for_test().len(), 0,
        "queue empty when no new roots marked");

    let Value::Object(rc) = &obj else { panic!() };
    GcRef::clear_mark(rc);
}

#[test]
fn snapshot_roots_skips_primitive_roots() {
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::ConcurrentMarkSweep);

    // Pin a primitive root (legitimate but mark-irrelevant).
    let _pin = heap.pin_root(Value::I64(42));

    let count = heap.snapshot_roots_into_mark_queue_for_test();
    assert_eq!(count, 0, "primitive root → 0 marks");
    assert_eq!(heap.mark_queue_for_test().len(), 0);
}

#[test]
fn snapshot_roots_includes_external_scanner_output() {
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::ConcurrentMarkSweep);

    let external_obj = heap.alloc_object(
        dummy_type_desc("External"), vec![Value::Null], NativeData::None
    );
    let external_clone = external_obj.clone();
    heap.set_external_root_scanner(Box::new(move |visit| {
        visit(&external_clone);
    }));

    let count = heap.snapshot_roots_into_mark_queue_for_test();
    assert_eq!(count, 1, "external scanner yielded 1 root → 1 mark");

    let Value::Object(rc) = &external_obj else { panic!() };
    assert!(GcRef::is_marked(rc));
    GcRef::clear_mark(rc);
}

#[test]
fn mark_if_unmarked_returns_true_first_then_false() {
    let heap = ArcMagrGC::new();
    let obj = heap.alloc_object(dummy_type_desc("X"), vec![], NativeData::None);

    assert!(ArcMagrGC::mark_if_unmarked_for_test(&obj), "first call marks");
    assert!(!ArcMagrGC::mark_if_unmarked_for_test(&obj), "second call CAS fails");

    let Value::Object(rc) = &obj else { panic!() };
    assert!(GcRef::is_marked(rc));
    GcRef::clear_mark(rc);
}

#[test]
fn mark_if_unmarked_returns_false_for_primitives() {
    assert!(!ArcMagrGC::mark_if_unmarked_for_test(&Value::I64(1)));
    assert!(!ArcMagrGC::mark_if_unmarked_for_test(&Value::Null));
    assert!(!ArcMagrGC::mark_if_unmarked_for_test(&Value::Bool(true)));
    assert!(!ArcMagrGC::mark_if_unmarked_for_test(&Value::Str("x".into())));
}

#[test]
fn mark_queue_starts_empty_in_default_heap() {
    let heap = ArcMagrGC::new();
    assert!(heap.mark_queue_for_test().is_empty(),
        "fresh heap has empty mark_queue regardless of mode");
}

// ── P3: Barrier override branches on mode ──────────────────────────────────

#[test]
fn barrier_field_no_op_in_stw_mode() {
    let heap = ArcMagrGC::new();
    assert_eq!(heap.mode(), GcMode::StwMarkSweep);

    let owner = heap.alloc_object(dummy_type_desc("O"), vec![Value::Null], NativeData::None);
    let new = heap.alloc_object(dummy_type_desc("N"), vec![], NativeData::None);

    heap.write_barrier_field(&owner, 0, &new);

    assert!(heap.mark_queue_for_test().is_empty(),
        "STW mode → barrier is no-op, mark_queue stays empty");
    let Value::Object(rc) = &new else { panic!() };
    assert!(!GcRef::is_marked(rc),
        "STW mode → barrier does not mark new value");
}

#[test]
fn barrier_field_shades_new_value_in_concurrent_mode() {
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::ConcurrentMarkSweep);

    let owner = heap.alloc_object(dummy_type_desc("O"), vec![Value::Null], NativeData::None);
    let new = heap.alloc_object(dummy_type_desc("N"), vec![], NativeData::None);

    heap.write_barrier_field(&owner, 0, &new);

    let queue = heap.mark_queue_for_test();
    assert_eq!(queue.len(), 1, "concurrent mode → barrier pushes new to mark_queue");
    let Value::Object(rc) = &new else { panic!() };
    assert!(GcRef::is_marked(rc),
        "concurrent mode → barrier marks new value gray");

    GcRef::clear_mark(rc);
}

#[test]
fn barrier_array_shades_new_value_in_concurrent_mode() {
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::ConcurrentMarkSweep);

    let arr = heap.alloc_array(vec![Value::Null]);
    let new = heap.alloc_object(dummy_type_desc("E"), vec![], NativeData::None);

    heap.write_barrier_array_elem(&arr, 0, &new);

    let queue = heap.mark_queue_for_test();
    assert_eq!(queue.len(), 1);
    let Value::Object(rc) = &new else { panic!() };
    assert!(GcRef::is_marked(rc));
    GcRef::clear_mark(rc);
}

#[test]
fn barrier_idempotent_on_already_marked_in_concurrent_mode() {
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::ConcurrentMarkSweep);

    let owner = heap.alloc_object(dummy_type_desc("O"), vec![Value::Null], NativeData::None);
    let new = heap.alloc_object(dummy_type_desc("N"), vec![], NativeData::None);

    heap.write_barrier_field(&owner, 0, &new);
    heap.write_barrier_field(&owner, 0, &new);  // duplicate write

    let queue = heap.mark_queue_for_test();
    assert_eq!(queue.len(), 1,
        "duplicate write → CAS fails second time → no re-enqueue");

    let Value::Object(rc) = &new else { panic!() };
    GcRef::clear_mark(rc);
}

// ── P4b: End-to-end concurrent collect via VmContext ──────────────────────

#[test]
fn collect_cycles_with_context_routes_concurrent_under_concurrent_mode() {
    // End-to-end: VmContext + concurrent mode + cycle → cycle is freed.
    use crate::vm_context::VmContext;
    let ctx = VmContext::new();
    ctx.heap().set_mode(GcMode::ConcurrentMarkSweep);

    let heap_dyn = ctx.heap();
    // Build an unrooted cycle (use ArcMagrGC via VmContext's heap accessor).
    let a = heap_dyn.alloc_object(dummy_type_desc("A"), vec![Value::Null], NativeData::None);
    let b = heap_dyn.alloc_object(dummy_type_desc("B"), vec![Value::Null], NativeData::None);
    {
        let Value::Object(a_gc) = &a else { panic!() };
        let Value::Object(b_gc) = &b else { panic!() };
        a_gc.borrow_mut().refs[0] = b.clone();
        b_gc.borrow_mut().refs[0] = a.clone();
    }
    drop(a); drop(b);

    let pre_alive = {
        let mut n = 0;
        heap_dyn.iterate_live_objects(&mut |_| n += 1);
        n
    };
    assert_eq!(pre_alive, 2, "cycle keeps both alive before collect");

    // Production path: collect_cycles_with_context dispatches to the
    // concurrent implementation, runs full snapshot → drain → handshake
    // → sweep sequence.
    heap_dyn.collect_cycles_with_context(&ctx);

    let post_alive = {
        let mut n = 0;
        heap_dyn.iterate_live_objects(&mut |_| n += 1);
        n
    };
    assert_eq!(post_alive, 0, "concurrent collect via context frees unrooted cycle");
}

#[test]
fn collect_cycles_with_context_default_stw_unchanged() {
    // STW mode (default) routes to the same `collect_cycles()` path as
    // pre-this-spec. Verify cycle still freed (parity with STW).
    use crate::vm_context::VmContext;
    let ctx = VmContext::new();
    assert_eq!(ctx.heap().mode(), GcMode::StwMarkSweep);

    let heap_dyn = ctx.heap();
    let a = heap_dyn.alloc_object(dummy_type_desc("A"), vec![Value::Null], NativeData::None);
    let b = heap_dyn.alloc_object(dummy_type_desc("B"), vec![Value::Null], NativeData::None);
    {
        let Value::Object(a_gc) = &a else { panic!() };
        let Value::Object(b_gc) = &b else { panic!() };
        a_gc.borrow_mut().refs[0] = b.clone();
        b_gc.borrow_mut().refs[0] = a.clone();
    }
    drop(a); drop(b);

    heap_dyn.collect_cycles_with_context(&ctx);

    let mut n = 0;
    heap_dyn.iterate_live_objects(&mut |_| n += 1);
    assert_eq!(n, 0, "STW path via collect_cycles_with_context still frees cycle");
}

// ── P4a: Concurrent BFS drain + end-to-end collect (inline) ───────────────

fn alive_count(heap: &ArcMagrGC) -> usize {
    let mut n = 0;
    heap.iterate_live_objects(&mut |_| n += 1);
    n
}

#[test]
fn concurrent_collect_inline_preserves_pinned_chain() {
    // Pinned root → leaf chain; concurrent collect must keep all 3 alive.
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::ConcurrentMarkSweep);

    let leaf = heap.alloc_object(dummy_type_desc("Leaf"), vec![Value::Null], NativeData::None);
    let mid  = heap.alloc_object(dummy_type_desc("Mid"),  vec![leaf.clone()], NativeData::None);
    let root = heap.alloc_object(dummy_type_desc("Root"), vec![mid.clone()],  NativeData::None);
    let root_handle = heap.pin_root(root.clone());

    drop(leaf); drop(mid); drop(root);
    assert_eq!(alive_count(&heap), 3, "pinned root keeps the chain alive");

    let freed = heap.run_cycle_collection_concurrent_inline_for_test();
    assert_eq!(freed, 0, "no unreachable → freed_bytes == 0");
    assert_eq!(alive_count(&heap), 3, "rooted chain survives concurrent collect");

    heap.unpin_root(root_handle);
}

#[test]
fn concurrent_collect_inline_frees_unreachable_cycle() {
    // Two-node cycle with no root → concurrent collect should free both,
    // matching the STW path's behavior.
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::ConcurrentMarkSweep);

    let a = heap.alloc_object(dummy_type_desc("A"), vec![Value::Null], NativeData::None);
    let b = heap.alloc_object(dummy_type_desc("B"), vec![Value::Null], NativeData::None);
    {
        let Value::Object(a_gc) = &a else { panic!() };
        let Value::Object(b_gc) = &b else { panic!() };
        a_gc.borrow_mut().refs[0] = b.clone();
        b_gc.borrow_mut().refs[0] = a.clone();
    }
    drop(a); drop(b);
    assert_eq!(alive_count(&heap), 2);

    let freed = heap.run_cycle_collection_concurrent_inline_for_test();
    assert!(freed > 0);
    assert_eq!(alive_count(&heap), 0, "unreachable cycle freed");
}

#[test]
fn concurrent_collect_inline_traces_via_array_elements() {
    // Array of pinned objects — sweep must keep all elements alive
    // because the array is rooted and BFS traces through Array.
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::ConcurrentMarkSweep);

    let elems: Vec<Value> = (0..5)
        .map(|_| heap.alloc_object(dummy_type_desc("E"), vec![], NativeData::None))
        .collect();
    let arr = heap.alloc_array(elems);
    let pin = heap.pin_root(arr);

    let pre = alive_count(&heap);
    assert!(pre >= 6, "5 objects + 1 array = at least 6 alive before collect");

    let freed = heap.run_cycle_collection_concurrent_inline_for_test();
    assert_eq!(freed, 0);
    assert_eq!(alive_count(&heap), pre, "everything reachable from pinned array survives");

    heap.unpin_root(pin);
}

#[test]
fn concurrent_collect_inline_with_simulated_barrier_marks_late_writes() {
    // Simulate the concurrent mutator: between root snapshot and drain
    // completion, a mutator writes a new object into a rooted slot.
    // Without barrier shading, the new object would be missed by the
    // collector and incorrectly swept.
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::ConcurrentMarkSweep);

    let root_obj = heap.alloc_object(
        dummy_type_desc("Root"), vec![Value::Null], NativeData::None
    );
    let root_handle = heap.pin_root(root_obj.clone());

    // Phase: pre-mark — simulate concurrent collect start by snapshotting roots
    heap.snapshot_roots_into_mark_queue_for_test();

    // Simulate mutator: write a new object into root.refs[0] mid-collect.
    // Barrier dispatch shades the new object gray + enqueues.
    let new_child = heap.alloc_object(
        dummy_type_desc("LateChild"), vec![], NativeData::None
    );
    {
        let Value::Object(root_gc) = &root_obj else { panic!() };
        root_gc.borrow_mut().refs[0] = new_child.clone();
    }
    heap.write_barrier_field(&root_obj, 0, &new_child);  // P3 barrier dispatch

    // Now run drain + sweep. The late-written child must survive.
    let _traced = heap.run_cycle_collection_concurrent_inline_for_test();

    // Expectation: late-written child survives because barrier marked it.
    // root (pinned) + late_child (barrier-shaded, reachable via root.refs[0]).
    assert_eq!(alive_count(&heap), 2,
        "late-barrier-shaded child survives concurrent collect");

    heap.unpin_root(root_handle);
    drop(new_child);
}

#[test]
fn concurrent_collect_inline_resets_marks_on_survivors() {
    // Sweep's reset-marks behavior must apply under concurrent path too —
    // survivors come back unmarked for the next cycle.
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::ConcurrentMarkSweep);

    let root = heap.alloc_object(dummy_type_desc("R"), vec![Value::Null], NativeData::None);
    let pin = heap.pin_root(root.clone());

    heap.run_cycle_collection_concurrent_inline_for_test();
    let Value::Object(rc) = &root else { panic!() };
    assert!(!GcRef::is_marked(rc),
        "sweep resets survivor's mark for next cycle");

    // Second cycle should still preserve root.
    heap.run_cycle_collection_concurrent_inline_for_test();
    assert_eq!(alive_count(&heap), 1);
    assert!(!GcRef::is_marked(rc));

    heap.unpin_root(pin);
}

#[test]
fn barrier_mode_switch_takes_effect_immediately_on_next_write() {
    let heap = ArcMagrGC::new();
    let owner = heap.alloc_object(dummy_type_desc("O"), vec![Value::Null], NativeData::None);
    let n1 = heap.alloc_object(dummy_type_desc("N1"), vec![], NativeData::None);
    let n2 = heap.alloc_object(dummy_type_desc("N2"), vec![], NativeData::None);

    // STW mode: barrier no-op
    heap.write_barrier_field(&owner, 0, &n1);
    assert!(heap.mark_queue_for_test().is_empty());

    // Switch to concurrent: next barrier shades
    heap.set_mode(GcMode::ConcurrentMarkSweep);
    heap.write_barrier_field(&owner, 0, &n2);

    let queue = heap.mark_queue_for_test();
    assert_eq!(queue.len(), 1, "switch effective on next write");

    let Value::Object(rc) = &n2 else { panic!() };
    GcRef::clear_mark(rc);
}
