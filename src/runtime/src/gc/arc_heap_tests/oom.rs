use super::*;

// ── 6-1. Strict OOM rejection (Phase 3-OOM) ──────────────────────────────────

#[test]
fn strict_oom_off_by_default_no_rejection() {
    // 默认行为不变：alloc 越界仍成功（仅 fire 事件）。
    //
    // **add-mark-sweep-collector P3 **: under mark-sweep,
    // the tight `max_bytes=8` setting trips auto-collect inline during
    // `alloc_array` (the flag-deferred path is only active when a
    // VmContext wires the external flag; here we use a raw ArcMagrGC).
    // The un-pinned `v` would then be swept (Rust local strong refs no
    // longer count as roots — see migration note in
    // `cycle_collection.rs`). Pause GC for the test so auto-collect
    // doesn't fire; this still exercises the strict-mode-off path
    // (alloc returns non-Null even though over limit).
    let heap = ArcMagrGC::new();
    heap.set_max_heap_bytes(Some(8));  // 极小 limit
    heap.pause();
    let v = heap.alloc_array(vec![Value::I64(0); 100]);
    assert!(matches!(v, Value::Array(_)), "no rejection without strict mode");
    assert!(heap.used_bytes() > 0);
    heap.resume();
}

#[test]
fn strict_oom_alloc_returns_null_when_over_limit() {
    let heap = ArcMagrGC::new();
    heap.set_max_heap_bytes(Some(64));  // 紧 limit
    heap.set_strict_oom(true);
    let v = heap.alloc_array(vec![Value::I64(0); 100]);  // 显然超
    assert!(matches!(v, Value::Null), "strict mode rejects → Null, got {:?}", v);
}

#[test]
fn strict_oom_does_not_bump_stats_or_registry() {
    let heap = ArcMagrGC::new();
    heap.set_max_heap_bytes(Some(64));
    heap.set_strict_oom(true);

    let stats_before = heap.stats();
    let mut alive_before = 0;
    heap.iterate_live_objects(&mut |_| alive_before += 1);

    let _ = heap.alloc_array(vec![Value::I64(0); 100]);  // 拒绝

    let stats_after = heap.stats();
    let mut alive_after = 0;
    heap.iterate_live_objects(&mut |_| alive_after += 1);

    assert_eq!(stats_after.allocations, stats_before.allocations,
        "rejected alloc should not bump allocations counter");
    assert_eq!(stats_after.used_bytes, stats_before.used_bytes,
        "rejected alloc should not bump used_bytes");
    assert_eq!(alive_after, alive_before,
        "rejected alloc should not enter heap_registry");
}

#[test]
fn strict_oom_event_fires_on_rejection() {
    #[derive(Debug, Default)]
    struct OomCounter { count: AtomicUsize }
    impl GcObserver for OomCounter {
        fn on_event(&self, e: &GcEvent) {
            if matches!(e, GcEvent::OutOfMemory { .. }) {
                self.count.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
    let heap = ArcMagrGC::new();
    heap.set_max_heap_bytes(Some(64));
    heap.set_strict_oom(true);
    let obs = Arc::new(OomCounter::default());
    heap.add_observer(obs.clone());

    let _ = heap.alloc_array(vec![Value::I64(0); 100]);  // rejected

    assert!(obs.count.load(Ordering::SeqCst) >= 1, "OOM event fires on strict reject");
}

#[test]
fn strict_oom_under_limit_succeeds_normally() {
    let heap = ArcMagrGC::new();
    heap.set_max_heap_bytes(Some(100_000));  // 大 limit
    heap.set_strict_oom(true);
    let v = heap.alloc_array(vec![]);  // 微小 alloc
    assert!(matches!(v, Value::Array(_)), "small alloc under limit succeeds in strict mode");
}

#[test]
fn strict_oom_can_be_disabled_at_runtime() {
    let heap = ArcMagrGC::new();
    heap.set_max_heap_bytes(Some(64));
    heap.set_strict_oom(true);
    let v1 = heap.alloc_array(vec![Value::I64(0); 100]);
    assert!(matches!(v1, Value::Null));

    heap.set_strict_oom(false);  // 关闭 strict
    let v2 = heap.alloc_array(vec![Value::I64(0); 100]);
    assert!(matches!(v2, Value::Array(_)), "after disabling strict, alloc succeeds");
}

