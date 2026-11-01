//! M12 hole reuse at the region level: what `take_holes` / `VarHoleStash::fill` /
//! `retire_holes` do to slots, generations, flags, the census and the young set. Miri-sensitive
//! (raw slot pointers, in-place header rewrites): run under
//! `cargo +nightly miri test -p z42 --lib gc::var_region::holes`.

use super::super::block::{BlockType, FIRST_GEN};
use super::super::chunk::class_for;
use super::super::{VarGcRef, VarRegion};
use super::{VarHoleStash, GEN_REUSE_SPAN, HOLE_BATCH};

/// 24 payload bytes → a 40-byte slot.
const PAYLOAD: usize = 24;

fn class() -> u8 {
    class_for(PAYLOAD).1
}

fn addr(h: VarGcRef) -> usize {
    h.addr()
}

/// `n` blocks through the locked path; every other one tombstoned. Returns `(live, dead)`.
fn region_with_holes(n: usize, ty: BlockType) -> (VarRegion, Vec<VarGcRef>, Vec<VarGcRef>) {
    let mut r = VarRegion::new();
    let all: Vec<VarGcRef> = (0..n).map(|_| r.alloc(PAYLOAD, ty)).collect();
    let (mut live, mut dead) = (Vec::new(), Vec::new());
    for (i, h) in all.into_iter().enumerate() {
        if i % 2 == 0 {
            live.push(h);
        } else {
            assert!(r.tombstone(h));
            dead.push(h);
        }
    }
    (r, live, dead)
}

/// The census and the young set agree with a walk of the blocks (outside a stash's lifetime).
fn assert_census(r: &VarRegion) {
    let mut alive = vec![0u32; r.live_per_chunk_for_test().len()];
    let mut young = 0usize;
    r.iterate_alive(|v, h| {
        alive[r.chunk_of(v.addr()).expect("in a chunk")] += 1;
        if h.gen_age() < r.promotion_age {
            young += 1;
        }
    });
    assert_eq!(r.live_per_chunk_for_test(), alive, "live_per_chunk matches a walk");
    assert_eq!(r.live_count(), alive.iter().map(|&n| n as usize).sum::<usize>());
    assert_eq!(r.young_count(), young, "the young set holds exactly the alive young blocks");
}

#[test]
fn dead_slots_are_reused_in_place() {
    let (mut r, _live, dead) = region_with_holes(16, BlockType::Str);
    let chunks = r.chunk_count();
    let mut stash = VarHoleStash::new();
    assert!(r.take_holes(class(), &mut stash));
    assert_eq!(stash.unused(), dead.len());
    let dead_addrs: Vec<usize> = dead.iter().map(|&h| addr(h)).collect();
    let mut got = Vec::new();
    // SAFETY: owner thread; the slots came from `r`.
    while let Some(h) = stash.has(class()).then(|| unsafe { stash.fill(class(), PAYLOAD, BlockType::Str) }) {
        h.publish();
        got.push(addr(h));
    }
    got.sort_unstable();
    let mut want = dead_addrs.clone();
    want.sort_unstable();
    assert_eq!(got, want, "every dead slot is handed out exactly once, at its own address");
    r.retire_holes(&mut stash);
    assert_eq!(r.chunk_count(), chunks, "reuse never carves new memory");
    assert!(!r.take_holes(class(), &mut stash), "the free list is drained");
    assert_census(&r);
}

