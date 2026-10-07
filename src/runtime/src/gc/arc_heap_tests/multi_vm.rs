//! add-gc-multi-vm-stress (C3, 2026-05-24): multi-`ArcMagrGC` isolation
//! stress. Verifies that GC state is fully per-heap — two embedded
//! VMs running on the same process do not pollute each other's
//! allocations / cycles / mark queues / pause histograms.
//!
//! Complements C2 (single-heap stress) — that test driver covers
//! mutator → GC correctness on one heap; this module covers
//! cross-heap isolation. Each test spawns N threads, each owning a
//! distinct `ArcMagrGC` and running an independent workload.
//!
//! `ArcMagrGC` is already proven `Send + Sync` in `send_sync.rs`; we
//! just put it through real concurrent paces.
//!
//! Debug builds: every collect triggers `debug_validate_invariants()`
//! per-heap, so the C1 validator implicitly guards every iteration
//! across every thread.

use super::*;
use crate::gc::{GcMode, MagrGC};
use std::sync::Arc as StdArc;
use std::thread;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn alloc_obj(heap: &ArcMagrGC, td: &StdArc<TypeDesc>) -> Value {
    heap.alloc_object(td.clone(), vec![Value::Null], NativeData::None)
}

fn alloc_arr(heap: &ArcMagrGC, len: usize) -> Value {
    heap.alloc_array(vec![Value::Null; len])
}

/// Minimal per-heap workload: a few hundred allocs interspersed with
/// occasional `force_collect`. Pinning omitted intentionally — keeping
/// pure alloc-then-sweep maximises sweep-side stress while keeping the
/// per-thread footprint small enough that 4 threads × 5 modes finish
/// in <1s under cargo test.
fn run_workload(heap: &ArcMagrGC, td: StdArc<TypeDesc>, n_alloc: usize, collect_every: usize) {
    for i in 0..n_alloc {
        let _ = if i & 1 == 0 {
            alloc_obj(heap, &td)
        } else {
            alloc_arr(heap, 3)
        };
        if collect_every > 0 && (i + 1) % collect_every == 0 {
            let _ = heap.force_collect();
        }
    }
    // One final collect to ensure sweep runs once at minimum.
    let _ = heap.force_collect();
}

// ── Tests ────────────────────────────────────────────────────────────────────

/// Baseline: allocs + a collect on h1 leave h2 entirely untouched.
/// Catches any accidentally-global state (statics, thread-locals
/// leaking, mark queue cross-aliasing).
#[test]
fn two_heaps_alloc_no_cross_contamination() {
    let h1 = ArcMagrGC::default();
    let h2 = ArcMagrGC::default();

    let td = dummy_type_desc("OnlyOnH1");
    for _ in 0..50 {
        let _ = alloc_obj(&h1, &td);
    }
    let _ = h1.force_collect();

    let s1 = h1.stats();
    let s2 = h2.stats();

    assert!(s1.allocations >= 50, "h1 should track its own allocs");
    assert!(s1.gc_cycles >= 1,    "h1 should have collected");

    assert_eq!(s2.allocations, 0, "h2 must NOT see h1's allocations");
    assert_eq!(s2.gc_cycles,   0, "h2 must NOT see h1's collects");
    assert_eq!(s2.used_bytes,  0, "h2 used_bytes untouched");
    assert_eq!(s2.pause_histogram.count, 0, "h2 pause histogram untouched");
}

/// N threads, each with its own STW heap, running concurrent workloads.
/// Joining: every heap must show its own work and only its own work.
#[test]
fn concurrent_threads_independent_heaps_stw() {
    const N_THREADS: usize = 4;
    const N_ALLOCS:  usize = 100;

    let handles: Vec<_> = (0..N_THREADS)
        .map(|i| {
            thread::spawn(move || {
                let heap = ArcMagrGC::default();
                heap.set_mode(GcMode::StwMarkSweep);
                let td = dummy_type_desc(&format!("T{}", i));
                run_workload(&heap, td, N_ALLOCS, 25);
                heap.stats()
            })
        })
        .collect();

    let stats: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    for (i, s) in stats.iter().enumerate() {
        assert!(
            s.allocations >= N_ALLOCS as u64,
            "thread {}: expected ≥ {} allocations, got {}",
            i, N_ALLOCS, s.allocations
        );
        assert!(
            s.gc_cycles >= 1,
            "thread {}: expected ≥ 1 collect, got {}",
            i, s.gc_cycles
        );
        assert!(
            s.pause_histogram.count >= 1,
            "thread {}: pause histogram must record at least one collect",
            i
        );
    }
}

