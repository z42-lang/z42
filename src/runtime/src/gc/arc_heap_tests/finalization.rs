use super::*;

// ── 7-1. Finalization (add-custom-allocator P1: sweep-time triggering) ──────
//
// **Behavioral contract change (2026-05-22)**: with the Arc backing
// removed (P1), `GcRef::drop` is a no-op — finalizers no longer fire
// at scope exit. The finalizer now fires only when sweep_phase tombstones
// an unreachable entry, OR when the user calls `Std.GC.Finalize(x)`
// (P2 builtin). Tests migrated from "drop fires" → "force_collect fires".

#[test]
fn finalizer_fires_on_sweep_when_unreachable() {
    // Was `finalizer_fires_on_normal_rc_drop_no_cycle_no_collect`.
    // New contract: finalizer fires at next sweep, not at Drop.
    let heap = ArcMagrGC::new();
    let fired = Arc::new(AtomicUsize::new(0));
    let f = fired.clone();

    {
        let v = heap.alloc_object(dummy_type_desc("Drop"), vec![], NativeData::None);
        heap.register_finalizer(&v, Arc::new(move || {
            f.fetch_add(1, Ordering::SeqCst);
        }));
        // v 出 scope，GcRef::drop is no-op now; entry still alive in region.
    }

    // Trigger sweep — entry is unreachable (not pinned, no root) → tombstoned + finalizer fires.
    heap.force_collect();
    assert_eq!(fired.load(Ordering::SeqCst), 1, "finalizer fires at sweep");
}

#[test]
fn finalizer_one_shot_across_multiple_collects() {
    // Was `finalizer_one_shot_via_drop_then_collect`. New contract:
    // first collect fires the finalizer + tombstones; subsequent
    // collects find nothing for that slot.
    let heap = ArcMagrGC::new();
    let fired = Arc::new(AtomicUsize::new(0));
    let f = fired.clone();

    {
        let v = heap.alloc_object(dummy_type_desc("OneShot"), vec![], NativeData::None);
        heap.register_finalizer(&v, Arc::new(move || {
            f.fetch_add(1, Ordering::SeqCst);
        }));
    }

    heap.collect_cycles();
    heap.collect_cycles();
    heap.collect_cycles();

    assert_eq!(fired.load(Ordering::SeqCst), 1, "one-shot finalizer fires exactly once");
}

#[test]
fn finalizers_pending_reflects_alive_with_finalizer() {
    // Stats.finalizers_pending counts currently-alive entries with
    // a registered finalizer. After P1 the count only decreases via
    // sweep (no drop-time decrement).
    let heap = ArcMagrGC::new();
    let v1 = heap.alloc_object(dummy_type_desc("F1"), vec![], NativeData::None);
    let v2 = heap.alloc_object(dummy_type_desc("F2"), vec![], NativeData::None);
    let _v3_pin = heap.pin_root(
        heap.alloc_object(dummy_type_desc("F3"), vec![], NativeData::None)
    );
    let _v1_pin = heap.pin_root(v1.clone());
    let _v2_pin = heap.pin_root(v2.clone());
    heap.register_finalizer(&v1, Arc::new(|| {}));
    heap.register_finalizer(&v2, Arc::new(|| {}));
    // _v3 没 finalizer
    assert_eq!(heap.stats().finalizers_pending, 2);

    heap.cancel_finalizer(&v1);
    assert_eq!(heap.stats().finalizers_pending, 1);

    // Unpin v2 to make it unreachable; collect; verify finalizer count goes to 0.
    drop(v2);
    heap.unpin_root(_v2_pin);
    heap.force_collect();
    assert_eq!(heap.stats().finalizers_pending, 0,
        "after sweep tombstones v2, no pending finalizers remain");
}

// ── 7. Finalization (Phase 3d: real triggering) ──────────────────────────────