#[test]
fn a_reused_slot_keeps_the_tombstone_generation_and_rejects_stale_handles() {
    let (mut r, _live, dead) = region_with_holes(4, BlockType::Str);
    let stale = dead[0];
    let mut stash = VarHoleStash::new();
    assert!(r.take_holes(class(), &mut stash));
    let mut fresh = None;
    // SAFETY: owner thread; the slots came from `r`.
    while let Some(h) = stash.has(class()).then(|| unsafe { stash.fill(class(), PAYLOAD, BlockType::Str) }) {
        if addr(h) == addr(stale) {
            fresh = Some(h);
        } else {
            h.publish();
        }
    }
    let fresh = fresh.expect("the stale handle's slot was reused");
    // Not visible before its funnel publishes it.
    assert!(r.resolve(fresh).is_none(), "a filled hole is not alive until published");
    let mut seen = false;
    r.iterate_alive(|h, _| seen |= addr(h) == addr(fresh));
    assert!(!seen, "a heap walk does not see an unpublished block");
    fresh.publish();
    assert!(r.resolve(fresh).is_some() && fresh.is_live());
    assert!(r.resolve(stale).is_none() && !stale.is_live(), "the old occupant's handle stays dead");
    // SAFETY: both are headers in `r`'s chunks.
    let (g_old, g_new) = unsafe { (stale.header_ptr().as_ref().generation(), fresh.header_ptr().as_ref().generation()) };
    assert_eq!(g_old, g_new, "same slot, one header");
    assert_eq!(fresh.gen16(), stale.gen16().wrapping_add(2), "tombstone took it to even, reuse to the next odd");
    assert!(!r.tombstone(stale), "a stale handle cannot tombstone the new occupant");
    r.retire_holes(&mut stash);
    assert_census(&r);
}

#[test]
fn a_reused_slot_starts_young_unmarked_and_without_flags() {
    let mut r = VarRegion::new();
    let _neighbour = r.alloc(PAYLOAD, BlockType::Object);
    // Before it died, the block was old, marked, remembered and side-flagged.
    let victim = r.alloc(PAYLOAD, BlockType::Object);
    // SAFETY: header of a live block in `r`.
    let hdr = unsafe { victim.header_ptr().as_ref() };
    hdr.raise_gen_age_to(3);
    hdr.mark(crate::object::header::marks::MarkKind::Major(1));
    assert!(hdr.try_set_remembered());
    hdr.set_side();
    assert!(r.tombstone(victim));
    // The flagged block is on the dead list: nothing is handed out until the heap drains it.
    let mut stash = VarHoleStash::new();
    assert!(!r.take_holes(class(), &mut stash), "no reuse while the flagged-dead list is pending");
    assert!(stash.is_dry(class()));
    assert_eq!(r.take_flagged_dead(), vec![addr(victim)]);
    r.retire_holes(&mut stash);
    assert!(r.take_holes(class(), &mut stash));
    let mut reused = None;
    // SAFETY: owner thread; the slots came from `r`.
    while let Some(h) = stash.has(class()).then(|| unsafe { stash.fill(class(), PAYLOAD, BlockType::Object) }) {
        h.publish();
        if addr(h) == addr(victim) {
            reused = Some(h);
        }
    }
    let h = reused.expect("the flagged block's slot is reusable once drained");
    // SAFETY: header of a live block in `r`.
    let hdr = unsafe { h.header_ptr().as_ref() };
    assert_eq!(hdr.gen_age(), 0);
    assert!(!hdr.is_remembered());
    assert!(!hdr.needs_dead_notice(), "side and remembered bits are clear");
    assert!(!hdr.is_marked(crate::object::header::marks::MarkKind::Major(1)) && hdr.major_epoch() == 0);
    assert_eq!(hdr.block_type(), BlockType::Object);
    assert!(r.young_blocks_for_test().contains(&h.header_ptr()), "reused block is young");
    r.retire_holes(&mut stash);
    assert_census(&r);
}

#[test]
fn unused_slots_go_back_and_the_census_is_restored() {
    let (mut r, _live, dead) = region_with_holes(12, BlockType::Str);
    let mut stash = VarHoleStash::new();
    assert!(r.take_holes(class(), &mut stash));
    // Taken slots count as live (and young) while stashed.
    assert_eq!(r.live_count(), 6 + dead.len());
    // SAFETY: owner thread; the slots came from `r`.
    let used: Vec<VarGcRef> = (0..2).map(|_| stash.has(class()).then(|| unsafe { stash.fill(class(), PAYLOAD, BlockType::Str) }).unwrap()).collect();
    used.iter().for_each(|h| h.publish());
    r.retire_holes(&mut stash);
    assert_eq!(stash.unused(), 0);
    assert!(stash.is_empty());
    assert_census(&r);
    // The other four are back on the free list — the locked path finds them.
    let back: Vec<usize> = (0..dead.len() - 2).map(|_| addr(r.alloc(PAYLOAD, BlockType::Str))).collect();
    let dead_addrs: Vec<usize> = dead.iter().map(|&h| addr(h)).collect();
    assert!(back.iter().all(|a| dead_addrs.contains(a)), "returned slots are reused, not leaked");
}

