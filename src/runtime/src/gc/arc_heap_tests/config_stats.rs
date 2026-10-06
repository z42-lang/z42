use super::*;

// ── 6. Heap config ───────────────────────────────────────────────────────────

#[test]
fn set_max_heap_bytes_reflects_in_stats() {
    let heap = ArcMagrGC::new();
    heap.set_max_heap_bytes(Some(1_000_000));
    assert_eq!(heap.stats().max_bytes, Some(1_000_000));
    heap.set_max_heap_bytes(None);
    assert_eq!(heap.stats().max_bytes, None);
}

#[test]
fn used_bytes_increases_with_alloc() {
    let heap = ArcMagrGC::new();
    assert_eq!(heap.used_bytes(), 0);
    let _ = heap.alloc_array(vec![Value::I64(1); 10]);
    assert!(heap.used_bytes() > 0);
}

// ── 11. Stats ────────────────────────────────────────────────────────────────

#[test]
fn stats_allocations_monotonically_increases() {
    let heap = ArcMagrGC::new();
    assert_eq!(heap.stats().allocations, 0);
    let _ = heap.alloc_array(vec![]);
    assert_eq!(heap.stats().allocations, 1);
    let _ = heap.alloc_array(vec![Value::I64(1)]);
    let _ = heap.alloc_object(dummy_type_desc("Foo"), vec![], NativeData::None);
    assert_eq!(heap.stats().allocations, 3);
}

#[test]
fn stats_gc_cycles_increments_on_collect_cycles() {
    let heap = ArcMagrGC::new();
    assert_eq!(heap.stats().gc_cycles, 0);
    heap.collect_cycles();
    assert_eq!(heap.stats().gc_cycles, 1);
    heap.collect_cycles();
    heap.collect_cycles();
    assert_eq!(heap.stats().gc_cycles, 3);
}

// extend-runtime-counters P1a: the non-generational cycle collector
// classifies every collection as **major** — minor stays 0, major tracks
// gc_cycles. (The mode is explicit: the default is generational, where both
// entry points run a minor — see `collection.rs`.)
#[test]
fn stats_major_collections_track_non_generational_cycles() {
    let heap = ArcMagrGC::new();
    heap.set_mode(crate::gc::GcMode::StwMarkSweep);
    let s0 = heap.stats();
    assert_eq!((s0.minor_collections, s0.major_collections), (0, 0));

    heap.collect_cycles();
    heap.force_collect();
    let s = heap.stats();
    assert_eq!(s.gc_cycles, 2);
    assert_eq!(s.major_collections, 2, "collect_cycles + force_collect are both major");
    assert_eq!(s.minor_collections, 0, "no generational minor in default mode");
}

#[test]
fn stats_struct_has_all_expected_fields() {
    let heap = ArcMagrGC::new();
    let s = heap.stats();
    // Just access every field — compile time check that all fields exist
    let _ = (
        s.allocations,
        s.gc_cycles,
        s.minor_collections,
        s.major_collections,
        s.reclaimed_bytes,
        s.used_bytes,
        s.committed_bytes,
        s.max_bytes,
        s.roots_pinned,
        s.finalizers_pending,
        s.observers,
    );
}


/// **P0-16 (gc-strategy-stopgap)**: `committed_bytes` is the memory the regions hold, which is not
/// what `used_bytes` counts. Chunks are pooled rather than handed back, so when everything dies the
/// live estimate falls and the committed view does not — exactly the gap between `used` and RSS
/// the runtime audit measured.
#[test]
fn committed_bytes_counts_the_chunks_the_regions_hold() {
    let heap = ArcMagrGC::new();
    heap.set_mode(crate::gc::GcMode::StwMarkSweep);
    heap.set_nursery_bytes_for_test(1 << 40);
    let before = heap.stats().committed_bytes;
    let pins: Vec<_> = (0..2000)
        .map(|_| heap.pin_root(heap.alloc_object(dummy_type_desc("C"), vec![], NativeData::None)))
        .collect();
    let full = heap.stats();
    let slot = std::mem::size_of::<crate::gc::region::RegionEntry<crate::metadata::ScriptObject>>() as u64;
    assert!(full.committed_bytes >= before + 2000 * slot,
        "2000 object slots are at least 2000 × {slot} B of chunk ({} → {})", before, full.committed_bytes);
    for p in pins {
        heap.unpin_root(p);
    }
    heap.force_collect();
    let after = heap.stats();
    assert!(after.used_bytes < full.used_bytes, "the objects are gone from the live estimate");
    assert_eq!(after.committed_bytes, full.committed_bytes, "but their chunks are still held");
}
