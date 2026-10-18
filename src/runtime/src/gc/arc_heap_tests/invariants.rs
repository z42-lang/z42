//! add-gc-debug-invariants P1 (2026-05-22): heap-wide invariant
//! validation tests. Each test exercises either the healthy path
//! (validation passes after a collect cycle) or a corrupted-state
//! path (panics with the expected message via #[should_panic]).
//!
//! Note: these tests are gated by `cfg(debug_assertions)` indirectly
//! — the invariant validator itself is debug-only, so release builds
//! skip these test bodies (validate is no-op).

use super::*;
use crate::gc::GcMode;

// ── Healthy paths: validation passes after collect ─────────────────────────

#[test]
fn healthy_heap_validates_after_stw_collect() {
    let heap = ArcMagrGC::new();
    let v = heap.alloc_object(dummy_type_desc("Foo"), vec![], NativeData::None);
    let _pin = heap.pin_root(v);
    heap.force_collect();
    // collect's tail debug_validate_invariants would have panicked
    // already if anything was wrong. Reaching here = passed.
    heap.debug_validate_invariants();
}

#[test]
fn healthy_heap_validates_after_generational_mode_minor() {
    use crate::vm_context::VmContext;
    let ctx = VmContext::new();
    ctx.heap().set_mode(GcMode::GenerationalMarkSweep);
    let heap_dyn = ctx.heap();
    let pinned = heap_dyn.alloc_object(dummy_type_desc("Pinned"), vec![], NativeData::None);
    let _pin = heap_dyn.pin_root(pinned);
    // Ephemeral allocs.
    for _ in 0..5 {
        let _ = heap_dyn.alloc_object(dummy_type_desc("Eph"), vec![], NativeData::None);
    }

    heap_dyn.collect_cycles_with_context(&ctx);
    // Validation runs at end of generational dispatch.
}

#[test]
fn healthy_heap_validates_with_cross_gen_writes_under_generational() {
    use crate::vm_context::VmContext;
    let ctx = VmContext::new();
    ctx.heap().set_mode(GcMode::GenerationalMarkSweep);
    let heap_dyn = ctx.heap();

    // Promote owner.
    let owner = heap_dyn.alloc_object(dummy_type_desc("Owner"),
        vec![Value::Null], NativeData::None);
    let _pin_owner = heap_dyn.pin_root(owner.clone());
    for _ in 0..2 {
        heap_dyn.collect_cycles_with_context(&ctx);
    }

    // Cross-gen write.
    let child = heap_dyn.alloc_object(dummy_type_desc("Child"), vec![], NativeData::None);
    let _pin_child = heap_dyn.pin_root(child.clone());
    {
        let Value::Object(o) = &owner else { panic!() };
        o.borrow().set_ref_raw_for_test(0, &child.clone());
    }
    heap_dyn.write_barrier_field(&owner, 0, &child);

    // Minor collect with cross-gen reference active.
    heap_dyn.collect_cycles_with_context(&ctx);
}

// ── Corruption detection: should_panic on violations ──────────────────────

#[test]
#[should_panic(expected = "mark_queue stale post-collect")]
fn validation_detects_stale_mark_queue() {
    let heap = ArcMagrGC::new();
    // Manually populate mark_queue (simulate a bug where a major's
    // mark didn't drain). The next debug_validate_invariants call
    // should panic.
    let v = heap.alloc_object(dummy_type_desc("Phantom"), vec![], NativeData::None);
    let _pin = heap.pin_root(v.clone());
    heap.mark_queue_for_test();  // touch the API
    // Push directly via internal access and leave it un-drained.
    {
        let mut q = heap.mark_queue_for_test_mut();
        q.push(v);
    }
    heap.debug_validate_invariants();  // expected panic
}

#[test]
#[should_panic(expected = "stale mark bit after sweep")]
fn validation_detects_stale_mark_on_an_object() {
    let heap = ArcMagrGC::new();
    let v = heap.alloc_object(dummy_type_desc("MarkLeak"), vec![], NativeData::None);
    let _pin = heap.pin_root(v.clone());
    // add-incremental-major-gc M1: a stale mark is now an epoch that is neither the current one
    // nor 0 — which a sweep can never leave behind. Stamp one by hand.
    let Value::Object(gc) = &v else { panic!() };
    let stale = crate::gc::refs::MarkKind::Major(100);
    assert_ne!(stale, heap.major_mark_for_test());
    crate::gc::ObjRef::mark(gc, stale);
    assert!(crate::gc::ObjRef::is_marked(gc, stale));

    heap.debug_validate_invariants();  // expected panic
}

/// add-incremental-major-gc M1: a leftover **minor** bit is stale too — the next minor would skip
/// tracing the entry's children.
#[test]
#[should_panic(expected = "stale mark bit after sweep")]
fn validation_detects_stale_minor_mark_on_an_object() {
    let heap = ArcMagrGC::new();
    let v = heap.alloc_object(dummy_type_desc("MinorLeak"), vec![], NativeData::None);
    let _pin = heap.pin_root(v.clone());
    let Value::Object(gc) = &v else { panic!() };
    crate::gc::ObjRef::mark(gc, crate::gc::refs::MarkKind::Minor);
    heap.debug_validate_invariants();  // expected panic
}

/// add-incremental-major-gc M1: the current epoch is **not** stale — it is exactly what a survivor
/// of the last cycle holds.
#[test]
fn validation_accepts_current_epoch() {
    let heap = ArcMagrGC::new();
    let v = heap.alloc_object(dummy_type_desc("Survivor"), vec![], NativeData::None);
    let _pin = heap.pin_root(v.clone());
    let Value::Object(gc) = &v else { panic!() };
    crate::gc::ObjRef::mark(gc, heap.major_mark_for_test());
    heap.debug_validate_invariants();
}