#[test]
fn finalizer_fires_when_object_freed_via_cycle_collect() {
    let heap = ArcMagrGC::new();
    let fired = Arc::new(AtomicUsize::new(0));
    let f = fired.clone();

    let a = heap.alloc_object(dummy_type_desc("F"), vec![Value::Null], NativeData::None);
    let b = heap.alloc_object(dummy_type_desc("G"), vec![Value::Null], NativeData::None);
    {
        let Value::Object(g) = &a else { panic!() };
        g.borrow_mut().refs[0] = b.clone();
        let Value::Object(g) = &b else { panic!() };
        g.borrow_mut().refs[0] = a.clone();
    }
    heap.register_finalizer(&a, Arc::new(move || {
        f.fetch_add(1, Ordering::SeqCst);
    }));
    drop(a);
    drop(b);

    heap.collect_cycles();

    assert_eq!(fired.load(Ordering::SeqCst), 1, "finalizer fires on cycle break");
    assert_eq!(heap.stats().finalizers_pending, 0, "finalizer removed after fire");
}

#[test]
fn finalizer_does_not_fire_when_object_kept_alive() {
    // **add-custom-allocator P1 (2026-05-22)**: under pure-tracing GC
    // (post-add-mark-sweep-collector), Rust-local Value strong refs are
    // NOT roots. To preserve the object across collect we must pin it
    // explicitly; otherwise sweep tombstones it and fires the finalizer.
    let heap = ArcMagrGC::new();
    let fired = Arc::new(AtomicUsize::new(0));
    let f = fired.clone();
    let alive = heap.alloc_object(dummy_type_desc("Alive"), vec![], NativeData::None);
    let _pin = heap.pin_root(alive.clone());
    heap.register_finalizer(&alive, Arc::new(move || {
        f.fetch_add(1, Ordering::SeqCst);
    }));

    heap.collect_cycles();

    assert_eq!(fired.load(Ordering::SeqCst), 0, "finalizer not fired for pinned object");
    assert_eq!(heap.stats().finalizers_pending, 1);
}

// ── add-custom-allocator P2 (2026-05-22): Std.GC.Finalize / finalize_now ──

#[test]
fn finalize_now_fires_registered_finalizer_immediately() {
    let heap = ArcMagrGC::new();
    let fired = Arc::new(AtomicUsize::new(0));
    let f = fired.clone();

    let v = heap.alloc_object(dummy_type_desc("FinalizeNow"), vec![], NativeData::None);
    heap.register_finalizer(&v, Arc::new(move || {
        f.fetch_add(1, Ordering::SeqCst);
    }));

    assert_eq!(fired.load(Ordering::SeqCst), 0,
        "no fire before Finalize call");

    let returned = heap.finalize_now(&v);
    assert!(returned, "finalize_now returns true when finalizer fires");
    assert_eq!(fired.load(Ordering::SeqCst), 1,
        "finalizer fires immediately via finalize_now");
}

#[test]
fn finalize_now_returns_false_without_finalizer() {
    let heap = ArcMagrGC::new();
    let v = heap.alloc_object(dummy_type_desc("NoFin"), vec![], NativeData::None);

    let returned = heap.finalize_now(&v);
    assert!(!returned,
        "finalize_now returns false when no finalizer registered");
}

#[test]
fn finalize_now_returns_false_on_primitive() {
    let heap = ArcMagrGC::new();
    assert!(!heap.finalize_now(&Value::I64(42)));
    assert!(!heap.finalize_now(&Value::Null));
    assert!(!heap.finalize_now(&Value::Str("x".into())));
}

#[test]
fn finalize_now_tombstones_the_entry() {
    let heap = ArcMagrGC::new();
    let v = heap.alloc_object(dummy_type_desc("Tomb"), vec![], NativeData::None);
    let _pin = heap.pin_root(v.clone());

    // Before Finalize: object alive.
    let mut alive_before = 0;
    heap.iterate_live_objects(&mut |_| alive_before += 1);
    assert_eq!(alive_before, 1);

    heap.finalize_now(&v);

    // After Finalize: object tombstoned despite being pinned.
    let mut alive_after = 0;
    heap.iterate_live_objects(&mut |_| alive_after += 1);
    assert_eq!(alive_after, 0,
        "finalize_now tombstones the entry; iterate_alive no longer visits it");
}

