//! Major-mark queue plumbing: `mark_if_unmarked`, `snapshot_roots_into_mark_queue`,
//! `drain_mark_queue`, and marking-period allocate-black — the pieces an incremental
//! major is built from, exercised in isolation: roots get marked + enqueued,
//! idempotency on re-snapshot, primitive roots ignored.

use super::*;
use crate::gc::{GcMode, GcRef, MagrGC};

#[test]
fn snapshot_roots_marks_and_enqueues_pinned_roots() {
    let heap = ArcMagrGC::new();

    let obj = heap.alloc_object(dummy_type_desc("Root"), vec![Value::Null], NativeData::None);
    let _pin = heap.pin_root(obj.clone());

    let count = heap.snapshot_roots_into_mark_queue_for_test();
    assert_eq!(count, 1, "single pinned root → 1 newly-marked");

    let queue = heap.mark_queue_for_test();
    assert_eq!(queue.len(), 1);
    let Value::Object(rc) = &obj else { panic!() };
    assert!(GcRef::is_marked(rc, heap.major_mark_for_test()), "root is marked after snapshot");
}

#[test]
fn snapshot_roots_idempotent_on_already_marked() {
    let heap = ArcMagrGC::new();

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
}

#[test]
fn snapshot_roots_skips_primitive_roots() {
    let heap = ArcMagrGC::new();

    // Pin a primitive root (legitimate but mark-irrelevant).
    let _pin = heap.pin_root(Value::I64(42));

    let count = heap.snapshot_roots_into_mark_queue_for_test();
    assert_eq!(count, 0, "primitive root → 0 marks");
    assert_eq!(heap.mark_queue_for_test().len(), 0);
}

#[test]
fn snapshot_roots_includes_external_scanner_output() {
    let heap = ArcMagrGC::new();

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
    assert!(GcRef::is_marked(rc, heap.major_mark_for_test()));
}

#[test]
fn mark_if_unmarked_returns_true_first_then_false() {
    let heap = ArcMagrGC::new();
    let obj = heap.alloc_object(dummy_type_desc("X"), vec![], NativeData::None);

    assert!(heap.mark_if_unmarked_for_test(&obj), "first call marks");
    assert!(!heap.mark_if_unmarked_for_test(&obj), "second call CAS fails");

    let Value::Object(rc) = &obj else { panic!() };
    assert!(GcRef::is_marked(rc, heap.major_mark_for_test()));
}

#[test]
fn mark_if_unmarked_returns_false_for_primitives() {
    let heap = ArcMagrGC::new();
    assert!(!heap.mark_if_unmarked_for_test(&Value::I64(1)));
    assert!(!heap.mark_if_unmarked_for_test(&Value::Null));
    assert!(!heap.mark_if_unmarked_for_test(&Value::Bool(true)));
    // unify-gc-heap PR-4: `Value::Str` is now a GC heap ref (not a primitive) —
    // it marks like any heap object (covered by `mark_if_unmarked_marks_string`).
}

#[test]
fn mark_if_unmarked_marks_string() {
    // unify-gc-heap PR-4: strings are GC blocks; the first mark wins the CAS, the
    // second fails (idempotent), exactly like Object/Closure.
    let heap = ArcMagrGC::new();
    let s = Value::Str("hello".into());
    assert!(heap.mark_if_unmarked_for_test(&s), "first call marks the string block");
    assert!(!heap.mark_if_unmarked_for_test(&s), "second call CAS fails");
    // (leaked test block — no need to clear the mark; never swept/reused.)
}

#[test]
fn mark_queue_starts_empty_in_default_heap() {
    let heap = ArcMagrGC::new();
    assert!(heap.mark_queue_for_test().is_empty(),
        "fresh heap has empty mark_queue regardless of mode");
}

// ── Barriers never feed the mark queue ────────────────────────────────────

#[test]
fn barrier_field_no_op_in_stw_mode() {
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::StwMarkSweep);   // flip-gc-default-to-generational: opt in explicitly
    assert_eq!(heap.mode(), GcMode::StwMarkSweep);

    let owner = heap.alloc_object(dummy_type_desc("O"), vec![Value::Null], NativeData::None);
    let new = heap.alloc_object(dummy_type_desc("N"), vec![], NativeData::None);

    heap.write_barrier_field(&owner, 0, &new);

    assert!(heap.mark_queue_for_test().is_empty(),
        "STW mode → barrier is no-op, mark_queue stays empty");
    let Value::Object(rc) = &new else { panic!() };
    assert!(!GcRef::is_marked(rc, heap.major_mark_for_test()),
        "STW mode → barrier does not mark new value");
}

