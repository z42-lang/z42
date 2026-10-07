//! Pooled-chunk decommit (`region/decommit.rs`) and its var-region twin.

use std::ptr::NonNull;
use std::sync::Arc;

use super::*;
use crate::gc::footprint::Footprint;
use crate::gc::refs::GcRef;

/// Fill `n` whole chunks through the TLAB path (the only one that refills pooled chunks) and
/// return their handles.
fn fill_chunks(r: &mut Region<Arc<()>>, token: &Arc<()>, n: usize) -> Vec<RegionHandle> {
    let mut out = Vec::new();
    for _ in 0..n {
        let mut claim = r.borrow_chunk();
        while let Some((_, generation)) = claim.fill(Arc::clone(token)) {
            out.push(RegionHandle { chunk_idx: claim.chunk_idx, entry_idx: claim.filled() - 1, generation });
        }
        r.retire_chunk(&claim);
    }
    out
}

fn pooled_region() -> (Region<Arc<()>>, Arc<()>, Arc<Footprint>, Vec<RegionHandle>) {
    let fp = Arc::new(Footprint::default());
    let mut r: Region<Arc<()>> = Region::new();
    r.attach_footprint(fp.clone(), |_| 100);
    let token = Arc::new(());
    let handles = fill_chunks(&mut r, &token, 3);
    for h in &handles {
        assert!(r.tombstone(*h));
    }
    assert_eq!(r.reclaim_dead_chunks(), 3, "all three chunks die and are pooled");
    (r, token, fp, handles)
}

/// Decommitting drops the dead entries (their payloads go), un-initializes the slots and moves
/// the chunk out of the footprint; nothing is left for a later sweep or drop to touch.
#[test]
fn decommit_releases_the_dead_payloads_and_the_slot_memory() {
    let (mut r, token, fp, _) = pooled_region();
    assert_eq!(Arc::strong_count(&token), 1 + 3 * CHUNK_SIZE, "pooled dead entries still own their values");
    let (committed, pooled) = (fp.committed(), fp.pooled());

    let freed = r.decommit_pool(u64::MAX);
    assert_eq!(freed, 3 * Region::<Arc<()>>::SLOT_BYTES);
    assert_eq!(Arc::strong_count(&token), 1, "every dead entry was dropped");
    assert!((0..3).all(|ci| r.decommitted[ci] && r.initialized[ci].iter().all(|&i| !i)));
    assert_eq!(fp.committed(), committed - freed - 3 * CHUNK_SIZE as u64 * 100, "slots and payloads credited");
    assert_eq!(fp.pooled(), pooled - freed);
    assert_eq!(r.validate(), Ok(()));
    assert_eq!(r.decommit_pool(u64::MAX), 0, "nothing left to decommit");
}

/// The ABA guard: a refilled decommitted chunk restarts its slots **above** every generation
/// they reached, so a handle to a previous occupant can never match the new one.
#[test]
fn a_refilled_decommitted_chunk_starts_above_every_stale_generation() {
    let (mut r, token, fp, old) = pooled_region();
    // A weak reference to a previous occupant, taken while it was alive.
    let weak_entry = NonNull::from(r.resolve(old[0]));
    r.decommit_pool(u64::MAX);
    let before = fp.committed();

    let new = fill_chunks(&mut r, &token, 3);
    assert!(fp.committed() > before, "refilling recommits and recharges");
    for h in &new {
        let stale = old.iter().find(|o| o.chunk_idx == h.chunk_idx && o.entry_idx == h.entry_idx)
            .expect("the pool hands the same chunks back");
        assert!(h.generation > stale.generation, "slot ({}, {}) reissued generation {} ≤ stale {}",
            h.chunk_idx, h.entry_idx, h.generation, stale.generation);
    }
    // SAFETY: the entry pointer stays valid (chunks are never unmapped) and `old[0].generation`
    // is the one it was issued with.
    let weak = GcRef::downgrade(&unsafe { GcRef::from_region_entry(weak_entry, old[0].generation) });
    assert!(weak.upgrade().is_none(), "a stale weak handle must not reach the new occupant");
    assert_eq!(r.validate(), Ok(()));
}

