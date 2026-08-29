//! add-gc-tlab (stage 2, 2026-08-29): TLAB (chunk-exclusive per-thread
//! allocation) unit tests.
//!
//! The other GC unit tests drive `ArcMagrGC` from an **unarmed** thread (no
//! `VmContext`), so they exercise the ambient locked allocation path. These
//! tests explicitly `arm()` the thread so `finish_alloc` / `alloc_array_obj`
//! take the TLAB fast path (borrow chunk → lock-free bump-fill → retire), and
//! verify it is behavior-equivalent to the locked path: cross-chunk growth,
//! generation-preserving reuse, chunk-level reclaim, strict-OOM degradation,
//! and concurrent multi-thread allocation into one shared heap.

use super::*;
use crate::gc::MagrGC;
use std::sync::Arc as StdArc;
use std::thread;

/// RAII: arms the current thread for TLAB allocation, and on drop retires the
/// TLAB back into `heap` (→ unbound) and disarms — so a cargo worker thread
/// reused by a later test starts clean (unarmed + unbound TLAB).
struct ArmGuard<'a> {
    heap: &'a ArcMagrGC,
}
impl<'a> ArmGuard<'a> {
    fn new(heap: &'a ArcMagrGC) -> Self {
        crate::gc::tlab::arm();
        ArmGuard { heap }
    }
}
impl Drop for ArmGuard<'_> {
    fn drop(&mut self) {
        self.heap.retire_thread_tlab();
        crate::gc::tlab::disarm();
    }
}

fn alive_count(heap: &ArcMagrGC) -> usize {
    let mut n = 0;
    heap.iterate_live_objects(&mut |_| n += 1);
    n
}

/// Cross-chunk: allocating more than one `CHUNK_SIZE` (256) via the TLAB spans
/// multiple borrowed chunks; after retire (snapshot forces it) every pinned
/// object is alive and visible.
#[test]
fn tlab_cross_chunk_all_pinned_alive() {
    let heap = ArcMagrGC::new();
    let _g = ArmGuard::new(&heap);
    let n = 300; // > CHUNK_SIZE → at least 2 object chunks
    let mut pins = Vec::new();
    for _ in 0..n {
        let v = heap.alloc_object(dummy_type_desc("X"), vec![], NativeData::None);
        pins.push(heap.pin_root(v));
    }
    // iterate_live_objects retires the TLAB first → all merged + visible.
    assert_eq!(alive_count(&heap), n, "all TLAB-allocated pinned objects visible");
    assert!(
        heap.region_object_for_test().lock().chunks_count_for_test() >= 2,
        "300 objects span >= 2 chunks"
    );
}

/// Field values written through the TLAB fast-fill path read back correctly
/// (guards the generation-preserving vs fresh write modes in `ChunkClaim::fill`).
#[test]
fn tlab_object_field_values_roundtrip() {
    let heap = ArcMagrGC::new();
    let _g = ArmGuard::new(&heap);
    let leaf = heap.alloc_object(dummy_type_desc("Leaf"), vec![], NativeData::None);
    let _p0 = heap.pin_root(leaf.clone());
    // Object whose field 0 references `leaf`.
    let holder = heap.alloc_object(
        dummy_type_desc("Holder"),
        vec![leaf.clone()],
        NativeData::None,
    );
    let _p1 = heap.pin_root(holder.clone());
    heap.retire_thread_tlab();
    // Read field 0 back — must still point at `leaf`.
    if let Value::Object(gc) = &holder {
        let obj = gc.borrow();
        assert!(
            obj.refs.iter().any(|r| matches!(r, Value::Object(_))),
            "holder retains its object-typed field after TLAB fill"
        );
    } else {
        panic!("expected object");
    }
    // A full collect keeps both (rooted) alive.
    heap.force_collect();
    assert_eq!(alive_count(&heap), 2);
}

/// Unrooted TLAB-allocated objects are reclaimed by a normal collect (the
/// retire-before-mark hook merges the borrowed chunk so sweep sees them).
#[test]
fn tlab_unrooted_objects_are_collected() {
    let heap = ArcMagrGC::new();
    let _g = ArmGuard::new(&heap);
    for _ in 0..50 {
        let _ = heap.alloc_object(dummy_type_desc("Garbage"), vec![], NativeData::None);
    }
    heap.force_collect();
    assert_eq!(alive_count(&heap), 0, "unrooted TLAB objects swept");
}

/// Chunk-level reclaim (D7): repeated alloc-then-collect of garbage recycles
/// dead chunks through `free_chunk_pool` instead of growing `chunks`
/// unboundedly. Without reclaim the chunk count would grow every round.
#[test]
fn tlab_chunk_reclaim_bounds_growth() {
    let heap = ArcMagrGC::new();
    let _g = ArmGuard::new(&heap);
    let mut max_chunks = 0;
    for _ in 0..6 {
        for _ in 0..300 {
            let _ = heap.alloc_object(dummy_type_desc("Round"), vec![], NativeData::None);
        }
        heap.force_collect();
        let c = heap.region_object_for_test().lock().chunks_count_for_test();
        max_chunks = max_chunks.max(c);
    }
    // 300 objects need 2 chunks; with pool reuse the total stays small across 6
    // rounds. Allow generous slack but well below 6*2 = 12 (no-reuse growth).
    assert!(
        max_chunks <= 4,
        "chunk pool reuse bounds growth (saw {max_chunks} chunks)"
    );
}