#[test]
fn collect_cycles_with_context_frees_an_unrooted_cycle_under_stw() {
    // STW mode routes `collect_cycles_with_context` to `collect_cycles()` under a pause.
    // flip-gc-default-to-generational (2026-09-10): STW is not the default, so select
    // it explicitly — this test is about the STW path, not about what the default is.
    use crate::vm_context::VmContext;
    let ctx = VmContext::new();
    ctx.heap().set_mode(GcMode::StwMarkSweep);
    assert_eq!(ctx.heap().mode(), GcMode::StwMarkSweep);

    let heap_dyn = ctx.heap();
    let a = heap_dyn.alloc_object(dummy_type_desc("A"), vec![Value::Null], NativeData::None);
    let b = heap_dyn.alloc_object(dummy_type_desc("B"), vec![Value::Null], NativeData::None);
    {
        let Value::Object(a_gc) = &a else { panic!() };
        let Value::Object(b_gc) = &b else { panic!() };
        a_gc.borrow_mut().refs_mut_raw()[0] = b.clone();
        b_gc.borrow_mut().refs_mut_raw()[0] = a.clone();
    }
    drop(a); drop(b);

    heap_dyn.collect_cycles_with_context(&ctx);

    let mut n = 0;
    heap_dyn.iterate_live_objects(&mut |_| n += 1);
    assert_eq!(n, 0, "STW path via collect_cycles_with_context still frees cycle");
}

// ── Marking-period allocate-black ─────────────────────────────────────────

fn alive_count(heap: &ArcMagrGC) -> usize {
    let mut n = 0;
    heap.iterate_live_objects(&mut |_| n += 1);
    n
}

/// Regression for the *new-object* sweep hazard.
///
/// An incremental major snapshots roots once, when the cycle opens, and never
/// re-scans them before the sweep, and the SATB barrier only records the old
/// value of an overwritten heap slot. So an object allocated during the cycle
/// that becomes reachable afterwards — in production, one that lands in a frame
/// reg — is shaded by nothing, and sweep would tombstone it while the mutator
/// still holds a live handle.
///
/// Driven by hand precisely because the point is to allocate *between* the
/// snapshot and the sweep. Deterministic — no threads, no interleaving needed;
/// the hazard is not a race. The cycle-level model is `tests/gc_incremental_model.rs`.
#[test]
fn allocate_black_keeps_an_object_that_becomes_a_root_after_the_snapshot() {
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::StwMarkSweep); // the closing `collect_cycles` is then a whole-heap one

    let old = heap.alloc_object(dummy_type_desc("Old"), vec![Value::Null], NativeData::None);
    let old_pin = heap.pin_root(old.clone());

    // The cycle opens — the allocate-black window opens with it.
    heap.begin_alloc_black();
    heap.snapshot_roots_into_mark_queue_for_test();

    // Between slices mutators run again. One allocates an object that becomes a
    // root only now, i.e. after the snapshot. It is never stored into a heap
    // field, so no write barrier fires for it.
    let fresh = heap.alloc_object(dummy_type_desc("Fresh"), vec![Value::Null], NativeData::None);
    let fresh_pin = heap.pin_root(fresh.clone());

    // Later slices drain and sweep, then the window closes.
    heap.drain_mark_queue();
    heap.sweep_phase();
    heap.end_alloc_black();

    assert_eq!(alive_count(&heap), 2,
        "an object allocated during the cycle must survive that cycle \
         — allocate-black is what makes that true");

    // Retention is for exactly one cycle, not forever: the newborn keeps this cycle's epoch,
    // so the next cycle's epoch (add-incremental-major-gc M1) sees it white and can take it.
    let Value::Object(fresh_rc) = &fresh else { panic!() };
    heap.reset_marks_for_test();
    assert!(!GcRef::is_marked(fresh_rc, heap.major_mark_for_test()), "newborn is white to the next cycle");

    heap.unpin_root(fresh_pin);
    heap.unpin_root(old_pin);
    heap.collect_cycles();
    assert_eq!(alive_count(&heap), 0, "once unpinned, the next cycle reclaims both");
}

/// The same hazard for `region_var` blocks — strings, closures and array
/// backings are mark-swept by `VarRegion::sweep` exactly like region entries, so
/// `acquire_var_block` (the single chokepoint for all of them) needs the same
/// shading. Without it this string is tombstoned while still rooted.
#[test]
fn allocate_black_covers_var_region_blocks() {
    let heap = ArcMagrGC::new();

    heap.begin_alloc_black();
    heap.snapshot_roots_into_mark_queue_for_test();

    let s = heap.alloc_str("allocated during the cycle");
    let s_pin = heap.pin_root(Value::Str(s));

    heap.drain_mark_queue();
    heap.sweep_phase();
    heap.end_alloc_black();

    assert!(s.var_ref().is_live(),
        "a string allocated during the cycle must survive it");

    heap.unpin_root(s_pin);
}
