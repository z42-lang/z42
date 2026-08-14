use super::*;

// ── Cycle collection (Phase 3c) ──────────────────────────────────────────────

fn alive_count(heap: &ArcMagrGC) -> usize {
    let mut n = 0;
    heap.iterate_live_objects(&mut |_| n += 1);
    n
}

#[test]
fn simple_two_node_cycle_is_freed_after_collect() {
    let heap = ArcMagrGC::new();
    let a = heap.alloc_object(dummy_type_desc("A"), vec![Value::Null], NativeData::None);
    let b = heap.alloc_object(dummy_type_desc("B"), vec![Value::Null], NativeData::None);
    {
        let Value::Object(a_gc) = &a else { panic!() };
        let Value::Object(b_gc) = &b else { panic!() };
        a_gc.borrow_mut().refs[0] = b.clone();
        b_gc.borrow_mut().refs[0] = a.clone();
    }
    drop(a);
    drop(b);
    assert_eq!(alive_count(&heap), 2, "cycle keeps both alive before collect");

    heap.collect_cycles();

    assert_eq!(alive_count(&heap), 0, "cycle collector frees both nodes");
}

#[test]
fn self_reference_cycle_is_freed() {
    let heap = ArcMagrGC::new();
    let a = heap.alloc_object(dummy_type_desc("Self"), vec![Value::Null], NativeData::None);
    {
        let Value::Object(a_gc) = &a else { panic!() };
        a_gc.borrow_mut().refs[0] = a.clone();
    }
    drop(a);
    assert_eq!(alive_count(&heap), 1);

    heap.collect_cycles();

    assert_eq!(alive_count(&heap), 0, "self-reference cycle freed");
}

#[test]
fn cycle_with_external_user_ref_is_not_broken_yet() {
    // **add-mark-sweep-collector P3 (2026-05-21)**: under mark-sweep,
    // "external user ref" alone does NOT keep an object alive. The mark
    // phase only walks pinned roots + external_root_scanner output;
    // Rust-local Value strong refs are invisible to the GC. To preserve
    // a Value across `collect_cycles`, embedders MUST `pin_root` it.
    // Trial-deletion's previous behavior (Arc::strong_count > 1 implicitly
    // preserved cycle nodes) is gone — pure tracing semantics.
    let heap = ArcMagrGC::new();
    let a = heap.alloc_object(dummy_type_desc("A"), vec![Value::Null], NativeData::None);
    let b = heap.alloc_object(dummy_type_desc("B"), vec![Value::Null], NativeData::None);
    {
        let Value::Object(a_gc) = &a else { panic!() };
        let Value::Object(b_gc) = &b else { panic!() };
        a_gc.borrow_mut().refs[0] = b.clone();
        b_gc.borrow_mut().refs[0] = a.clone();
    }
    // Pin a explicitly — this is the new contract for "I want this to
    // survive collect_cycles even though my code holds a strong ref".
    let user_pin = heap.pin_root(a.clone());
    drop(a);
    drop(b);

    heap.collect_cycles();

    // Pinned root keeps a alive → cycle keeps b alive.
    assert_eq!(alive_count(&heap), 2, "pinned root keeps cycle alive");

    // Unpin → next collect frees the cycle.
    heap.unpin_root(user_pin);
    heap.collect_cycles();
    assert_eq!(alive_count(&heap), 0, "after unpin + collect, mark-sweep frees the cycle");
}

#[test]
fn pinned_root_cycle_is_not_broken() {
    let heap = ArcMagrGC::new();
    let a = heap.alloc_object(dummy_type_desc("A"), vec![Value::Null], NativeData::None);
    let b = heap.alloc_object(dummy_type_desc("B"), vec![Value::Null], NativeData::None);
    {
        let Value::Object(a_gc) = &a else { panic!() };
        let Value::Object(b_gc) = &b else { panic!() };
        a_gc.borrow_mut().refs[0] = b.clone();
        b_gc.borrow_mut().refs[0] = a.clone();
    }
    let _root = heap.pin_root(a.clone());

    heap.collect_cycles();

    // a / b 都是 reachable from pinned root → 不被破坏
    assert_eq!(alive_count(&heap), 2, "pinned cycle survives collect");
    let Value::Object(a_gc) = &a else { panic!() };
    // a.refs[0] 还是 b 的引用，不是 Null
    assert!(matches!(a_gc.borrow().refs[0], Value::Object(_)));
}

#[test]
fn unrelated_alive_object_is_not_affected_by_collect() {
    // **add-mark-sweep-collector P3 (2026-05-21)**: mark-sweep requires
    // the object to be reachable from a pinned root (or external scanner
    // output) to survive. The Rust local `_alive` is NOT a root by
    // itself. Pin it explicitly to preserve through collect.
    let heap = ArcMagrGC::new();
    let alive = heap.alloc_object(dummy_type_desc("Alive"), vec![Value::I64(42)], NativeData::None);
    let pin = heap.pin_root(alive.clone());

    heap.collect_cycles();

    assert_eq!(alive_count(&heap), 1, "pinned object survives collect");
    let Value::Object(gc) = &alive else { panic!() };
    assert_eq!(gc.borrow().refs[0], Value::I64(42), "data intact");

    heap.unpin_root(pin);
}

