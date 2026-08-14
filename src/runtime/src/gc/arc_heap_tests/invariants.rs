//! add-gc-debug-invariants P1 (2026-05-22): heap-wide invariant
//! validation tests. Each test exercises either the healthy path
//! (validation passes after a collect cycle) or a corrupted-state
//! path (panics with the expected message via #[should_panic]).
//!
//! Note: these tests are gated by `cfg(debug_assertions)` indirectly
//! — the invariant validator itself is debug-only, so release builds
//! skip these test bodies (validate is no-op).

use super::*;
use crate::gc::{GcMode, GcRef};

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
fn healthy_heap_validates_after_concurrent_mode_collect() {
    use crate::vm_context::VmContext;
    let ctx = VmContext::new();
    ctx.heap().set_mode(GcMode::ConcurrentMarkSweep);
    let heap_dyn = ctx.heap();
    let v = heap_dyn.alloc_object(dummy_type_desc("Foo"), vec![], NativeData::None);
    let _pin = heap_dyn.pin_root(v);

    heap_dyn.collect_cycles_with_context(&ctx);
    // Validation runs at end of concurrent dispatch; if any invariant
    // were violated we'd have panicked.
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
        o.borrow_mut().refs[0] = child.clone();
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
    // Manually populate mark_queue (simulate a bug where concurrent
    // mark didn't drain). The next debug_validate_invariants call
    // should panic.
    let v = heap.alloc_object(dummy_type_desc("Phantom"), vec![], NativeData::None);
    let _pin = heap.pin_root(v.clone());
    heap.mark_queue_for_test();  // touch the API
    // Push directly via internal access — same as concurrent barrier
    // would do but we leave it un-drained.
    {
        let mut q = heap.mark_queue_for_test_mut();
        q.push(v);
    }
    heap.debug_validate_invariants();  // expected panic
}

#[test]
#[should_panic(expected = "stale mark bit in region_object")]
fn validation_detects_stale_mark_in_region_object() {
    let heap = ArcMagrGC::new();
    let v = heap.alloc_object(dummy_type_desc("MarkLeak"), vec![], NativeData::None);
    let _pin = heap.pin_root(v.clone());
    // Manually mark + don't clear → sweep would normally clear; we
    // simulate the leak.
    let Value::Object(gc) = &v else { panic!() };
    GcRef::mark(gc);
    assert!(GcRef::is_marked(gc));

    heap.debug_validate_invariants();  // expected panic
}

#[test]
#[should_panic(expected = "invariant violation")]
fn validation_detects_region_object_corruption() {
    let heap = ArcMagrGC::new();
    let v = heap.alloc_object(dummy_type_desc("Corrupt"), vec![], NativeData::None);
    let _pin = heap.pin_root(v.clone());

    // Corrupt: clear young_list directly. The entry is still alive
    // young → not in list → YoungEntryNotInList violation.
    {
        let mut r = heap.region_object_for_test().lock();
        r.clear_young_list_for_test();
    }

    heap.debug_validate_invariants();  // expected panic
}
