//! Unit tests for [`PauseHistogram`] (add-gc-pause-histogram).
//!
//! Covers bucket boundary correctness, record-side effects on
//! min/max/total/count, sentinel handling for the empty case, and
//! saturation on extreme repeated input.

use super::*;

/// Serialize tests that read or mutate the process-global
/// `Z42_GC_PAUSE_WINDOW` env var. Without this, parallel runs of
/// `pause_window_cap_from_env_clamps_and_falls_back` (sets the var to
/// various values) race with `default_*` tests (call
/// `PauseHistogram::default()` which reads the env via
/// `pause_window_cap_from_env`) and the latter sees garbage caps.
static PAUSE_WINDOW_ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn default_is_empty() {
    let _env_guard = PAUSE_WINDOW_ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let h = PauseHistogram::default();
    assert_eq!(h.buckets, [0; 8]);
    assert_eq!(h.min_us, u64::MAX, "empty sentinel");
    assert_eq!(h.max_us, 0);
    assert_eq!(h.total_us, 0);
    assert_eq!(h.count, 0);
    // add-gc-pause-window: window is empty + capacity = env / default.
    assert_eq!(h.recent_pauses.len(), 0);
    assert!(h.window_cap >= 1, "capacity must be positive");
}

#[test]
fn default_has_empty_window_with_default_capacity() {
    let _env_guard = PAUSE_WINDOW_ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    std::env::remove_var("Z42_GC_PAUSE_WINDOW");
    let h = PauseHistogram::default();
    assert_eq!(h.recent_pauses.len(), 0);
    assert_eq!(h.window_cap, PAUSE_WINDOW_DEFAULT_CAP);
}

#[test]
fn record_appends_to_window_in_chronological_order() {
    let mut h = PauseHistogram::default();
    h.record(11);
    h.record(22);
    h.record(33);
    let snapshot: Vec<u64> = h.recent_pauses.iter().copied().collect();
    assert_eq!(snapshot, vec![11u64, 22, 33]);
}

#[test]
fn window_evicts_oldest_at_capacity() {
    let mut h = PauseHistogram::default();
    h.window_cap = 3;  // shrink for the test
    h.recent_pauses = std::collections::VecDeque::with_capacity(3);

    for v in [1u64, 2, 3, 4, 5] {
        h.record(v);
    }
    let snapshot: Vec<u64> = h.recent_pauses.iter().copied().collect();
    assert_eq!(snapshot, vec![3u64, 4, 5], "oldest two (1, 2) evicted");
    assert_eq!(h.count, 5, "count still tracks every sample");
}

/// runtime-config-phase2 : parsing logic moved to
/// `crate::config::parse_gc_pause_window` and is covered by
/// `config::tests::from_getter_gc_pause_window_clamps_and_validates`.
/// `pause_window_cap_from_env()` is now a thin delegator to
/// `runtime_config().gc_pause_window`. Smoke-test that the delegator
/// returns a sane value (≥1, ≤MAX). Detailed parsing cases live in
/// the config module's tests where they don't fight the process-global
/// `LazyLock` cache.
#[test]
fn pause_window_cap_from_env_delegates_to_runtime_config() {
    let cap = pause_window_cap_from_env();
    assert!(cap >= 1, "capacity must be positive");
    assert!(cap <= PAUSE_WINDOW_MAX_CAP, "capacity must respect MAX clamp");
}

#[test]
fn bucket_index_boundaries() {
    // Sub-µs collects fall in bucket 0.
    assert_eq!(PauseHistogram::bucket_index(0), 0);
    assert_eq!(PauseHistogram::bucket_index(9), 0);

    // Half-open intervals: boundary value lands in the HIGHER bucket.
    assert_eq!(PauseHistogram::bucket_index(10),         1);
    assert_eq!(PauseHistogram::bucket_index(99),         1);
    assert_eq!(PauseHistogram::bucket_index(100),        2);
    assert_eq!(PauseHistogram::bucket_index(999),        2);
    assert_eq!(PauseHistogram::bucket_index(1_000),      3);
    assert_eq!(PauseHistogram::bucket_index(9_999),      3);
    assert_eq!(PauseHistogram::bucket_index(10_000),     4);
    assert_eq!(PauseHistogram::bucket_index(99_999),     4);
    assert_eq!(PauseHistogram::bucket_index(100_000),    5);
    assert_eq!(PauseHistogram::bucket_index(999_999),    5);
    assert_eq!(PauseHistogram::bucket_index(1_000_000),  6);
    assert_eq!(PauseHistogram::bucket_index(9_999_999),  6);

    // Catastrophic-pause bucket.
    assert_eq!(PauseHistogram::bucket_index(10_000_000), 7);
    assert_eq!(PauseHistogram::bucket_index(u64::MAX),   7);
}

#[test]
fn record_updates_bucket() {
    let mut h = PauseHistogram::default();
    h.record(5);           // bucket 0
    h.record(50);          // bucket 1
    h.record(500);         // bucket 2
    h.record(5_000);       // bucket 3
    h.record(50_000);      // bucket 4
    h.record(500_000);     // bucket 5
    h.record(5_000_000);   // bucket 6
    h.record(50_000_000);  // bucket 7

    assert_eq!(h.buckets, [1; 8]);
    assert_eq!(h.count, 8);
}

#[test]
fn record_updates_min_max_total_count() {
    let mut h = PauseHistogram::default();
    h.record(100);
    h.record(50);
    h.record(200);

    assert_eq!(h.min_us, 50);
    assert_eq!(h.max_us, 200);
    assert_eq!(h.total_us, 350);
    assert_eq!(h.count, 3);
}

#[test]
fn record_first_value_sets_min_from_sentinel() {
    // Regression guard: the empty sentinel (u64::MAX) must NOT survive
    // the first `record` call — even when the recorded pause is 0.
    let mut h = PauseHistogram::default();
    h.record(0);

    assert_eq!(h.min_us, 0, "first record must overwrite sentinel");
    assert_eq!(h.max_us, 0);
    assert_eq!(h.count, 1);
}

#[test]
fn record_saturates_on_overflow() {
    let mut h = PauseHistogram::default();
    h.count    = u64::MAX - 1;
    h.total_us = u64::MAX - 1;
    h.buckets[0] = u64::MAX - 1;

    // Two more `record(1)` calls hit saturation in count / total /
    // bucket simultaneously.
    h.record(1);
    h.record(1);

    assert_eq!(h.count,        u64::MAX, "count saturates instead of wrapping");
    assert_eq!(h.total_us,     u64::MAX, "total saturates");
    assert_eq!(h.buckets[0],   u64::MAX, "bucket saturates");
}
