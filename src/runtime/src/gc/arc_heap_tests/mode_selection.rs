//! GcMode + set_mode dispatch tests: the API, the default, and that a mode switch
//! takes effect at the next collect.

use super::*;
use crate::gc::{GcMode, MagrGC};

#[test]
fn mode_default_is_generational_mark_sweep() {
    // flip-gc-default-to-generational (2026-09-10).
    let heap = ArcMagrGC::new();
    assert_eq!(heap.mode(), GcMode::GenerationalMarkSweep);
}

#[test]
fn set_mode_changes_observable_mode() {
    let heap = ArcMagrGC::new();
    assert_eq!(heap.mode(), GcMode::default());
    heap.set_mode(GcMode::StwMarkSweep);
    assert_eq!(heap.mode(), GcMode::StwMarkSweep);
    heap.set_mode(GcMode::GenerationalMarkSweep);
    assert_eq!(heap.mode(), GcMode::GenerationalMarkSweep);
}

#[test]
fn set_mode_then_collect_uses_the_new_mode() {
    // After switching to STW, a forced collect is a whole-heap mark-sweep: an unrooted
    // cycle is freed, and the mode stays as set.
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::StwMarkSweep);

    // Allocate + drop a small cycle.
    let a = heap.alloc_object(dummy_type_desc("A"), vec![Value::Null], NativeData::None);
    let b = heap.alloc_object(dummy_type_desc("B"), vec![Value::Null], NativeData::None);
    {
        let Value::Object(a_gc) = &a else { panic!() };
        let Value::Object(b_gc) = &b else { panic!() };
        a_gc.borrow_mut().refs_mut_raw()[0] = b.clone();
        b_gc.borrow_mut().refs_mut_raw()[0] = a.clone();
    }
    drop(a); drop(b);

    let stats = heap.force_collect();
    assert!(stats.freed_bytes > 0, "STW collect frees the unrooted cycle");

    // Mode remains as set.
    assert_eq!(heap.mode(), GcMode::StwMarkSweep);
}
