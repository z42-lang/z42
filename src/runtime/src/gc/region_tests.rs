//! Unit tests for `gc/region.rs` —— add-custom-allocator P0.
//!
//! Coverage:
//! - Allocation: empty → first chunk → grow across chunks
//! - Pointer stability: `&RegionEntry` address is stable across
//!   subsequent allocs (CRITICAL for `GcRef::as_ptr` identity hashing)
//! - Free list reuse: tombstoned slot pops back into use
//! - Generation bumping: tombstone bumps gen; stale handle no longer
//!   matches
//! - Iteration: walks live entries in order, skips tombstoned

use super::*;
use std::sync::atomic::Ordering;

#[test]
fn alloc_into_empty_region_creates_first_chunk_and_entry() {
    let mut r: Region<u64> = Region::new();
    assert_eq!(r.alive_count(), 0);

    let h = r.alloc(42);
    assert_eq!(h.chunk_idx, 0);
    assert_eq!(h.entry_idx, 0);
    assert_eq!(h.generation, 0);

    assert_eq!(r.alive_count(), 1);
    let e = r.resolve(h);
    assert_eq!(*e.value.lock(), 42);
    assert!(e.alive.load(Ordering::Acquire));
}

#[test]
fn alloc_grows_chunk_when_full() {
    let mut r: Region<u64> = Region::new();
    let mut handles = Vec::with_capacity(CHUNK_SIZE + 1);

    for i in 0..(CHUNK_SIZE + 5) {
        handles.push(r.alloc(i as u64));
    }

    // First CHUNK_SIZE handles in chunk 0; next 5 in chunk 1.
    assert_eq!(handles[0].chunk_idx, 0);
    assert_eq!(handles[CHUNK_SIZE - 1].chunk_idx, 0);
    assert_eq!(handles[CHUNK_SIZE].chunk_idx, 1);
    assert_eq!(handles[CHUNK_SIZE].entry_idx, 0);
    assert_eq!(handles[CHUNK_SIZE + 4].chunk_idx, 1);
    assert_eq!(handles[CHUNK_SIZE + 4].entry_idx, 4);

    assert_eq!(r.alive_count(), CHUNK_SIZE + 5);
}

#[test]
fn alloc_pointer_stability_across_growth() {
    // CRITICAL: `GcRef::as_ptr` returns &Mutex<T> inside RegionEntry.
    // That address MUST remain stable as the region grows past chunk
    // boundaries. Verified by recording an address pre-grow and
    // checking equality post-grow.
    let mut r: Region<u64> = Region::new();

    let h0 = r.alloc(100);
    let addr_before: *const RegionEntry<u64> = r.resolve(h0);

    // Force multiple chunk growths.
    for i in 0..(3 * CHUNK_SIZE + 10) {
        r.alloc(i as u64);
    }

    let addr_after: *const RegionEntry<u64> = r.resolve(h0);
    assert_eq!(addr_before, addr_after,
        "RegionEntry address must not move when region grows");

    // Inner value also intact.
    let e = r.resolve(h0);
    assert_eq!(*e.value.lock(), 100);
}

#[test]
fn tombstone_marks_dead_and_pushes_free_list() {
    let mut r: Region<u64> = Region::new();
    let h = r.alloc(7);

    assert_eq!(r.alive_count(), 1);
    assert!(r.resolve(h).alive.load(Ordering::Acquire));

    let ok = r.tombstone(h);
    assert!(ok, "first tombstone succeeds");
    assert_eq!(r.alive_count(), 0);
    assert!(!r.resolve(h).alive.load(Ordering::Acquire));
}

#[test]
fn free_list_reuses_tombstoned_slot() {
    let mut r: Region<u64> = Region::new();
    let h1 = r.alloc(10);
    let h2 = r.alloc(20);
    assert_eq!(r.alive_count(), 2);

    r.tombstone(h1);
    assert_eq!(r.alive_count(), 1);

    let h3 = r.alloc(30);
    // h3 should reuse h1's slot.
    assert_eq!(h3.chunk_idx, h1.chunk_idx);
    assert_eq!(h3.entry_idx, h1.entry_idx);
    // But generation bumped, so h1 (stale) != h3.
    assert_ne!(h3.generation, h1.generation);
    // h2 untouched.
    assert_eq!(*r.resolve(h2).value.lock(), 20);
    assert_eq!(*r.resolve(h3).value.lock(), 30);
}