/// Committed pooled chunks are reused before decommitted ones, and only the excess is given
/// back.
#[test]
fn decommit_takes_the_oldest_pooled_chunks_and_reuse_takes_the_committed_ones() {
    let (mut r, token, _fp, _) = pooled_region();
    let pool: Vec<u32> = r.free_chunk_pool.clone();
    r.decommit_pool(1); // one chunk's worth
    assert_eq!(r.pool_decommitted, 1);
    assert!(r.decommitted[pool[0] as usize], "the bottom of the pool goes first");

    let claim = r.borrow_chunk();
    assert_eq!(claim.chunk_idx, *pool.last().unwrap(), "a committed chunk is taken first");
    r.retire_chunk(&claim);
    for _ in 0..2 {
        let claim = r.borrow_chunk();
        r.retire_chunk(&claim);
    }
    assert_eq!(r.pool_decommitted, 0, "popping the decommitted bottom shrinks the prefix");
    assert!(!r.decommitted[pool[0] as usize]);
    drop(token);
}

/// A never-filled tail slot of a decommitted chunk has no handles, but a slot of the next
/// refill must still clear the floor even if the chunk is pooled and refilled again without
/// another decommit in between.
#[test]
fn the_floor_outlives_one_refill() {
    let (mut r, token, _fp, old) = pooled_region();
    r.decommit_pool(u64::MAX);
    let max_old = old.iter().map(|h| h.generation).max().unwrap();
    // Refill only part of one chunk, kill it, pool it, refill it whole.
    let mut claim = r.borrow_chunk();
    let (_, g) = claim.fill(Arc::clone(&token)).unwrap();
    assert!(g >= max_old);
    r.retire_chunk(&claim);
    let ci = claim.chunk_idx;
    assert!(r.tombstone(RegionHandle { chunk_idx: ci, entry_idx: 0, generation: g }));
    assert!(r.reclaim_dead_chunks() >= 1);
    let again = fill_chunks(&mut r, &token, 3);
    assert!(again.iter().all(|h| h.generation >= max_old));
}

// ── the variable-length region ───────────────────────────────────────────────────────────

use crate::gc::var_region::{class_for, BlockType, VarRegion};

/// A decommitted var chunk gives its pages back, stale handles into it still resolve to
/// nothing, and a TLAB refill takes it (recommitted) after the committed ones.
#[test]
fn var_pool_decommit_gives_back_pages_and_refills() {
    let fp = Arc::new(Footprint::default());
    let mut r = VarRegion::new();
    r.attach_footprint(fp.clone());
    let handles: Vec<_> = (0..384).map(|_| r.alloc(1024, BlockType::Str)).collect();
    r.sweep(crate::gc::refs::MarkKind::Major(1)); // nothing marked → every block dies
    let pooled = r.reclaim_dead_var_chunks().pooled;
    assert!(pooled > 1, "several bump chunks pooled");
    let (committed, pooled_bytes) = (fp.committed(), fp.pooled());

    let freed = r.decommit_pool(u64::MAX);
    assert!(freed >= pooled as u64 * 3 * 16 * 1024, "at least three whole pages per 64K chunk");
    assert_eq!(fp.committed(), committed - freed);
    assert_eq!(fp.pooled(), pooled_bytes - freed);
    assert!(handles.iter().all(|h| r.resolve(*h).is_none()), "stale handles stay dead");

    // Refill every pooled chunk through the TLAB path; each comes back usable.
    let (footprint, class) = class_for(1024);
    for _ in 0..pooled {
        let mut claim = r.borrow_chunk();
        while let Some(h) = claim.fill(1024, footprint, class, BlockType::Str) {
            assert!(r.resolve(h).is_some(), "a refilled block resolves");
        }
        r.retire_chunk(&mut claim);
    }
    assert_eq!(fp.committed(), committed, "every decommitted page was charged back");
    assert_eq!(r.decommit_pool(u64::MAX), 0, "the pool is empty");
}
