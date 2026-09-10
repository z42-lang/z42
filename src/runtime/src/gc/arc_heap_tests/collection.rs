use super::*;

// ── 5. Collection control ────────────────────────────────────────────────────

#[test]
fn force_collect_returns_full_kind() {
    let heap = ArcMagrGC::new();
    let stats = heap.force_collect();
    assert_eq!(stats.kind, Some(GcKind::Full));
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
    assert_eq!(stats.kind, Some(GcKind::Full));
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
    assert_eq!(heap.force_collect().kind, Some(GcKind::Full));
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