#[test]
fn tombstone_bumps_generation() {
    let mut r: Region<u64> = Region::new();
    let h = r.alloc(1);
    let gen_before = r.resolve(h).generation.load(Ordering::Acquire);
    r.tombstone(h);
    let gen_after = r.resolve(h).generation.load(Ordering::Acquire);
    assert_eq!(gen_after, gen_before + 1,
        "tombstone bumps generation by exactly 1");
}

#[test]
fn tombstone_stale_handle_is_noop() {
    let mut r: Region<u64> = Region::new();
    let h1 = r.alloc(1);
    r.tombstone(h1);

    // Allocate fresh into the slot.
    let h2 = r.alloc(2);
    assert_eq!(h2.chunk_idx, h1.chunk_idx);
    assert_eq!(h2.entry_idx, h1.entry_idx);

    // Stale h1 now has wrong generation; tombstone should be no-op.
    let ok = r.tombstone(h1);
    assert!(!ok, "stale tombstone is no-op (generation mismatch)");
    // h2 still alive.
    assert!(r.resolve(h2).alive.load(Ordering::Acquire));
    assert_eq!(*r.resolve(h2).value.lock(), 2);
}

#[test]
fn iterate_alive_visits_in_alloc_order_skipping_tombstoned() {
    let mut r: Region<u64> = Region::new();
    let _h0 = r.alloc(100);
    let h1 = r.alloc(200);
    let _h2 = r.alloc(300);
    r.tombstone(h1);

    let mut seen = Vec::new();
    r.iterate_alive(|_h, e| {
        seen.push(*e.value.lock());
    });
    assert_eq!(seen, vec![100, 300],
        "iterate skips tombstoned entry (h1=200)");
}

#[test]
fn iterate_alive_handles_grow_correctly() {
    let mut r: Region<u64> = Region::new();
    for i in 0..(CHUNK_SIZE + 3) {
        r.alloc(i as u64);
    }
    let mut count = 0;
    r.iterate_alive(|_h, _e| count += 1);
    assert_eq!(count, CHUNK_SIZE + 3);
}

#[test]
fn region_entry_mark_cas_idempotent() {
    let mut r: Region<u64> = Region::new();
    let h = r.alloc(0);
    let e = r.resolve(h);

    assert!(!e.is_marked());
    assert!(e.mark(), "first mark CAS succeeds");
    assert!(e.is_marked());
    assert!(!e.mark(), "second mark CAS fails (already marked)");

    e.clear_mark();
    assert!(!e.is_marked());
    assert!(e.mark(), "after clear, mark succeeds again");
}

#[test]
fn total_capacity_and_free_slot_count_track_growth() {
    let mut r: Region<u64> = Region::new();
    assert_eq!(r.total_capacity(), 0);
    assert_eq!(r.free_slot_count(), 0);

    r.alloc(1);
    assert_eq!(r.total_capacity(), CHUNK_SIZE);
    assert_eq!(r.free_slot_count(), CHUNK_SIZE - 1);

    let h = r.alloc(2);
    r.tombstone(h);
    // CHUNK_SIZE - 2 (bump remaining) + 1 (tombstone) = CHUNK_SIZE - 1
    assert_eq!(r.free_slot_count(), CHUNK_SIZE - 1);
}

#[test]
fn region_drop_runs_each_initialized_entry_drop() {
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc as StdArc;

    let drop_count = StdArc::new(AtomicUsize::new(0));

    struct DropCounter(StdArc<AtomicUsize>);
    impl Drop for DropCounter {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::AcqRel);
        }
    }

    {
        let mut r: Region<DropCounter> = Region::new();
        r.alloc(DropCounter(drop_count.clone()));
        r.alloc(DropCounter(drop_count.clone()));
        r.alloc(DropCounter(drop_count.clone()));
        // Region drops here.
    }

    // Expect 3 user-data drops (one per alloc'd entry).
    assert_eq!(drop_count.load(Ordering::Acquire), 3);
}

