//! add-write-barriers (2026-05-21): unit tests proving the
//! `ArcMagrGC::write_barrier_*` trait overrides dispatch to the
//! installed `BarrierObserver`. The call-site filter (Decision 1 —
//! only invoke when `new.is_heap_ref()`) is verified end-to-end by
//! the broader `z42 xtask.zpkg test` GREEN gate (stdlib + JIT
//! smoke + golden tests); these unit tests focus on the GC-side
//! dispatch + observer plumbing.

use super::*;
use std::sync::Arc;

use crate::gc::arc_heap::{BarrierEvent, BarrierObserver};

// ── 1. Direct trait dispatch ────────────────────────────────────────────────

#[test]
fn write_barrier_field_dispatches_observer() {
    let heap = ArcMagrGC::new();
    let obs = Arc::new(BarrierObserver::new());
    heap.install_barrier_observer(obs.clone());

    let owner = heap.alloc_object(dummy_type_desc("Owner"), vec![Value::Null], NativeData::None);
    let new = heap.alloc_object(dummy_type_desc("Child"), vec![], NativeData::None);

    heap.write_barrier_field(&owner, 0, &new);

    let events = obs.events();
    assert_eq!(events.len(), 1, "exactly one barrier dispatch recorded");
    match &events[0] {
        BarrierEvent::Field { slot, new_is_heap, .. } => {
            assert_eq!(*slot, 0);
            assert!(*new_is_heap, "Object new value → new_is_heap=true");
        }
        other => panic!("expected Field event, got {:?}", other),
    }
}

#[test]
fn write_barrier_array_elem_dispatches_observer() {
    let heap = ArcMagrGC::new();
    let obs = Arc::new(BarrierObserver::new());
    heap.install_barrier_observer(obs.clone());

    let arr = heap.alloc_array(vec![Value::Null, Value::Null]);
    let new = heap.alloc_object(dummy_type_desc("Elem"), vec![], NativeData::None);

    heap.write_barrier_array_elem(&arr, 1, &new);

    let events = obs.events();
    assert_eq!(events.len(), 1);
    match &events[0] {
        BarrierEvent::ArrayElem { idx, new_is_heap, .. } => {
            assert_eq!(*idx, 1);
            assert!(*new_is_heap);
        }
        other => panic!("expected ArrayElem event, got {:?}", other),
    }
}

// ── 2. Observer captures new_is_heap metadata accurately ────────────────────

#[test]
fn observer_records_new_is_heap_metadata_correctly() {
    // The trait method itself doesn't filter primitives — it just records
    // what it sees. Call sites are responsible for the is_heap_ref filter
    // (Decision 1). This test pins the observer's classification logic.
    let heap = ArcMagrGC::new();
    let obs = Arc::new(BarrierObserver::new());
    heap.install_barrier_observer(obs.clone());

    let owner = heap.alloc_object(dummy_type_desc("Owner"), vec![Value::Null], NativeData::None);
    let heap_ref = heap.alloc_array(vec![]);
    let primitive = Value::I64(42);

    // Direct dispatch (simulates a hypothetical caller that didn't filter)
    heap.write_barrier_field(&owner, 0, &heap_ref);
    heap.write_barrier_field(&owner, 0, &primitive);

    let events = obs.events();
    assert_eq!(events.len(), 2);
    match (&events[0], &events[1]) {
        (BarrierEvent::Field { new_is_heap: a, .. },
         BarrierEvent::Field { new_is_heap: b, .. }) => {
            assert!(*a,  "first new value is a heap ref");
            assert!(!*b, "second new value is a primitive");
        }
        other => panic!("expected two Field events, got {:?}", other),
    }
}

// ── 3. Observers are per-instance and clear-able ────────────────────────────