#[test]
fn finalize_now_idempotent_on_already_tombstoned() {
    let heap = ArcMagrGC::new();
    let fired = Arc::new(AtomicUsize::new(0));
    let f = fired.clone();
    let v = heap.alloc_object(dummy_type_desc("Idem"), vec![], NativeData::None);
    heap.register_finalizer(&v, Arc::new(move || {
        f.fetch_add(1, Ordering::SeqCst);
    }));

    let first = heap.finalize_now(&v);
    let second = heap.finalize_now(&v);

    assert!(first, "first call fires");
    assert!(!second, "second call does not re-fire (finalizer was taken)");
    assert_eq!(fired.load(Ordering::SeqCst), 1, "one-shot semantics");
}

#[test]
fn finalize_now_works_on_array_value() {
    let heap = ArcMagrGC::new();
    let fired = Arc::new(AtomicUsize::new(0));
    let f = fired.clone();
    let arr = heap.alloc_array(vec![Value::I64(1), Value::I64(2)]);
    heap.register_finalizer(&arr, Arc::new(move || {
        f.fetch_add(1, Ordering::SeqCst);
    }));

    let returned = heap.finalize_now(&arr);
    assert!(returned);
    assert_eq!(fired.load(Ordering::SeqCst), 1);
}

#[test]
fn finalize_now_does_not_disturb_other_objects() {
    let heap = ArcMagrGC::new();
    let target = heap.alloc_object(dummy_type_desc("Target"), vec![], NativeData::None);
    let other  = heap.alloc_object(dummy_type_desc("Other"),  vec![], NativeData::None);
    let _pin1 = heap.pin_root(target.clone());
    let _pin2 = heap.pin_root(other.clone());

    let fired = Arc::new(AtomicUsize::new(0));
    let f = fired.clone();
    heap.register_finalizer(&target, Arc::new(move || {
        f.fetch_add(1, Ordering::SeqCst);
    }));

    heap.finalize_now(&target);

    // `other` is unaffected — still pinned + alive.
    let mut alive = 0;
    heap.iterate_live_objects(&mut |_| alive += 1);
    assert_eq!(alive, 1, "only `target` tombstoned; `other` survives");
    assert_eq!(fired.load(Ordering::SeqCst), 1);
}

#[test]
fn finalizer_is_one_shot_after_fire() {
    let heap = ArcMagrGC::new();
    let fired = Arc::new(AtomicUsize::new(0));
    let f = fired.clone();
    let a = heap.alloc_object(dummy_type_desc("F"), vec![Value::Null], NativeData::None);
    {
        let Value::Object(g) = &a else { panic!() };
        g.borrow_mut().refs[0] = a.clone();
    }
    heap.register_finalizer(&a, Arc::new(move || {
        f.fetch_add(1, Ordering::SeqCst);
    }));
    drop(a);

    heap.collect_cycles();
    heap.collect_cycles();  // second call should not re-fire

    assert_eq!(fired.load(Ordering::SeqCst), 1, "one-shot");
}

// ── 7-existing. Finalization counter tests (Phase 3a baseline) ───────────────

#[test]
fn register_finalizer_increments_pending_count() {
    let heap = ArcMagrGC::new();
    let v = heap.alloc_array(vec![]);
    heap.register_finalizer(&v, Arc::new(|| {}));
    assert_eq!(heap.stats().finalizers_pending, 1);
}

#[test]
fn cancel_finalizer_decrements_pending_count() {
    let heap = ArcMagrGC::new();
    let v = heap.alloc_array(vec![]);
    heap.register_finalizer(&v, Arc::new(|| {}));
    heap.cancel_finalizer(&v);
    assert_eq!(heap.stats().finalizers_pending, 0);
}

#[test]
fn register_finalizer_on_atomic_value_is_noop() {
    let heap = ArcMagrGC::new();
    heap.register_finalizer(&Value::I64(1), Arc::new(|| {}));
    assert_eq!(heap.stats().finalizers_pending, 0);
}