#[test]
fn region_drop_does_not_touch_tombstoned_slots_double_drop() {
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc as StdArc;

    let drop_count = StdArc::new(AtomicUsize::new(0));

    struct DropCounter(StdArc<AtomicUsize>);
    impl Drop for DropCounter {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::AcqRel);
        }
    }

    {
        let mut r: Region<DropCounter> = Region::new();
        let h1 = r.alloc(DropCounter(drop_count.clone()));
        let _h2 = r.alloc(DropCounter(drop_count.clone()));
        r.tombstone(h1);
        // Tombstoning does NOT drop the user value (the slot still
        // owns a live RegionEntry holding the user value behind a
        // Mutex). At alloc reuse, the old entry is replaced in place
        // — old entry's Drop runs then, which would drop the user
        // value. But if we don't reuse, Region drop must drop it.
    }

    // Both user values dropped exactly once at region drop.
    // (Tombstone alone doesn't drop; that's by design — finalizer
    // dispatch is the caller's job.)
    assert_eq!(drop_count.load(Ordering::Acquire), 2);
}

// ── add-generational-gc P0 (2026-05-22) ────────────────────────────────────

#[test]
fn alloc_pushes_to_young_list_with_gen_age_zero() {
    let mut r: Region<u64> = Region::new();
    let h = r.alloc(7);
    assert_eq!(r.young_count(), 1);
    let entry = r.resolve(h);
    assert_eq!(entry.gen_age(), 0, "fresh alloc starts at gen_age=0");
}

#[test]
fn promote_increments_gen_age() {
    let mut r: Region<u64> = Region::new();
    let h = r.alloc(1);
    assert_eq!(r.resolve(h).gen_age(), 0);

    let promoted_first = r.promote(h);
    assert!(!promoted_first, "first promote (0→1) does not cross threshold yet");
    assert_eq!(r.resolve(h).gen_age(), 1);
    assert_eq!(r.young_count(), 1, "still in young_list after first promote");

    let promoted_second = r.promote(h);
    assert!(promoted_second, "second promote (1→2) crosses PROMOTION_THRESHOLD=2");
    assert_eq!(r.resolve(h).gen_age(), 2);
    assert_eq!(r.young_count(), 0, "removed from young_list at threshold");
}

#[test]
fn promote_stale_handle_is_noop() {
    let mut r: Region<u64> = Region::new();
    let h = r.alloc(1);
    r.tombstone(h);
    let h2 = r.alloc(2); // reuses slot; h is stale

    let result = r.promote(h);
    assert!(!result, "stale handle promote → false");
    // h2 still at gen_age=0
    assert_eq!(r.resolve(h2).gen_age(), 0);
}

#[test]
fn iterate_young_yields_only_young_entries() {
    let mut r: Region<u64> = Region::new();
    let h_young = r.alloc(10);
    let h_to_promote = r.alloc(20);
    // Promote h_to_promote 2 times → reaches threshold.
    r.promote(h_to_promote);
    r.promote(h_to_promote);

    let mut seen = Vec::new();
    r.iterate_young(|h, e| {
        seen.push((h.entry_idx, *e.value.lock()));
    });
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0], (h_young.entry_idx, 10),
        "only h_young remains in young_list after h_to_promote is promoted");
}

#[test]
fn iterate_young_skips_tombstoned_young_entries() {
    let mut r: Region<u64> = Region::new();
    let h_alive  = r.alloc(100);
    let h_dead   = r.alloc(200);
    r.tombstone(h_dead);

    let mut seen = Vec::new();
    r.iterate_young(|_h, e| {
        seen.push(*e.value.lock());
    });
    assert_eq!(seen, vec![100], "tombstoned entry removed from young_list");
    assert_eq!(r.young_count(), 1);
    let _ = h_alive; // suppress unused
}

#[test]
fn mark_card_dirty_sets_bit_at_chunk_offset() {
    let mut r: Region<u64> = Region::new();
    r.alloc(1); // ensures chunk 0 exists + card_dirty[0] initialized to 0

    assert!(!r.is_card_dirty(0));
    r.mark_card_dirty(0);
    assert!(r.is_card_dirty(0));

    // Out-of-range chunk index → no-op (defensive).
    r.mark_card_dirty(999);
    assert!(!r.is_card_dirty(999));
}