#[test]
fn observer_independent_per_heap_instance() {
    let heap_a = ArcMagrGC::new();
    let heap_b = ArcMagrGC::new();
    let obs_a = Arc::new(BarrierObserver::new());
    let obs_b = Arc::new(BarrierObserver::new());
    heap_a.install_barrier_observer(obs_a.clone());
    heap_b.install_barrier_observer(obs_b.clone());

    let owner_a = heap_a.alloc_object(dummy_type_desc("A"), vec![Value::Null], NativeData::None);
    let owner_b = heap_b.alloc_object(dummy_type_desc("B"), vec![Value::Null], NativeData::None);
    let child = heap_a.alloc_object(dummy_type_desc("Child"), vec![], NativeData::None);

    heap_a.write_barrier_field(&owner_a, 0, &child);

    assert_eq!(obs_a.count(), 1, "heap_a observer sees its dispatch");
    assert_eq!(obs_b.count(), 0, "heap_b observer untouched");

    heap_b.write_barrier_field(&owner_b, 0, &child);
    assert_eq!(obs_a.count(), 1);
    assert_eq!(obs_b.count(), 1);
}

#[test]
fn observer_clear_stops_recording() {
    let heap = ArcMagrGC::new();
    let obs = Arc::new(BarrierObserver::new());
    heap.install_barrier_observer(obs.clone());

    let owner = heap.alloc_object(dummy_type_desc("Owner"), vec![Value::Null], NativeData::None);
    let child = heap.alloc_object(dummy_type_desc("Child"), vec![], NativeData::None);

    heap.write_barrier_field(&owner, 0, &child);
    assert_eq!(obs.count(), 1);

    let removed = heap.clear_barrier_observer();
    assert!(removed.is_some(), "clear returns the removed observer");

    heap.write_barrier_field(&owner, 0, &child);
    assert_eq!(obs.count(), 1, "no further dispatches after clear");
}

// ── 4. Barrier overhead does not perturb GC state ───────────────────────────

#[test]
fn barrier_install_does_not_alter_gc_collect_behavior() {
    // Sanity: installing an observer must not change collect outcomes —
    // observer is metadata-only, no side effects on heap_registry / stats.
    let heap = ArcMagrGC::new();

    // Baseline: build + collect a small cycle without observer.
    let baseline_stats = {
        let a = heap.alloc_object(dummy_type_desc("A"), vec![Value::Null], NativeData::None);
        let b = heap.alloc_object(dummy_type_desc("B"), vec![Value::Null], NativeData::None);
        {
            let Value::Object(a_gc) = &a else { panic!() };
            let Value::Object(b_gc) = &b else { panic!() };
            a_gc.borrow_mut().refs[0] = b.clone();
            b_gc.borrow_mut().refs[0] = a.clone();
        }
        drop(a); drop(b);
        heap.force_collect()
    };
    assert!(baseline_stats.freed_bytes > 0);

    // With observer: same workload, same outcomes (observer just records
    // calls — we never call write_barrier_field in this fixture so events
    // should stay empty; key check is that collect still frees the cycle).
    let obs = Arc::new(BarrierObserver::new());
    heap.install_barrier_observer(obs.clone());
    let a = heap.alloc_object(dummy_type_desc("A2"), vec![Value::Null], NativeData::None);
    let b = heap.alloc_object(dummy_type_desc("B2"), vec![Value::Null], NativeData::None);
    {
        let Value::Object(a_gc) = &a else { panic!() };
        let Value::Object(b_gc) = &b else { panic!() };
        a_gc.borrow_mut().refs[0] = b.clone();
        b_gc.borrow_mut().refs[0] = a.clone();
    }
    drop(a); drop(b);
    let observed_stats = heap.force_collect();
    assert!(observed_stats.freed_bytes > 0,
        "collect still frees cycles with observer installed");
    assert_eq!(obs.count(), 0,
        "observer records 0 events because no write_barrier_field was invoked \
         (cycle wiring happens via borrow_mut, not the barrier API)");

    heap.clear_barrier_observer();
}