#[test]
fn multiple_disjoint_cycles_all_freed() {
    let heap = ArcMagrGC::new();
    // 第一个环 a-b
    let a = heap.alloc_object(dummy_type_desc("A1"), vec![Value::Null], NativeData::None);
    let b = heap.alloc_object(dummy_type_desc("B1"), vec![Value::Null], NativeData::None);
    {
        let Value::Object(g) = &a else { panic!() };
        g.borrow_mut().refs[0] = b.clone();
        let Value::Object(g) = &b else { panic!() };
        g.borrow_mut().refs[0] = a.clone();
    }
    // 第二个环 c-d
    let c = heap.alloc_object(dummy_type_desc("C2"), vec![Value::Null], NativeData::None);
    let d = heap.alloc_object(dummy_type_desc("D2"), vec![Value::Null], NativeData::None);
    {
        let Value::Object(g) = &c else { panic!() };
        g.borrow_mut().refs[0] = d.clone();
        let Value::Object(g) = &d else { panic!() };
        g.borrow_mut().refs[0] = c.clone();
    }
    drop(a); drop(b); drop(c); drop(d);
    assert_eq!(alive_count(&heap), 4);

    heap.collect_cycles();

    assert_eq!(alive_count(&heap), 0, "both cycles independently freed");
}

#[test]
fn collect_cycles_freed_bytes_observable() {
    let heap = ArcMagrGC::new();
    let a = heap.alloc_object(dummy_type_desc("A"), vec![Value::Null], NativeData::None);
    let b = heap.alloc_object(dummy_type_desc("B"), vec![Value::Null], NativeData::None);
    {
        let Value::Object(g) = &a else { panic!() };
        g.borrow_mut().refs[0] = b.clone();
        let Value::Object(g) = &b else { panic!() };
        g.borrow_mut().refs[0] = a.clone();
    }
    drop(a);
    drop(b);

    let used_before = heap.used_bytes();
    let stats = heap.force_collect();
    assert_eq!(stats.kind, Some(GcKind::Full));
    assert!(stats.freed_bytes > 0, "force_collect should report freed bytes for cycle");
    let used_after = heap.used_bytes();
    assert!(used_after < used_before, "used_bytes decreases after cycle collection");
}

#[test]
fn region_prunes_dropped_objects_after_collect() {
    // **add-custom-allocator P1 (2026-05-22)**: heap_registry removed;
    // sweep tombstones unreachable entries in the region directly.
    // GcRef::drop is no-op so the scope-exit alone doesn't prune —
    // we must force a collect.
    let heap = ArcMagrGC::new();
    {
        let _ephemeral = heap.alloc_array(vec![]);
        // _ephemeral 出 scope；GcRef::drop is no-op. Entry stays alive
        // in region until sweep tombstones it.
    }
    heap.force_collect();
    let mut count = 0;
    heap.iterate_live_objects(&mut |_| count += 1);
    assert_eq!(count, 0, "sweep tombstones unrooted entry → iterate_live_objects sees none");
}

#[test]
fn snapshot_includes_pinned_root_object() {
    let heap = ArcMagrGC::new();
    let v = heap.alloc_object(dummy_type_desc("Foo"), vec![], NativeData::None);
    let _h = heap.pin_root(v);
    let snap = heap.take_snapshot();
    assert!(snap.total_objects >= 1);
    assert!(snap.objects_by_type.contains_key("Foo"));
}

#[test]
fn snapshot_traverses_nested_objects() {
    let heap = ArcMagrGC::new();
    let inner1 = heap.alloc_object(dummy_type_desc("Inner"), vec![], NativeData::None);
    let inner2 = heap.alloc_object(dummy_type_desc("Inner"), vec![], NativeData::None);
    let outer  = heap.alloc_object(
        dummy_type_desc("Outer"),
        vec![inner1, inner2],
        NativeData::None,
    );
    let _h = heap.pin_root(outer);
    let snap = heap.take_snapshot();
    assert!(snap.total_objects >= 3);
    assert_eq!(snap.objects_by_type.get("Outer").map(|s| s.count), Some(1));
    assert_eq!(snap.objects_by_type.get("Inner").map(|s| s.count), Some(2));
}

#[test]
fn iterate_live_objects_visits_root_reachable() {
    let heap = ArcMagrGC::new();
    let elems = (0..5)
        .map(|i| heap.alloc_object(dummy_type_desc("Foo"), vec![Value::I64(i)], NativeData::None))
        .collect::<Vec<_>>();
    let arr = heap.alloc_array(elems);
    let _h  = heap.pin_root(arr);

    let mut count = 0;
    heap.iterate_live_objects(&mut |_v| count += 1);
    // 1 array + 5 objects
    assert_eq!(count, 6);
}

#[test]
fn iterate_live_objects_dedupes_cycle() {
    let heap = ArcMagrGC::new();
    // 自引用 cycle：obj.refs[0] = obj 自己（通过 wrap-by-clone）
    let obj = heap.alloc_object(dummy_type_desc("Cycle"), vec![Value::Null], NativeData::None);
    let Value::Object(rc) = &obj else { panic!() };
    rc.borrow_mut().refs[0] = obj.clone();
    let _h = heap.pin_root(obj);

    let mut count = 0;
    heap.iterate_live_objects(&mut |_v| count += 1);
    // visited dedup → 仅 1 次
    assert_eq!(count, 1);
}