#[test]
fn clear_card_dirty_resets_all_bits() {
    let mut r: Region<u64> = Region::new();
    // Force multiple chunks.
    for i in 0..(2 * CHUNK_SIZE + 5) {
        r.alloc(i as u64);
    }
    r.mark_card_dirty(0);
    r.mark_card_dirty(1);
    r.mark_card_dirty(2);
    assert!(r.is_card_dirty(0));
    assert!(r.is_card_dirty(1));
    assert!(r.is_card_dirty(2));

    r.clear_card_dirty();
    assert!(!r.is_card_dirty(0));
    assert!(!r.is_card_dirty(1));
    assert!(!r.is_card_dirty(2));
}

#[test]
fn iterate_dirty_cards_yields_all_entries_in_dirty_chunks() {
    let mut r: Region<u64> = Region::new();
    // Fill chunk 0 with 3 entries; mark chunk 0 dirty.
    let _h0 = r.alloc(10);
    let _h1 = r.alloc(20);
    let _h2 = r.alloc(30);
    // Allocate 1 into chunk 1 but don't mark.
    let _h3 = r.alloc(40);

    // Force chunk 1 — alloc CHUNK_SIZE - 3 more dummies to fill chunk 0,
    // then 1 more for chunk 1.
    for i in 0..(CHUNK_SIZE - 4) {
        r.alloc(100 + i as u64);
    }
    let _h_chunk1 = r.alloc(999); // lands in chunk 1
    assert!(r.chunks_count_for_test() >= 2);

    r.mark_card_dirty(0);

    let mut seen = Vec::new();
    r.iterate_dirty_cards(|_h, e| {
        seen.push(*e.value.lock());
    });
    // Chunk 0 had 4 alloc'd + (CHUNK_SIZE-4) dummies = CHUNK_SIZE entries.
    assert_eq!(seen.len(), CHUNK_SIZE,
        "all chunk-0 entries yielded as roots");
}

#[test]
fn iterate_dirty_cards_skips_clean_chunks() {
    let mut r: Region<u64> = Region::new();
    let _h = r.alloc(1);
    // No mark_card_dirty.
    let mut seen = 0;
    r.iterate_dirty_cards(|_h, _e| seen += 1);
    assert_eq!(seen, 0, "no dirty cards → no entries visited");
}

#[test]
fn tombstone_via_entry_removes_from_young_list() {
    let mut r: Region<u64> = Region::new();
    let h = r.alloc(42);
    assert_eq!(r.young_count(), 1);

    // Take a pointer; the entry stays valid as long as region does.
    let entry: *const RegionEntry<u64> = r.resolve(h);
    let entry_ref = unsafe { &*entry };
    r.tombstone_via_entry(entry_ref);

    assert_eq!(r.young_count(), 0,
        "tombstone_via_entry removes the entry from young_list");
}

// ── add-gc-debug-invariants P0 (2026-05-22) ────────────────────────────────

use crate::gc::region::Violation;

#[test]
fn validate_healthy_region_passes() {
    let mut r: Region<u64> = Region::new();
    // Empty region — should validate cleanly.
    assert_eq!(r.validate(), Ok(()));

    // After several allocs + a tombstone + a promote, still valid.
    let h1 = r.alloc(1);
    let h2 = r.alloc(2);
    let _h3 = r.alloc(3);
    r.tombstone(h1);
    r.promote(h2);  // gen_age 0 → 1 (still young)
    assert_eq!(r.validate(), Ok(()),
        "alloc + tombstone + promote leaves invariants intact");
}

#[test]
fn validate_healthy_after_growing_across_chunks() {
    let mut r: Region<u64> = Region::new();
    for i in 0..(2 * CHUNK_SIZE + 5) {
        r.alloc(i as u64);
    }
    assert_eq!(r.validate(), Ok(()),
        "multi-chunk growth preserves invariants");
}

