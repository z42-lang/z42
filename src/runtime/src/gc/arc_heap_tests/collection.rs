use super::*;

// ── 5. Collection control ────────────────────────────────────────────────────

#[test]
fn force_collect_returns_a_kind() {
    let heap = ArcMagrGC::new();
    let stats = heap.force_collect();
    // Which kind depends on the mode (see `force_collect_reports_*` below).
    assert!(stats.kind.is_some());
    assert_eq!(stats.freed_bytes, 0);
    assert_eq!(heap.stats().gc_cycles, 1);
}

#[test]
fn pause_skips_force_collect() {
    let heap = ArcMagrGC::new();
    heap.pause();
    let stats = heap.force_collect();
    assert_eq!(stats.kind, None);  // skipped
    assert_eq!(heap.stats().gc_cycles, 0);  // not incremented
}

#[test]
fn resume_after_pause_re_enables_collect() {
    let heap = ArcMagrGC::new();
    heap.pause();
    heap.resume();
    let stats = heap.force_collect();
    assert!(stats.kind.is_some());
}

#[test]
fn nested_pause_requires_matching_resume() {
    let heap = ArcMagrGC::new();
    heap.pause();
    heap.pause();
    heap.resume();
    // Still paused after one resume
    assert_eq!(heap.force_collect().kind, None);
    heap.resume();
    // Now unpaused
    assert!(heap.force_collect().kind.is_some());
}

#[test]
fn collect_cycles_increments_gc_cycles() {
    let heap = ArcMagrGC::new();
    heap.collect_cycles();
    heap.collect_cycles();
    assert_eq!(heap.stats().gc_cycles, 2);
}

#[test]
fn stats_collect_does_not_change_counters() {
    let heap = ArcMagrGC::new();
    let _ = heap.alloc_array(vec![]);
    let before = heap.stats();
    heap.collect();  // default no-op
    assert_eq!(heap.stats(), before);
}

// ── Auto-collect on memory pressure (Phase 3d) ───────────────────────────────

#[test]
fn auto_collect_triggers_when_over_threshold() {
    let heap = ArcMagrGC::new();
    heap.set_max_heap_bytes(Some(2_000));  // 阈值 1800 (90%)
    let gc_before = heap.stats().gc_cycles;

    // 反复 alloc 直到 used 越过 90% 阈值
    let mut keep_alive: Vec<Value> = Vec::new();
    for _ in 0..30 {
        keep_alive.push(heap.alloc_array(vec![Value::I64(0); 8]));
    }
    let gc_after = heap.stats().gc_cycles;
    assert!(gc_after > gc_before, "auto-collect should fire when heap >90% limit");
}

/// 一次回收之后，紧跟的小分配不得再触发一次 —— 闸门问的是「自上次回收以来长了多少」。
///
/// **arm-gc-by-default (2026-09-09)**：闸门从「预算的 10%」换成了**相对增长**
/// （`live × ALLOWANCE_HEAP_RATIO`，下界 `nursery × ALLOWANCE_NURSERY_RATIO`），
/// 所以这里显式把 nursery 调小来控制算术，而不是靠预算的百分比。
#[test]
fn auto_collect_throttled_by_growth_delta() {
    let heap = ArcMagrGC::new();
    heap.set_nursery_bytes_for_test(1024);
    heap.set_max_heap_bytes(Some(10_000));

    // 一次性 alloc 跨过增长闸门，触发一次回收。
    let _big = heap.alloc_array(vec![Value::I64(0); 4000]);
    let gc1 = heap.stats().gc_cycles;
    assert!(gc1 > 0, "crossing the growth gate must collect (got {gc1})");

    // 紧跟一个空数组：几乎没长，不应再触发。
    let _small = heap.alloc_array(vec![]);
    let gc2 = heap.stats().gc_cycles;
    assert_eq!(gc1, gc2, "small alloc within the growth gate does not retrigger");
}


// ── P0-16 (gc-strategy-stopgap): a report names the collection that actually ran ─────────────

#[derive(Debug, Default)]
struct KindRecorder(parking_lot::Mutex<Vec<GcKind>>);
impl GcObserver for KindRecorder {
    fn on_event(&self, event: &GcEvent) {
        if let GcEvent::AfterCollect { kind, .. } = event {
            self.0.lock().push(*kind);
        }
    }
}

fn minor_major(heap: &ArcMagrGC) -> (u64, u64) {
    let s = heap.stats();
    (s.minor_collections, s.major_collections)
}

/// Under STW there is one generation, so every collection is a major.
#[test]
fn an_stw_collection_reports_major() {
    let heap = ArcMagrGC::new();
    heap.set_mode(crate::gc::GcMode::StwMarkSweep);
    let rec = Arc::new(KindRecorder::default());
    heap.add_observer(rec.clone());
    assert_eq!(heap.force_collect().kind, Some(GcKind::Major));
    heap.collect_cycles();
    assert_eq!(*rec.0.lock(), vec![GcKind::Major, GcKind::Major]);
    assert_eq!(minor_major(&heap), (0, 2));
}

/// Under the generational collector, `force_collect` with no cycle open runs a **minor** — and must
/// say so, in its return value, in the event and in the counters. It used to report `Full` and count
/// a major that never ran.
#[test]
fn a_generational_force_collect_reports_the_minor_it_ran() {
    let heap = ArcMagrGC::new();
    heap.set_mode(crate::gc::GcMode::GenerationalMarkSweep);
    let rec = Arc::new(KindRecorder::default());
    heap.add_observer(rec.clone());
    assert_eq!(heap.force_collect().kind, Some(GcKind::Minor));
    heap.collect_cycles();
    assert_eq!(*rec.0.lock(), vec![GcKind::Minor, GcKind::Minor]);
    assert_eq!(minor_major(&heap), (2, 0));
}

/// With an incremental cycle open, `force_collect` finishes it (a major completes) before its minor.
#[test]
fn a_generational_force_collect_that_finishes_a_cycle_reports_major() {
    let heap = ArcMagrGC::new();
    heap.set_mode(crate::gc::GcMode::GenerationalMarkSweep);
    heap.set_nursery_bytes_for_test(1 << 40);
    let root = heap.alloc_object(dummy_type_desc("Root"), vec![Value::Null], NativeData::None);
    let _pin = heap.pin_root(root);
    assert!(!heap.run_major_slice_for_test(1), "cycle open");
    assert_eq!(heap.force_collect().kind, Some(GcKind::Major));
    assert_eq!(minor_major(&heap), (1, 1));
}