#[test]
fn a_filled_slot_tombstoned_before_retire_keeps_census_and_young_set_exact() {
    // `finalize_now` on another thread can tombstone a block its owner has not retired yet.
    let (mut r, _live, _dead) = region_with_holes(8, BlockType::Object);
    let mut stash = VarHoleStash::new();
    assert!(r.take_holes(class(), &mut stash));
    // SAFETY: owner thread; the slots came from `r`.
    let h = stash.has(class()).then(|| unsafe { stash.fill(class(), PAYLOAD, BlockType::Object) }).unwrap();
    h.publish();
    assert!(r.tombstone(h));
    r.retire_holes(&mut stash);
    assert_census(&r);
    assert!(!r.young_blocks_for_test().contains(&h.header_ptr()));
}

#[test]
fn a_stashed_slot_keeps_its_chunk_from_being_pooled() {
    let mut r = VarRegion::new();
    let mut claim = r.borrow_chunk();
    let blocks: Vec<VarGcRef> =
        (0..8).map(|_| claim.fill(PAYLOAD, class_for(PAYLOAD).0, class(), BlockType::Str).unwrap()).collect();
    r.retire_chunk(&mut claim);
    for &b in &blocks {
        assert!(r.tombstone(b));
    }
    let mut stash = VarHoleStash::new();
    assert!(r.take_holes(class(), &mut stash));
    assert_eq!(r.reclaim_dead_var_chunks().pooled, 0, "a chunk with stashed slots is not dead");
    r.retire_holes(&mut stash);
    assert_eq!(r.reclaim_dead_var_chunks().pooled, 1, "once returned, the chunk dies whole");
    assert!(!r.take_holes(class(), &mut stash), "its free entries went stale with the pooling");
}

#[test]
fn a_slot_stops_being_recycled_before_its_generation_wraps() {
    let mut r = VarRegion::new();
    let a = r.alloc(PAYLOAD, BlockType::Str);
    let b = r.alloc(PAYLOAD, BlockType::Str);
    // Live generations as far past a fresh chunk's `reuse_gen` (`FIRST_GEN`) as they may get:
    // `a`'s next occupant lands exactly on the span, `b`'s would pass it.
    let (ga, gb) = (FIRST_GEN.wrapping_add(GEN_REUSE_SPAN - 2), FIRST_GEN.wrapping_add(GEN_REUSE_SPAN));
    // SAFETY: headers of live blocks in `r`.
    unsafe {
        a.header_ptr().as_ref().generation.store(ga, std::sync::atomic::Ordering::Relaxed);
        b.header_ptr().as_ref().generation.store(gb, std::sync::atomic::Ordering::Relaxed);
    }
    let a = VarGcRef::pack(a.header_ptr(), ga);
    let b = VarGcRef::pack(b.header_ptr(), gb);
    assert!(r.tombstone(a), "its next occupant reaches GEN_REUSE_SPAN: still recyclable");
    assert!(r.tombstone(b), "would pass it: retired until the chunk is pooled");
    let mut stash = VarHoleStash::new();
    assert!(r.take_holes(class(), &mut stash));
    assert_eq!(stash.unused(), 1);
    // SAFETY: owner thread; the slot came from `r`.
    let h = stash.has(class()).then(|| unsafe { stash.fill(class(), PAYLOAD, BlockType::Str) }).unwrap();
    assert_eq!(addr(h), addr(a));
    h.publish();
    r.retire_holes(&mut stash);
}