#[test]
fn validate_detects_old_in_young_list() {
    let mut r: Region<u64> = Region::new();
    let h = r.alloc(1);
    // Manually corrupt: bump gen_age to threshold but leave in young_list.
    let entry = r.resolve(h);
    entry.gen_age.store(PROMOTION_THRESHOLD, Ordering::Relaxed);

    match r.validate() {
        Err(Violation::OldEntryInYoungList { gen_age, .. }) => {
            assert_eq!(gen_age, PROMOTION_THRESHOLD);
        }
        other => panic!("expected OldEntryInYoungList, got {:?}", other),
    }
}

#[test]
fn validate_detects_young_not_in_list() {
    let mut r: Region<u64> = Region::new();
    let h = r.alloc(10);
    // Manually corrupt: remove from young_list without changing gen_age.
    r.young_list.clear();

    match r.validate() {
        Err(Violation::YoungEntryNotInList { chunk_idx, entry_idx }) => {
            assert_eq!((chunk_idx, entry_idx), (h.chunk_idx, h.entry_idx));
        }
        other => panic!("expected YoungEntryNotInList, got {:?}", other),
    }
}

#[test]
fn validate_detects_duplicate_in_young_list() {
    let mut r: Region<u64> = Region::new();
    let h = r.alloc(1);
    r.young_list.push((h.chunk_idx, h.entry_idx)); // duplicate

    match r.validate() {
        Err(Violation::DuplicateInYoungList { chunk_idx, entry_idx }) => {
            assert_eq!((chunk_idx, entry_idx), (h.chunk_idx, h.entry_idx));
        }
        other => panic!("expected DuplicateInYoungList, got {:?}", other),
    }
}

#[test]
fn validate_detects_alive_in_free_list() {
    let mut r: Region<u64> = Region::new();
    let h = r.alloc(1);
    // Manually corrupt: push h's location to free_list without tombstoning.
    r.free_list.push((h.chunk_idx, h.entry_idx));

    match r.validate() {
        Err(Violation::AliveSlotInFreeList { chunk_idx, entry_idx }) => {
            assert_eq!((chunk_idx, entry_idx), (h.chunk_idx, h.entry_idx));
        }
        other => panic!("expected AliveSlotInFreeList, got {:?}", other),
    }
}

#[test]
fn validate_detects_location_mismatch() {
    let mut r: Region<u64> = Region::new();
    let h = r.alloc(1);
    // SAFETY: tests corrupt internal state intentionally.
    // We can't easily mutate `location` (it's not pub) — work around by
    // allocating + manually setting via unsafe field write.
    let entry_ptr = r.resolve(h) as *const RegionEntry<u64> as *mut RegionEntry<u64>;
    unsafe {
        (*entry_ptr).location = (99, 99);
    }

    match r.validate() {
        Err(Violation::LocationMismatch { chunk_idx, entry_idx, recorded }) => {
            assert_eq!((chunk_idx, entry_idx), (h.chunk_idx, h.entry_idx));
            assert_eq!(recorded, (99, 99));
        }
        other => panic!("expected LocationMismatch, got {:?}", other),
    }
}

#[test]
fn validate_detects_card_dirty_length_mismatch() {
    let mut r: Region<u64> = Region::new();
    r.alloc(1);
    // Manually corrupt: truncate card_dirty.
    r.card_dirty.clear();

    match r.validate() {
        Err(Violation::CardDirtyLengthMismatch { expected, actual }) => {
            assert_eq!(expected, 1);
            assert_eq!(actual, 0);
        }
        other => panic!("expected CardDirtyLengthMismatch, got {:?}", other),
    }
}

#[test]
fn validate_tolerates_promoted_entry_outside_young_list() {
    // Promotion: bring gen_age to threshold; entry leaves young_list.
    // This is the canonical "old in region, not in young_list" state —
    // must validate cleanly.
    let mut r: Region<u64> = Region::new();
    let h = r.alloc(1);
    r.promote(h);  // 0 → 1
    r.promote(h);  // 1 → 2 (threshold); removed from young_list
    assert_eq!(r.young_count(), 0);
    let entry = r.resolve(h);
    assert!(entry.gen_age() >= PROMOTION_THRESHOLD);

    assert_eq!(r.validate(), Ok(()),
        "old-and-not-in-young_list is a valid state");
}

