use super::*;

// ── 8. Weak references ───────────────────────────────────────────────────────

#[test]
fn make_weak_on_object_succeeds() {
    let heap = ArcMagrGC::new();
    let v = heap.alloc_object(dummy_type_desc("Foo"), vec![], NativeData::None);
    assert!(heap.make_weak(&v).is_some());
}

#[test]
fn make_weak_on_array_succeeds() {
    let heap = ArcMagrGC::new();
    let v = heap.alloc_array(vec![]);
    assert!(heap.make_weak(&v).is_some());
}

#[test]
fn make_weak_on_atomic_returns_none() {
    let heap = ArcMagrGC::new();
    assert!(heap.make_weak(&Value::I64(1)).is_none());
    assert!(heap.make_weak(&Value::Str("x".into())).is_none());
    assert!(heap.make_weak(&Value::Null).is_none());
    assert!(heap.make_weak(&Value::Bool(true)).is_none());
}

#[test]
fn upgrade_weak_succeeds_while_strong_alive() {
    let heap = ArcMagrGC::new();
    let v = heap.alloc_array(vec![Value::I64(42)]);
    let w = heap.make_weak(&v).unwrap();
    let upgraded = heap.upgrade_weak(&w).unwrap();
    let (Value::Array(a), Value::Array(b)) = (&v, &upgraded) else { panic!() };
    assert!(crate::gc::ArrRef::ptr_eq(a, b));
}

#[test]
fn upgrade_weak_fails_after_sweep_tombstones() {
    // **add-custom-allocator P1 **: WeakRef no longer
    // invalidates at Rust-scope-exit (ArrRef::drop is now no-op). It
    // invalidates when sweep_phase tombstones the entry. Was named
    // `upgrade_weak_fails_after_strong_dropped` — semantics updated.
    let heap = ArcMagrGC::new();
    let w = {
        let v = heap.alloc_array(vec![]);
        heap.make_weak(&v).unwrap()
    };
    // Force a collect — `v` was unrooted, so sweep tombstones it.
    heap.force_collect();
    assert!(heap.upgrade_weak(&w).is_none(),
        "Weak invalidates after sweep tombstones the entry");
}

// ── 8.5 Handle table（reorganize-gc-stdlib）─────────────────────

#[test]
fn handle_alloc_returns_nonzero_for_object() {
    let heap = ArcMagrGC::new();
    let v = heap.alloc_object(dummy_type_desc("H"), vec![], NativeData::None);
    let slot = heap.handle_alloc(&v, GcHandleKind::Strong);
    assert!(slot >= 1, "slot 0 is reserved as 'unallocated' sentinel");
}

#[test]
fn handle_alloc_atomic_weak_returns_zero_slot() {
    let heap = ArcMagrGC::new();
    // Atomic values can't be weakly referenced.
    assert_eq!(heap.handle_alloc(&Value::I64(42), GcHandleKind::Weak), 0);
    assert_eq!(heap.handle_alloc(&Value::Str("x".into()), GcHandleKind::Weak), 0);
    assert_eq!(heap.handle_alloc(&Value::Bool(true), GcHandleKind::Weak), 0);
    assert!(!heap.handle_is_alloc(0));
}

#[test]
fn handle_alloc_null_returns_zero_slot() {
    let heap = ArcMagrGC::new();
    assert_eq!(heap.handle_alloc(&Value::Null, GcHandleKind::Strong), 0);
    assert_eq!(heap.handle_alloc(&Value::Null, GcHandleKind::Weak), 0);
}

#[test]
fn handle_alloc_atomic_strong_stores_value() {
    let heap = ArcMagrGC::new();
    let slot = heap.handle_alloc(&Value::I64(99), GcHandleKind::Strong);
    assert!(slot >= 1);
    assert_eq!(heap.handle_target(slot), Some(Value::I64(99)));
    assert_eq!(heap.handle_kind(slot), Some(GcHandleKind::Strong));
}

#[test]
fn handle_strong_anchors_after_external_drop() {
    let heap = ArcMagrGC::new();
    let slot = {
        let v = heap.alloc_object(dummy_type_desc("Anchor"), vec![], NativeData::None);
        heap.handle_alloc(&v, GcHandleKind::Strong)
        // `v` dropped here — only the strong slot keeps the target alive
    };
    assert!(heap.handle_is_alloc(slot));
    let recovered = heap.handle_target(slot).expect("strong slot anchors target");
    assert!(matches!(recovered, Value::Object(_)));
}

#[test]
fn handle_weak_clears_after_sweep_collects_target() {
    // **add-custom-allocator P1 **: weak handle target
    // invalidates when sweep tombstones the entry, not at scope exit
    // of the original strong ref (ArrRef::drop is now no-op).
    let heap = ArcMagrGC::new();
    let slot = {
        let v = heap.alloc_object(dummy_type_desc("WeakTgt"), vec![], NativeData::None);
        heap.handle_alloc(&v, GcHandleKind::Weak)
    };
    // Force collect → target unrooted → tombstoned → weak resolves to None.
    heap.force_collect();
    assert!(heap.handle_is_alloc(slot));
    assert_eq!(heap.handle_target(slot), None,
        "weak handle target gone after sweep tombstones");
    assert_eq!(heap.handle_kind(slot), Some(GcHandleKind::Weak));
}

#[test]
fn handle_free_invalidates_slot() {
    let heap = ArcMagrGC::new();
    let v = heap.alloc_object(dummy_type_desc("F"), vec![], NativeData::None);
    let slot = heap.handle_alloc(&v, GcHandleKind::Strong);
    assert!(heap.handle_is_alloc(slot));
    heap.handle_free(slot);
    assert!(!heap.handle_is_alloc(slot));
    assert_eq!(heap.handle_target(slot), None);
    assert_eq!(heap.handle_kind(slot), None);
}

#[test]
fn handle_free_then_realloc_reuses_slot() {
    let heap = ArcMagrGC::new();
    let v1 = heap.alloc_object(dummy_type_desc("R1"), vec![], NativeData::None);
    let slot1 = heap.handle_alloc(&v1, GcHandleKind::Strong);
    heap.handle_free(slot1);

    let v2 = heap.alloc_object(dummy_type_desc("R2"), vec![], NativeData::None);
    let slot2 = heap.handle_alloc(&v2, GcHandleKind::Strong);
    assert_eq!(slot1, slot2, "freed slot must be reused via free_list LIFO");
}

#[test]
fn handle_free_idempotent_and_safe_for_zero() {
    let heap = ArcMagrGC::new();
    heap.handle_free(0); // sentinel — must be no-op
    heap.handle_free(99_999); // out of range — must be no-op
    let v = heap.alloc_object(dummy_type_desc("I"), vec![], NativeData::None);
    let slot = heap.handle_alloc(&v, GcHandleKind::Strong);
    heap.handle_free(slot);
    heap.handle_free(slot); // double-free — must be no-op
    assert!(!heap.handle_is_alloc(slot));
}

#[test]
fn handle_kind_distinguishes_strong_and_weak() {
    let heap = ArcMagrGC::new();
    let v = heap.alloc_object(dummy_type_desc("K"), vec![], NativeData::None);
    let s = heap.handle_alloc(&v, GcHandleKind::Strong);
    let w = heap.handle_alloc(&v, GcHandleKind::Weak);
    assert_eq!(heap.handle_kind(s), Some(GcHandleKind::Strong));
    assert_eq!(heap.handle_kind(w), Some(GcHandleKind::Weak));
}