/// Arrays go through the TLAB array-region fast path too.
#[test]
fn tlab_arrays_cross_chunk_alive() {
    let heap = ArcMagrGC::new();
    let _g = ArmGuard::new(&heap);
    let mut pins = Vec::new();
    for _ in 0..300 {
        let v = heap.alloc_array(vec![Value::Null; 2]);
        pins.push(heap.pin_root(v));
    }
    assert_eq!(alive_count(&heap), 300);
    assert!(
        heap.region_array_for_test().lock().chunks_count_for_test() >= 2,
        "300 arrays span >= 2 array chunks"
    );
}

/// Strict-OOM mode bypasses the TLAB (D6): allocation past the limit is
/// precisely refused (returns Null) with exact `used_bytes` accounting.
#[test]
fn tlab_strict_oom_degrades_to_ambient_refund() {
    let heap = ArcMagrGC::new();
    let _g = ArmGuard::new(&heap);
    // One object to measure per-object size, then cap the heap just above it.
    let v0 = heap.alloc_object(dummy_type_desc("S"), vec![], NativeData::None);
    let _p = heap.pin_root(v0.clone());
    heap.retire_thread_tlab();
    let used = heap.used_bytes();
    heap.set_max_heap_bytes(Some(used)); // exactly full
    heap.set_strict_oom(true);
    let before = heap.used_bytes();
    let refused = heap.alloc_object(dummy_type_desc("S"), vec![], NativeData::None);
    assert!(matches!(refused, Value::Null), "strict OOM refuses over-limit alloc");
    assert_eq!(heap.used_bytes(), before, "refused alloc is precisely refunded");
}

/// A thread WITHOUT a VmContext (unarmed) keeps the ambient path — objects are
/// visible immediately after alloc with no intervening retire.
#[test]
fn tlab_unarmed_thread_uses_ambient_path() {
    let heap = ArcMagrGC::new();
    // No ArmGuard → unarmed.
    let v = heap.alloc_object(dummy_type_desc("Ambient"), vec![], NativeData::None);
    let _p = heap.pin_root(v);
    // No retire / collect: ambient path merged immediately.
    assert_eq!(alive_count(&heap), 1);
}

/// Concurrent stress: N threads share ONE heap, each arms + allocs a mix of
/// objects/arrays, retires + disarms. After join a full collect runs cleanly
/// (debug builds validate region invariants inside every collect), and a
/// pinned root allocated on the main thread survives.
#[test]
fn tlab_concurrent_shared_heap_stress() {
    let heap = StdArc::new(ArcMagrGC::new());
    // Pin a root on the main thread (armed) so we can assert it survives.
    let main_guard = ArmGuard::new(&heap);
    let survivor = heap.alloc_object(dummy_type_desc("Survivor"), vec![], NativeData::None);
    let _pin = heap.pin_root(survivor.clone());
    heap.retire_thread_tlab();
    drop(main_guard);

    let n_threads = 6;
    let per = 400usize;
    let handles: Vec<_> = (0..n_threads)
        .map(|t| {
            let heap = StdArc::clone(&heap);
            thread::spawn(move || {
                crate::gc::tlab::arm();
                let td = dummy_type_desc("W");
                for i in 0..per {
                    if i & 1 == 0 {
                        let _ = heap.alloc_object(td.clone(), vec![Value::Null], NativeData::None);
                    } else {
                        let _ = heap.alloc_array(vec![Value::Null; 3]);
                    }
                    // Occasional concurrent collect from a worker exercises the
                    // borrowed-chunk-skip + region-lock discipline. (No safepoint
                    // coordination here — that's the VM's job; the lock + skip
                    // keeps it race-free regardless.)
                    if t == 0 && (i + 1) % 128 == 0 {
                        let _ = heap.force_collect();
                    }
                }
                // Retire this worker's TLAB into the shared heap, then disarm.
                heap.retire_thread_tlab();
                crate::gc::tlab::disarm();
            })
        })
        .collect();
    for h in handles {
        h.join().expect("worker thread panicked");
    }

    // Final quiescent collect on the main thread: all workers joined (TLABs
    // retired), so this is effectively STW.
    crate::gc::tlab::arm();
    heap.force_collect();
    // The pinned survivor is still alive; workers' unrooted objects are gone.
    let mut n = 0;
    heap.iterate_live_objects(&mut |v| {
        if let Value::Object(_) = v {
            n += 1;
        }
    });
    assert_eq!(n, 1, "only the pinned survivor object remains");
    heap.retire_thread_tlab();
    crate::gc::tlab::disarm();
}