#[test]
fn validate_tolerates_tombstoned_entries() {
    let mut r: Region<u64> = Region::new();
    let h1 = r.alloc(1);
    let _h2 = r.alloc(2);
    r.tombstone(h1);
    // Slot in free_list with alive=false → valid.
    assert_eq!(r.validate(), Ok(()));
}

#[test]
fn tombstoned_old_entry_not_in_young_list_no_op() {
    let mut r: Region<u64> = Region::new();
    let h = r.alloc(1);
    r.promote(h); // 0→1
    r.promote(h); // 1→2, removed from young_list
    assert_eq!(r.young_count(), 0);
    assert_eq!(r.resolve(h).gen_age(), 2);

    let ok = r.tombstone(h);
    assert!(ok, "old entry tombstone succeeds");
    // young_list was already empty; remove_from_young_list is a no-op.
    assert_eq!(r.young_count(), 0);
}

#[test]
fn region_drop_after_slot_reuse_drops_each_value_exactly_once() {
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc as StdArc;

    let drop_count = StdArc::new(AtomicUsize::new(0));

    struct DropCounter(StdArc<AtomicUsize>);
    impl Drop for DropCounter {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::AcqRel);
        }
    }

    {
        let mut r: Region<DropCounter> = Region::new();
        let h1 = r.alloc(DropCounter(drop_count.clone()));
        r.tombstone(h1);
        // The tombstoned slot is reused on next alloc; the in-place
        // replacement drops the old user value.
        r.alloc(DropCounter(drop_count.clone()));
        // After reuse: drop_count = 1 (original v1 was dropped when
        // alloc replaced the entry).
        assert_eq!(drop_count.load(Ordering::Acquire), 1);
        // Region drop here releases the second value.
    }
    assert_eq!(drop_count.load(Ordering::Acquire), 2);
}

// ── shrink-object-footprint P1: finalizer slot ──────────────────────────────

#[test]
fn region_entry_stays_lean_for_script_objects() {
    use std::mem::size_of;
    // The whole point of P1: a `RegionEntry` header must not carry a 24-byte
    // mutex-wrapped finalizer for a capability nothing registers. `T + 40`.
    assert_eq!(
        size_of::<RegionEntry<crate::metadata::ScriptObject>>(),
        size_of::<crate::metadata::ScriptObject>() + 40,
        "RegionEntry header grew — check what was added before updating this number",
    );
}

#[test]
fn finalizer_set_take_is_fire_once() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    let entry = RegionEntry::new_for_test(7u32);
    assert!(!entry.has_finalizer());
    assert!(entry.take_finalizer().is_none());

    let hits = Arc::new(AtomicUsize::new(0));
    let h = Arc::clone(&hits);
    entry.set_finalizer(Arc::new(move || { h.fetch_add(1, Ordering::SeqCst); }));
    assert!(entry.has_finalizer());

    let fin = entry.take_finalizer().expect("installed finalizer");
    fin();
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    // Fire-once: the slot is empty after the take.
    assert!(!entry.has_finalizer());
    assert!(entry.take_finalizer().is_none());
}

#[test]
fn finalizer_reinstall_drops_the_previous_box() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    // `dropped` counts how many of the closures' captured Arcs have been released,
    // which is how we see that overwriting a slot frees the old box.
    let live = Arc::new(AtomicUsize::new(0));
    let entry = RegionEntry::new_for_test(1u32);

    let a = Arc::clone(&live);
    entry.set_finalizer(Arc::new(move || { a.fetch_add(1, Ordering::SeqCst); }));
    assert_eq!(Arc::strong_count(&live), 2, "first closure holds a clone");

    let b = Arc::clone(&live);
    entry.set_finalizer(Arc::new(move || { b.fetch_add(1, Ordering::SeqCst); }));
    assert_eq!(
        Arc::strong_count(&live), 2,
        "re-installing must drop the previous finalizer box, not leak it",
    );

    drop(entry);
    assert_eq!(
        Arc::strong_count(&live), 1,
        "dropping the entry must free an un-taken finalizer",
    );
}
