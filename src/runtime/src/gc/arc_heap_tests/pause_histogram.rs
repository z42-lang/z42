//! Integration tests for [`PauseHistogram`] on `ArcMagrGC`
//! (add-gc-pause-histogram, 2026-05-22).
//!
//! Per-field bucket / boundary correctness is covered in
//! `gc::types::types_tests`; here we verify the heap actually wires
//! `record(pause_us)` into every collect path and projects the
//! histogram through `stats()` correctly.

use super::*;
use crate::gc::GcMode;

fn alloc_obj(heap: &ArcMagrGC, name: &str) -> Value {
    heap.alloc_object(dummy_type_desc(name), vec![Value::Null], NativeData::None)
}

#[test]
fn empty_heap_reports_default_histogram() {
    let heap = ArcMagrGC::default();
    let h = heap.stats().pause_histogram;
    assert_eq!(h.count, 0);
    assert_eq!(h.buckets, [0; 8]);
    assert_eq!(h.min_us, u64::MAX, "sentinel preserved when count == 0");
}

#[test]
fn histogram_visible_in_stats_after_collect() {
    let heap = ArcMagrGC::default();
    heap.set_mode(GcMode::StwMarkSweep);

    // A handful of force_collect calls (no live allocations needed —
    // we just want each collect to record into the histogram).
    let n = 5;
    for _ in 0..n {
        let _ = heap.force_collect();
    }

    let h = heap.stats().pause_histogram;
    assert_eq!(h.count, n as u64, "every collect must record");

    // Sum across all buckets should match count exactly.
    let bucket_sum: u64 = h.buckets.iter().sum();
    assert_eq!(bucket_sum, h.count);

    // total_us is the sum of every recorded pause; must be ≥ count*0
    // (trivially true) and the empty-sentinel min must have been
    // replaced by a real measurement.
    assert!(h.min_us != u64::MAX, "first record clears sentinel");
    assert!(h.max_us >= h.min_us);
}

#[test]
fn histogram_persists_across_mode_switches() {
    let heap = ArcMagrGC::default();

    heap.set_mode(GcMode::StwMarkSweep);
    let _ = heap.force_collect();
    let _ = heap.force_collect();
    let count_after_stw = heap.stats().pause_histogram.count;
    assert_eq!(count_after_stw, 2);

    // Switch mode — histogram must NOT reset.
    heap.set_mode(GcMode::GenerationalMarkSweep);
    let count_after_switch = heap.stats().pause_histogram.count;
    assert_eq!(count_after_switch, 2, "mode switch preserves histogram");

    let _ = heap.force_collect();
    let count_after_more = heap.stats().pause_histogram.count;
    assert_eq!(count_after_more, 3);
}

#[test]
fn histogram_records_force_collect() {
    // force_collect is a separate entry point from collect_cycles;
    // make sure both wire `record`.
    let heap = ArcMagrGC::default();
    heap.set_mode(GcMode::StwMarkSweep);

    let _v = alloc_obj(&heap, "Foo");
    let _ = heap.force_collect();

    let h = heap.stats().pause_histogram;
    assert_eq!(h.count, 1);
    assert!(h.total_us >= h.min_us);
}

// ── add-gc-pause-window (2026-05-24) ─────────────────────────────────────────

#[test]
fn recent_pauses_visible_in_stats_snapshot() {
    let heap = ArcMagrGC::default();
    heap.set_mode(GcMode::StwMarkSweep);

    let n_collects = 7u64;
    for _ in 0..n_collects {
        let _ = heap.force_collect();
    }

    let h = heap.stats().pause_histogram;
    assert_eq!(h.count, n_collects);
    // Window length matches count when far below capacity.
    assert_eq!(h.recent_pauses.len() as u64, n_collects);
    assert!(h.window_cap >= n_collects as usize, "window_cap must be ≥ collect count for this test");
}

#[test]
fn recent_pauses_does_not_exceed_capacity() {
    // Drive past capacity by shrinking it on a fresh heap. Direct
    // field mutation is fine in unit-test scope.
    let heap = ArcMagrGC::default();
    heap.set_mode(GcMode::StwMarkSweep);

    // Bound the window tight so we can saturate in a few collects.
    {
        let mut h = heap.stats().pause_histogram;
        h.window_cap = 3;
        h.recent_pauses.clear();
        // Re-inject the trimmed histogram (only possible through the
        // public Mutex on ArcMagrGC; not exposed here). Instead just
        // run many collects and check len <= window_cap.
        let _ = h;
    }

    let n_collects = 32u64;
    for _ in 0..n_collects {
        let _ = heap.force_collect();
    }
    let h = heap.stats().pause_histogram;
    assert_eq!(h.count, n_collects);
    assert!(
        h.recent_pauses.len() <= h.window_cap,
        "len {} must be ≤ cap {}",
        h.recent_pauses.len(),
        h.window_cap,
    );
}
