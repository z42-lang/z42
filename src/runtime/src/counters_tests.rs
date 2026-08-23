use super::*;

#[test]
fn new_starts_all_zero() {
    let c = RuntimeCounters::new();
    let s = c.snapshot();
    assert_eq!(s, Snapshot::default());
}

#[test]
fn fetch_add_observable_via_snapshot() {
    let c = RuntimeCounters::new();
    c.builtin_calls.fetch_add(42, Ordering::Relaxed);
    c.exceptions_thrown.fetch_add(3, Ordering::Relaxed);
    c.exceptions_caught.fetch_add(2, Ordering::Relaxed);

    let s = c.snapshot();
    assert_eq!(s.builtin_calls,     42);
    assert_eq!(s.exceptions_thrown, 3);
    assert_eq!(s.exceptions_caught, 2);
    assert_eq!(s.native_calls,      0);
}

#[test]
fn snapshot_is_copy_independent_of_source() {
    let c = RuntimeCounters::new();
    c.builtin_calls.fetch_add(5, Ordering::Relaxed);
    let s1 = c.snapshot();

    // Subsequent mutations of c don't affect captured snapshot
    c.builtin_calls.fetch_add(10, Ordering::Relaxed);
    let s2 = c.snapshot();

    assert_eq!(s1.builtin_calls, 5);
    assert_eq!(s2.builtin_calls, 15);
}

#[test]
fn display_lists_all_fields() {
    let s = Snapshot {
        builtin_calls:        100,
        native_calls:         50,
        jit_methods_compiled: 10,
        jit_compile_us_total: 12345,
        jit_native_from_interp: 42,
        exceptions_thrown:    5,
        exceptions_caught:    3,
    };
    let out = format!("{s}");
    // Every field name appears, every value appears.
    for needle in [
        "builtin_calls:        100",
        "native_calls:         50",
        "jit_methods_compiled: 10",
        "jit_compile_us_total: 12345",
        "exceptions_thrown:    5",
        "exceptions_caught:    3",
    ] {
        assert!(out.contains(needle), "Snapshot display missing `{needle}`; got:\n{out}");
    }
}

#[test]
fn to_json_is_single_line_with_all_fields() {
    let s = Snapshot {
        builtin_calls:        100,
        native_calls:         50,
        jit_methods_compiled: 10,
        jit_compile_us_total: 12345,
        jit_native_from_interp: 42,
        exceptions_thrown:    5,
        exceptions_caught:    3,
    };
    let j = s.to_json();
    assert!(!j.contains('\n'), "JSON stats must be single-line; got:\n{j}");
    assert!(j.starts_with("{\"z42vm_counters\":1,"), "missing sentinel key; got:\n{j}");
    for needle in [
        "\"builtin_calls\":100",
        "\"native_calls\":50",
        "\"jit_methods_compiled\":10",
        "\"jit_compile_us_total\":12345",
        "\"jit_native_from_interp\":42",
        "\"exceptions_thrown\":5",
        "\"exceptions_caught\":3",
    ] {
        assert!(j.contains(needle), "JSON stats missing `{needle}`; got:\n{j}");
    }
}

#[test]
fn concurrent_increments_are_lossless() {
    use std::sync::Arc;
    use std::thread;

    let c = Arc::new(RuntimeCounters::new());
    let mut handles = Vec::new();
    for _ in 0..8 {
        let c = Arc::clone(&c);
        handles.push(thread::spawn(move || {
            for _ in 0..1000 {
                c.builtin_calls.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }
    for h in handles { h.join().unwrap(); }

    assert_eq!(c.snapshot().builtin_calls, 8000);
}

// ── ProfileSnapshot (extend-runtime-counters P1a) ──────────────────────────

/// A counter snapshot with distinct non-zero values, for merge/serialize tests.
fn sample_counters() -> Snapshot {
    Snapshot {
        builtin_calls:        100,
        native_calls:         50,
        jit_methods_compiled: 10,
        jit_compile_us_total: 12345,
        jit_native_from_interp: 42,
        exceptions_thrown:    5,
        exceptions_caught:    3,
    }
}

#[test]
fn profile_snapshot_json_is_superset_of_counters_json() {
    let p = ProfileSnapshot::new(sample_counters(), 7777, 4, 1, 65536);
    let j = p.to_json();

    // Single line + sentinel preserved.
    assert!(!j.contains('\n'), "profile JSON must be single-line; got:\n{j}");
    assert!(j.starts_with("{\"z42vm_counters\":1,"), "missing sentinel; got:\n{j}");

    // Every counter key from the base snapshot survives (no drift).
    for needle in [
        "\"builtin_calls\":100",
        "\"native_calls\":50",
        "\"jit_methods_compiled\":10",
        "\"jit_compile_us_total\":12345",
        "\"jit_native_from_interp\":42",
        "\"exceptions_thrown\":5",
        "\"exceptions_caught\":3",
    ] {
        assert!(j.contains(needle), "profile JSON missing counter key `{needle}`; got:\n{j}");
    }

    // Heap-derived keys are appended.
    for needle in [
        "\"allocations\":7777",
        "\"minor_collections\":4",
        "\"major_collections\":1",
        "\"reclaimed_bytes\":65536",
    ] {
        assert!(j.contains(needle), "profile JSON missing heap key `{needle}`; got:\n{j}");
    }

    // Valid JSON object (balanced single closing brace at end).
    assert!(j.ends_with('}'), "profile JSON must end with `}}`; got:\n{j}");
    assert_eq!(j.matches("{").count(), 1, "expected a single flat object; got:\n{j}");
}

#[test]
fn profile_snapshot_display_lists_counter_and_heap_fields() {
    let p = ProfileSnapshot::new(sample_counters(), 7777, 4, 1, 65536);
    let out = format!("{p}");
    for needle in [
        "builtin_calls:        100",
        "exceptions_caught:    3",
        "allocations:          7777",
        "gc_minor_collections: 4",
        "gc_major_collections: 1",
        "gc_reclaimed_bytes:   65536",
    ] {
        assert!(out.contains(needle), "profile display missing `{needle}`; got:\n{out}");
    }
}

#[test]
fn profile_snapshot_default_all_zero() {
    let p = ProfileSnapshot::default();
    assert_eq!(p.counters, Snapshot::default());
    assert_eq!(p.allocations, 0);
    assert_eq!(p.minor_collections, 0);
    assert_eq!(p.major_collections, 0);
    assert_eq!(p.reclaimed_bytes, 0);
}