#[test]
fn a_batch_is_bounded_and_refills_from_what_is_left() {
    let n = 2 * (HOLE_BATCH + 8);
    let (mut r, _live, dead) = region_with_holes(n, BlockType::Str);
    let mut stash = VarHoleStash::new();
    let mut filled = 0usize;
    loop {
        // SAFETY: owner thread; the slots came from `r`.
        if let Some(h) = stash.has(class()).then(|| unsafe { stash.fill(class(), PAYLOAD, BlockType::Str) }) {
            h.publish();
            filled += 1;
            continue;
        }
        if stash.is_dry(class()) || !r.take_holes(class(), &mut stash) {
            break;
        }
        assert!(stash.unused() <= HOLE_BATCH);
    }
    assert_eq!(filled, dead.len());
    assert!(stash.is_dry(class()));
    r.retire_holes(&mut stash);
    assert!(!stash.is_dry(class()), "retire forgets the dry marks");
    assert_census(&r);
}

#[test]
fn chunk_epoch_is_unchanged_by_reuse() {
    // Weak object references check the chunk epoch, then the header: reuse must leave the first
    // alone (the address is still a header) and make the second fail.
    let (mut r, live, dead) = region_with_holes(4, BlockType::Object);
    let ci = r.chunk_of(live[0].addr()).unwrap() as u32;
    let epoch = r.chunk_epoch(ci);
    let mut stash = VarHoleStash::new();
    assert!(r.take_holes(class(), &mut stash));
    // SAFETY: owner thread; the slots came from `r`.
    while let Some(h) = stash.has(class()).then(|| unsafe { stash.fill(class(), PAYLOAD, BlockType::Object) }) {
        h.publish();
    }
    r.retire_holes(&mut stash);
    assert_eq!(r.chunk_epoch(ci), epoch);
    assert!(dead.iter().all(|h| !h.is_live()));
    assert!(live.iter().all(|h| h.is_live()));
}

/// A collection that runs while another thread still holds a hole stash (outside the safepoint
/// protocol; `tlab_concurrent_shared_heap_stress` does it) meets the stash's slots in the young set:
/// taken at `take_holes`, dead until filled and published. Every young-set walk must leave them to
/// their owner — before, the minor sweep asserted on them, and a slot whose previous occupant was
/// old was taken out of the young set by the age check, so the block refilled into it was young but
/// unlisted (no minor would ever sweep it, or clear its mark).
#[test]
fn a_collection_leaves_a_foreign_stash_alone() {
    let mut r = VarRegion::new();
    let all: Vec<VarGcRef> = (0..8).map(|_| r.alloc(PAYLOAD, BlockType::Object)).collect();
    for h in all.iter().skip(1).step_by(2) {
        // The previous occupants were old: their headers keep the age after the tombstone.
        h.raise_gen_age_to(crate::gc::PROMOTION_THRESHOLD);
        assert!(r.tombstone(*h));
    }
    let mut stash = VarHoleStash::new();
    assert!(r.take_holes(class(), &mut stash));
    let taken: Vec<usize> = all.iter().skip(1).step_by(2).map(|&h| addr(h)).collect();
    // One slot filled but not yet published, the rest untouched.
    // SAFETY: owner thread; the slots came from `r`.
    let filled = unsafe { stash.fill(class(), PAYLOAD, BlockType::Object) };

    // The "concurrent" collection: a minor sweep (nothing marked), an aging pass and a tenure.
    r.sweep_young(None);
    let _ = r.age_young_survivors();
    let young: Vec<usize> = r.young_blocks_for_test().iter().map(|p| p.as_ptr() as usize).collect();
    assert!(young.is_empty(), "the stashed slots are not blocks; the live blocks were swept");
    let listed = r.young_count();
    assert_eq!(listed, taken.len(), "the stashed slots stay listed (counted at take)");

    // The owner finishes: the filled block is born young and listed, the census holds.
    filled.publish();
    assert!(r.resolve(filled).is_some());
    assert!(r.young_blocks_for_test().iter().any(|p| p.as_ptr() as usize == addr(filled)));
    r.retire_holes(&mut stash);
    assert_census(&r);
    assert_eq!(r.young_count(), 1, "only the published block remains young");
}
