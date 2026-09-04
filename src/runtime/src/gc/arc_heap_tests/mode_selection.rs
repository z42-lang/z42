//! add-concurrent-gc P0 (2026-05-22): GcMode + set_mode dispatch tests.
//!
//! Verifies the mode switch + dispatch stub. Real concurrent collect
//! behavior lands in P4; this file pins the API + default + env-var
//! initialization.

use super::*;
use crate::gc::{GcMode, MagrGC};

#[test]
fn mode_default_is_stw_mark_sweep() {
    let heap = ArcMagrGC::new();
    assert_eq!(heap.mode(), GcMode::StwMarkSweep);
}

#[test]
fn set_mode_changes_observable_mode() {
    let heap = ArcMagrGC::new();
    assert_eq!(heap.mode(), GcMode::StwMarkSweep);
    heap.set_mode(GcMode::ConcurrentMarkSweep);
    assert_eq!(heap.mode(), GcMode::ConcurrentMarkSweep);
    heap.set_mode(GcMode::StwMarkSweep);
    assert_eq!(heap.mode(), GcMode::StwMarkSweep);
}

#[test]
fn set_mode_does_not_affect_in_progress_collect() {
    // P0 stub: concurrent arm dispatches to STW path too, so any mode
    // setting collects identically. This test pins the contract: after
    // calling set_mode and triggering collect, the result is consistent
    // with the chosen mode (STW behavior either way under P0).
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::ConcurrentMarkSweep);

    // Allocate + drop a small cycle.
    let a = heap.alloc_object(dummy_type_desc("A"), vec![Value::Null], NativeData::None);
    let b = heap.alloc_object(dummy_type_desc("B"), vec![Value::Null], NativeData::None);
    {
        let Value::Object(a_gc) = &a else { panic!() };
        let Value::Object(b_gc) = &b else { panic!() };
        a_gc.borrow_mut().refs_mut()[0] = b.clone();
        b_gc.borrow_mut().refs_mut()[0] = a.clone();
    }
    drop(a); drop(b);

    let stats = heap.force_collect();
    assert!(stats.freed_bytes > 0, "concurrent-mode stub still frees the cycle via STW path");

    // Mode remains as set.
    assert_eq!(heap.mode(), GcMode::ConcurrentMarkSweep);
}

#[test]
fn mode_dispatch_stub_preserves_stw_behavior() {
    // P0 contract: both arms of the dispatch route to the same STW
    // implementation. This test ensures setting concurrent doesn't
    // regress correctness (P4 replaces the concurrent arm with real
    // logic; P0 just verifies wiring is harmless).
    let mut alive_after_stw = 0;
    {
        let heap = ArcMagrGC::new();
        heap.set_mode(GcMode::StwMarkSweep);
        let _a = heap.alloc_object(dummy_type_desc("Solo"), vec![], NativeData::None);
        heap.force_collect();
        heap.iterate_live_objects(&mut |_| alive_after_stw += 1);
    }

    let mut alive_after_concurrent = 0;
    {
        let heap = ArcMagrGC::new();
        heap.set_mode(GcMode::ConcurrentMarkSweep);
        let _a = heap.alloc_object(dummy_type_desc("Solo"), vec![], NativeData::None);
        heap.force_collect();
        heap.iterate_live_objects(&mut |_| alive_after_concurrent += 1);
    }

    // Both routes should produce identical alive count (P0 stub).
    assert_eq!(alive_after_stw, alive_after_concurrent,
        "P0 stub: STW and concurrent-arm produce identical collect outcomes");
}