/// Same pattern under generational mode — exercises young_list +
/// per-chunk card_dirty isolation. Each thread's generational state
/// must stay confined to its own region.
#[test]
fn concurrent_threads_independent_heaps_generational() {
    const N_THREADS: usize = 3;
    const N_ALLOCS:  usize = 80;

    let handles: Vec<_> = (0..N_THREADS)
        .map(|i| {
            thread::spawn(move || {
                let heap = ArcMagrGC::default();
                heap.set_mode(GcMode::GenerationalMarkSweep);
                let td = dummy_type_desc(&format!("G{}", i));
                run_workload(&heap, td, N_ALLOCS, 20);
                heap.stats()
            })
        })
        .collect();

    for h in handles {
        let s = h.join().unwrap();
        assert!(s.allocations >= N_ALLOCS as u64);
        assert!(s.gc_cycles >= 1);
    }
}

/// Each thread uses a DIFFERENT GcMode simultaneously. Verifies the
/// `mode` AtomicU8 is per-instance, not a global, and that one heap
/// switching modes does not perturb another. Implicitly checks the
/// debug validator (C1) on every collect under every mode.
#[test]
fn multi_heaps_mixed_modes() {
    let modes = [
        GcMode::StwMarkSweep,
        GcMode::GenerationalMarkSweep,
    ];

    let handles: Vec<_> = modes
        .iter()
        .copied()
        .enumerate()
        .map(|(i, mode)| {
            thread::spawn(move || {
                let heap = ArcMagrGC::default();
                heap.set_mode(mode);
                let td = dummy_type_desc(&format!("M{}", i));
                run_workload(&heap, td, 60, 15);
                (mode, heap.stats(), heap.mode())
            })
        })
        .collect();

    for h in handles {
        let (requested, stats, final_mode) = h.join().unwrap();
        assert_eq!(
            final_mode, requested,
            "each heap must retain the mode it was set to (no cross-heap mode bleed)"
        );
        assert!(stats.allocations >= 60);
        assert!(stats.gc_cycles >= 1);
    }
}

/// Pause histograms (B5 add-gc-pause-histogram) are per-heap. After
/// concurrent workloads of different sizes, each heap's count must
/// match the number of collects it performed — never aggregate
/// across heaps.
#[test]
fn pause_histograms_per_heap_isolation() {
    const N_THREADS: usize = 4;
    let workloads_per_thread = [10usize, 20, 30, 40];

    let handles: Vec<_> = workloads_per_thread
        .iter()
        .copied()
        .enumerate()
        .map(|(i, n_collects)| {
            thread::spawn(move || {
                let heap = ArcMagrGC::default();
                let td = dummy_type_desc(&format!("H{}", i));
                // Each thread runs `n_collects` explicit force_collects.
                for _ in 0..n_collects {
                    let _ = alloc_obj(&heap, &td);
                    let _ = heap.force_collect();
                }
                heap.stats().pause_histogram
            })
        })
        .collect();

    for (h, expected) in handles.into_iter().zip(workloads_per_thread.iter()) {
        let hist = h.join().unwrap();
        assert_eq!(
            hist.count, *expected as u64,
            "each heap's histogram must record exactly its own collects, \
             not the sum across heaps"
        );
        // Sentinel cleared on first record.
        assert!(hist.min_us != u64::MAX);
        // Bucket sum equals count.
        let bucket_sum: u64 = hist.buckets.iter().sum();
        assert_eq!(bucket_sum, hist.count);
    }
    assert_eq!(N_THREADS, workloads_per_thread.len());
}
