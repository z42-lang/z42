use super::*;

// ── 9. Event observers ───────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct CountingObserver {
    count: AtomicUsize,
}
impl GcObserver for CountingObserver {
    fn on_event(&self, _event: &GcEvent) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn add_observer_increments_count() {
    let heap = ArcMagrGC::default();
    heap.add_observer(Arc::new(CountingObserver::default()));
    assert_eq!(heap.stats().observers, 1);
}

#[test]
fn observer_receives_before_and_after_on_collect_cycles() {
    let heap = ArcMagrGC::default();
    let obs  = Arc::new(CountingObserver::default());
    heap.add_observer(obs.clone());
    heap.collect_cycles();
    // 2 events: BeforeCollect + AfterCollect
    assert_eq!(obs.count.load(Ordering::SeqCst), 2);
}

#[test]
fn observer_receives_before_and_after_on_force_collect() {
    let heap = ArcMagrGC::default();
    let obs  = Arc::new(CountingObserver::default());
    heap.add_observer(obs.clone());
    heap.force_collect();
    assert_eq!(obs.count.load(Ordering::SeqCst), 2);
}

#[test]
fn remove_observer_stops_event_delivery() {
    let heap = ArcMagrGC::default();
    let obs  = Arc::new(CountingObserver::default());
    let id = heap.add_observer(obs.clone());
    heap.remove_observer(id);
    heap.collect_cycles();
    assert_eq!(obs.count.load(Ordering::SeqCst), 0);
    assert_eq!(heap.stats().observers, 0);
}

#[derive(Debug, Default)]
struct OomObserver {
    oom_seen: AtomicUsize,
}
impl GcObserver for OomObserver {
    fn on_event(&self, event: &GcEvent) {
        if matches!(event, GcEvent::OutOfMemory { .. }) {
            self.oom_seen.fetch_add(1, Ordering::SeqCst);
        }
    }
}

#[test]
fn oom_event_fires_when_used_exceeds_limit() {
    let heap = ArcMagrGC::default();
    let obs  = Arc::new(OomObserver::default());
    heap.add_observer(obs.clone());
    heap.set_max_heap_bytes(Some(8));  // tiny limit
    let _ = heap.alloc_array(vec![Value::I64(0); 100]);
    assert!(obs.oom_seen.load(Ordering::SeqCst) >= 1);
}

// ── 10. Profiler ─────────────────────────────────────────────────────────────

#[test]
fn alloc_sampler_fires_for_each_alloc() {
    let heap = ArcMagrGC::default();
    let counter = Arc::new(AtomicUsize::new(0));
    let c = counter.clone();
    heap.set_alloc_sampler(Some(Arc::new(move |_sample| {
        c.fetch_add(1, Ordering::SeqCst);
    })));
    let _ = heap.alloc_array(vec![]);
    let _ = heap.alloc_array(vec![Value::I64(1)]);
    let _ = heap.alloc_object(dummy_type_desc("Foo"), vec![], NativeData::None);
    assert_eq!(counter.load(Ordering::SeqCst), 3);
}

#[test]
fn set_alloc_sampler_none_clears_sampler() {
    let heap = ArcMagrGC::default();
    let counter = Arc::new(AtomicUsize::new(0));
    let c = counter.clone();
    heap.set_alloc_sampler(Some(Arc::new(move |_| {
        c.fetch_add(1, Ordering::SeqCst);
    })));
    heap.set_alloc_sampler(None);
    let _ = heap.alloc_array(vec![]);
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}

#[test]
fn snapshot_coverage_is_full_after_phase_3b_registry() {
    // Phase 3b: heap registry 升级 coverage 到 Full（不依赖 host pin）。
    let heap = ArcMagrGC::default();
    let snap = heap.take_snapshot();
    assert_eq!(snap.coverage, SnapshotCoverage::Full);
}

#[test]
fn snapshot_includes_unpinned_alive_object() {
    // Phase 3b: 没 pin 也能在 snapshot 里看到（只要还存活）。
    let heap = ArcMagrGC::default();
    let _alive = heap.alloc_object(dummy_type_desc("UnpinnedFoo"), vec![], NativeData::None);
    let snap = heap.take_snapshot();
    assert!(snap.objects_by_type.contains_key("UnpinnedFoo"));
    assert!(snap.total_objects >= 1);
}

#[test]
fn iterate_live_objects_full_coverage_includes_unpinned() {
    // Phase 3b: alloc 但没 pin 的对象，iterate_live_objects 也能找到。
    let heap = ArcMagrGC::default();
    let _a = heap.alloc_array(vec![]);
    let _b = heap.alloc_object(dummy_type_desc("Foo"), vec![], NativeData::None);
    let mut count = 0;
    heap.iterate_live_objects(&mut |_| count += 1);
    assert_eq!(count, 2);
}

