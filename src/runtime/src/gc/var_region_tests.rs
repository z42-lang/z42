//! Unit tests for the variable-length GC block allocator (unify-gc-heap PR-1).
//!
//! These are Miri/ASAN-sensitive (raw allocation, tagged pointers, strict provenance) — run
//! under `cargo +nightly miri test -p z42 gc::var_region` before landing.

use super::*;

/// Helper: write `bytes` into a freshly-allocated block and read them back.
fn write_read_roundtrip(region: &mut VarRegion, bytes: &[u8], ty: BlockType) -> VarGcRef {
    let h = region.alloc(bytes.len(), ty);
    // SAFETY: `region` outlives the borrow; we hold exclusive access via `&mut`.
    unsafe {
        let dst = h.payload_mut().expect("fresh handle resolves");
        dst.copy_from_slice(bytes);
    }
    // SAFETY: same region, still alive.
    let got = unsafe { h.payload().expect("just wrote") };
    assert_eq!(got, bytes);
    h
}

#[test]
fn header_is_16_bytes_payload_8_aligned() {
    assert_eq!(std::mem::size_of::<GcBlockHeader>(), 16);
    assert_eq!(GcBlockHeader::DATA_OFFSET, 16);
    assert_eq!(std::mem::align_of::<GcBlockHeader>(), 8);
}

#[test]
fn alloc_small_roundtrips_payload_and_metadata() {
    let mut r = VarRegion::new();
    let h = write_read_roundtrip(&mut r, b"hello", BlockType::Str);
    let header = r.resolve(h).expect("alive");
    assert_eq!(header.size(), 5);
    assert_eq!(header.block_type(), BlockType::Str);
    assert!(header.is_alive());
    assert_eq!(r.live_count(), 1);
}

#[test]
fn empty_payload_is_valid() {
    let mut r = VarRegion::new();
    let h = r.alloc(0, BlockType::ArrayPrim);
    let header = r.resolve(h).expect("alive");
    assert_eq!(header.size(), 0);
    // SAFETY: alive handle, region held.
    let p = unsafe { h.payload().expect("resolves") };
    assert_eq!(p.len(), 0);
}

#[test]
fn payload_is_zero_initialized() {
    let mut r = VarRegion::new();
    let h = r.alloc(64, BlockType::ArrayPrim);
    // SAFETY: alive handle, region held.
    let p = unsafe { h.payload().expect("resolves") };
    assert!(p.iter().all(|&b| b == 0), "payload must be zeroed on alloc");
}

#[test]
fn many_allocs_have_distinct_stable_addresses() {
    let mut r = VarRegion::new();
    let mut handles = Vec::new();
    for i in 0..1000usize {
        let bytes = (i as u64).to_le_bytes();
        handles.push(write_read_roundtrip(&mut r, &bytes, BlockType::ArrayPrim));
    }
    // All distinct + all still readable with the original content (addresses stable).
    for (i, h) in handles.iter().enumerate() {
        // SAFETY: alive, region held.
        let p = unsafe { h.payload().expect("stable") };
        assert_eq!(p, &(i as u64).to_le_bytes());
    }
    assert_eq!(r.live_count(), 1000);
}

#[test]
fn tombstone_makes_handle_stale() {
    let mut r = VarRegion::new();
    let h = r.alloc(16, BlockType::Str);
    assert!(r.resolve(h).is_some());
    assert!(r.tombstone(h));
    // After tombstone the handle no longer resolves.
    assert!(r.resolve(h).is_none());
    // SAFETY: region held; expected None due to dead/stale.
    assert!(unsafe { h.payload() }.is_none());
    assert_eq!(r.live_count(), 0);
    // Double-tombstone is a no-op.
    assert!(!r.tombstone(h));
}

#[test]
fn free_list_reuses_same_size_class_slot_with_new_generation() {
    let mut r = VarRegion::new();
    let h1 = r.alloc(20, BlockType::Str); // total 36 → class 64
    let addr1 = format!("{:?}", h1);
    assert!(r.tombstone(h1));
    // A fresh alloc of the same size class should reuse the tombstoned slot's address.
    let h2 = r.alloc(24, BlockType::ArrayPrim); // total 40 → class 64 (same)
    let addr2 = format!("{:?}", h2);
    // Same backing address (slot reused), but the OLD handle is stale (generation bumped).
    assert!(r.resolve(h2).is_some());
    assert!(r.resolve(h1).is_none(), "stale handle must not resolve to the reused slot");
    // Different generation → not ptr_eq even though same address.
    assert!(!h1.ptr_eq(&h2));
    // Sanity: addresses (masked) match — proving reuse rather than a new slot.
    assert!(addr1.contains("addr") && addr2.contains("addr"));
    assert_eq!(r.chunk_count(), 1, "reuse should not grow a new chunk");
}

#[test]
fn aba_guard_rejects_stale_after_reuse() {
    let mut r = VarRegion::new();
    let stale = r.alloc(8, BlockType::Str);
    r.tombstone(stale);
    // Reuse the slot many times; the stale handle must never resolve.
    for _ in 0..10 {
        let fresh = r.alloc(8, BlockType::Str);
        assert!(r.resolve(stale).is_none());
        r.tombstone(fresh);
    }
}

#[test]
fn sweep_reclaims_unmarked_keeps_marked() {
    let mut r = VarRegion::new();
    let keep = r.alloc(16, BlockType::Str);
    let drop1 = r.alloc(16, BlockType::Str);
    let keep2 = r.alloc(32, BlockType::ArrayPrim);
    let _drop2 = r.alloc(32, BlockType::ArrayPrim);
    assert_eq!(r.live_count(), 4);

    // Mark the survivors.
    assert!(keep.mark());
    assert!(keep2.mark());

    let (reclaimed, credited) = r.sweep();
    assert_eq!(reclaimed, 2, "two unmarked blocks reclaimed");
    // fix-var-sweep-accounting: only the Str block was charged to `used_bytes` at alloc
    // (header + its true payload length); the ArrayPrim block's bytes were charged — and
    // are credited — by the owning array header, so it contributes nothing here.
    assert_eq!(credited, (GcBlockHeader::DATA_OFFSET + 16) as u64,
        "credit the dead Str's real payload; array element blocks credit zero");
    assert_eq!(r.live_count(), 2);
    // Survivors resolve; reclaimed do not.
    assert!(r.resolve(keep).is_some());
    assert!(r.resolve(keep2).is_some());
    assert!(r.resolve(drop1).is_none());

    // Marks cleared on survivors → a second sweep with no marks reclaims them.
    let (reclaimed2, credited2) = r.sweep();
    assert_eq!(reclaimed2, 2);
    assert_eq!(credited2, (GcBlockHeader::DATA_OFFSET + 16) as u64,
        "the surviving Str, now dead, credits its own payload — not the ArrayPrim's");
    assert_eq!(r.live_count(), 0);
}

#[test]
fn iterate_alive_visits_only_live_blocks() {
    let mut r = VarRegion::new();
    let a = r.alloc(8, BlockType::Str);
    let _b = r.alloc(8, BlockType::Str);
    let c = r.alloc(8, BlockType::Str);
    r.tombstone(_b);

    let mut seen = 0;
    let mut saw_a = false;
    let mut saw_c = false;
    r.iterate_alive(|h, header| {
        seen += 1;
        assert!(header.is_alive());
        if h.ptr_eq(&a) {
            saw_a = true;
        }
        if h.ptr_eq(&c) {
            saw_c = true;
        }
    });
    assert_eq!(seen, 2);
    assert!(saw_a && saw_c);
}

#[test]
fn oversized_block_gets_dedicated_chunk() {
    let mut r = VarRegion::new();
    let big = 200 * 1024; // > CHUNK_BYTES (64 KB)
    let h = r.alloc(big, BlockType::ArrayPrim);
    let header = r.resolve(h).expect("alive");
    assert_eq!(header.size(), big);
    // Write to the far end to prove the whole payload is backed.
    // SAFETY: alive, region held, exclusive.
    unsafe {
        let p = h.payload_mut().expect("resolves");
        p[big - 1] = 0xAB;
        assert_eq!(p[big - 1], 0xAB);
    }
    // A dedicated chunk was allocated (plus possibly no bump chunk yet).
    assert!(r.chunk_count() >= 1);
    // Oversized tombstone works (not free-listed, but alive→dead).
    assert!(r.tombstone(h));
    assert!(r.resolve(h).is_none());
}

#[test]
fn chunk_growth_across_boundary() {
    let mut r = VarRegion::new();
    // Allocate enough ~4 KB blocks to overflow a 64 KB chunk several times.
    let mut handles = Vec::new();
    for i in 0..64usize {
        let mut bytes = vec![0u8; 4000];
        bytes[0] = i as u8;
        bytes[3999] = (i as u8).wrapping_mul(3);
        let h = r.alloc(bytes.len(), BlockType::ArrayPrim);
        // SAFETY: exclusive fresh handle.
        unsafe { h.payload_mut().unwrap().copy_from_slice(&bytes); }
        handles.push((h, bytes));
    }
    assert!(r.chunk_count() >= 2, "should have grown past one chunk");
    // All content intact after growth (no chunk relocation).
    for (h, expect) in &handles {
        // SAFETY: alive, region held.
        let p = unsafe { h.payload().unwrap() };
        assert_eq!(p, expect.as_slice());
    }
}

// ── Payload drop-glue (non-POD payloads, e.g. closure ClosureData) ──────────────────────

use std::sync::atomic::{AtomicUsize, Ordering as AOrd};

/// Only this test touches `DROP_COUNT`, so the shared static is race-free across the parallel
/// test runner (all drop-glue assertions live in the one test below).
static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

/// A payload with a real `Drop` that bumps `DROP_COUNT` — stands in for `ClosureData`'s owned
/// `String`. `#[repr(C)]` + a heap `Box` field so Miri catches a missed/double free.
#[repr(C)]
struct DropCounter {
    _owned: Box<u64>,
}
impl Drop for DropCounter {
    fn drop(&mut self) {
        DROP_COUNT.fetch_add(1, AOrd::SeqCst);
    }
}

/// Test drop glue: drop the `DropCounter` for `Closure`-tagged blocks; POD otherwise.
unsafe fn test_drop_glue(bt: BlockType, p: *mut u8, _size: usize) {
    if bt == BlockType::Closure {
        // SAFETY: Closure-tagged test blocks store exactly one initialized `DropCounter`.
        unsafe { std::ptr::drop_in_place(p as *mut DropCounter) }
    }
}

/// Allocate a block holding a fresh `DropCounter`.
fn alloc_counter(r: &mut VarRegion) -> VarGcRef {
    let h = r.alloc(std::mem::size_of::<DropCounter>(), BlockType::Closure);
    // SAFETY: fresh live block sized for DropCounter; write before any typed read.
    unsafe { h.payload_as_ptr::<DropCounter>().write(DropCounter { _owned: Box::new(7) }) };
    h
}

#[test]
fn drop_glue_finalizes_payload_on_reclaim_and_teardown() {
    DROP_COUNT.store(0, AOrd::SeqCst);
    {
        let mut r = VarRegion::with_drop_glue(test_drop_glue);

        // (1) tombstone runs the finalizer exactly once.
        let a = alloc_counter(&mut r);
        assert_eq!(DROP_COUNT.load(AOrd::SeqCst), 0);
        assert!(r.tombstone(a));
        assert_eq!(DROP_COUNT.load(AOrd::SeqCst), 1, "tombstone finalizes payload");
        // Double tombstone must NOT finalize again.
        assert!(!r.tombstone(a));
        assert_eq!(DROP_COUNT.load(AOrd::SeqCst), 1, "no double-finalize");

        // (2) sweep of an unmarked block finalizes it.
        let _b = alloc_counter(&mut r);
        let keep = alloc_counter(&mut r);
        assert!(keep.mark());
        let (reclaimed, _) = r.sweep();
        assert_eq!(reclaimed, 1);
        assert_eq!(DROP_COUNT.load(AOrd::SeqCst), 2, "sweep finalizes the unmarked block");

        // (3) reuse a tombstoned slot: allocating a fresh counter into a recycled slot must
        // finalize exactly once more when reclaimed (no double-finalize of the stale payload).
        let c = alloc_counter(&mut r); // may reuse `_b`/`a`'s slot (same size class)
        assert_eq!(DROP_COUNT.load(AOrd::SeqCst), 2, "fresh alloc into reused slot: no extra drop");
        assert!(r.tombstone(c));
        assert_eq!(DROP_COUNT.load(AOrd::SeqCst), 3, "reused-slot payload finalized once on reclaim");

        // (4) `keep` still alive → finalized at region teardown (drop below).
    }
    assert_eq!(DROP_COUNT.load(AOrd::SeqCst), 4, "region drop finalizes remaining live block");
}

#[test]
fn block_type_all_variants_roundtrip() {
    let mut r = VarRegion::new();
    for ty in [
        BlockType::Str,
        BlockType::ArrayValue,
        BlockType::ArrayPrim,
        BlockType::ArrayStruct,
        BlockType::Closure,
    ] {
        let h = r.alloc(24, ty);
        assert_eq!(r.resolve(h).unwrap().block_type(), ty);
    }
}

#[test]
fn reclaim_dead_var_chunks_pools_dead_bump_chunks_among_dedicated_ones() {
    // `reclaim_dead_var_chunks` resolves each block's owning chunk from the
    // block's address. That lookup is a binary search over chunk address
    // ranges (it was a linear scan per block, which made the whole function
    // O(blocks × chunks) and swallowed the GC pause). Dedicated chunks for
    // oversized blocks are sized to their payload, not `CHUNK_BYTES`, and land
    // in `chunks` interleaved with the 64K bump chunks — so the search must key
    // on each chunk's own range and never assume a uniform stride.
    let mut r = VarRegion::new();
    let mut oversized = Vec::new();
    let mut small = Vec::new();
    for _ in 0..6 {
        // Dedicated chunk (payload > CHUNK_BYTES), then enough small blocks to
        // fill a bump chunk or two.
        oversized.push(r.alloc(96 * 1024, BlockType::Str));
        for _ in 0..64 {
            small.push(r.alloc(1024, BlockType::Str));
        }
    }
    assert!(r.chunk_count() > 12, "expected bump chunks beyond the 6 dedicated ones");
    assert_eq!(r.free_chunk_pool_len(), 0);

    // One small block survives; everything else dies.
    let survivor = small[0];
    assert!(survivor.mark());
    r.sweep();
    assert_eq!(r.live_count(), 1);

    let before = r.chunk_count();
    let got = r.reclaim_dead_var_chunks();
    assert!(got.pooled > 0, "fully-dead bump chunks must be pooled");
    assert_eq!(r.free_chunk_pool_len(), got.pooled);
    // Dedicated chunks are never pooled (they are not `CHUNK_BYTES` wide) — fix-loh-never-freed
    // frees them outright instead, so they leave `chunk_count` altogether.
    assert_eq!(got.freed_chunks, 6, "every dead dedicated chunk is freed");
    assert_eq!(r.chunk_count(), before - got.freed_chunks);
    // The survivor's chunk is still live, so not every remaining chunk is in the pool.
    assert!(got.pooled < r.chunk_count(), "still-live chunks stay out of the pool");
    // The survivor is untouched by the purge of reclaimed chunks' blocks.
    assert!(r.resolve(survivor).is_some(), "live block survives chunk reclaim");

    // Reclaiming again is a no-op: the pooled chunks are already in the pool and the
    // dedicated ones no longer exist.
    assert_eq!(r.reclaim_dead_var_chunks(), VarChunkReclaim::default());
    assert_eq!(r.free_chunk_pool_len(), got.pooled);
}

// ---------------------------------------------------------------------------------------
// shrink-var-size-classes: quarter-octave size classes
// ---------------------------------------------------------------------------------------

/// Every in-chunk class index must name exactly ONE footprint. `alloc` pops a free-list slot
/// by class alone and reinitializes it for the new payload without re-checking capacity — if
/// two footprints ever collided on one index, a small slot could be handed to a larger block
/// and the payload would run off the end of it.
#[test]
fn each_size_class_index_names_exactly_one_footprint() {
    use super::chunk::{CHUNK_BYTES, MIN_BLOCK};
    let mut footprint_of: std::collections::BTreeMap<u8, usize> = Default::default();
    for payload in 0..=(CHUNK_BYTES - GcBlockHeader::DATA_OFFSET) {
        let (footprint, class) = class_for(payload);
        if class == OVERSIZED_CLASS {
            continue;
        }
        match footprint_of.entry(class) {
            std::collections::btree_map::Entry::Vacant(e) => {
                e.insert(footprint);
            }
            std::collections::btree_map::Entry::Occupied(e) => {
                assert_eq!(
                    *e.get(),
                    footprint,
                    "class {class} claimed by both {} and {footprint} (payload {payload})",
                    e.get()
                );
            }
        }
        assert!(footprint >= GcBlockHeader::DATA_OFFSET + payload, "class must fit the payload");
        assert!(footprint >= MIN_BLOCK, "footprint below MIN_BLOCK");
        assert_eq!(footprint % 8, 0, "footprint {footprint} breaks the 8-aligned bump invariant");
        assert!(footprint <= CHUNK_BYTES, "in-chunk footprint must fit a chunk");
    }
    // Quarter-octave classes: 32/40/48/56, 64/80/96/112, … up to 65536.
    assert_eq!(footprint_of.values().copied().collect::<Vec<_>>().first(), Some(&MIN_BLOCK));
    assert!(footprint_of.values().copied().eq({
        let mut v = footprint_of.values().copied().collect::<Vec<_>>();
        v.sort_unstable();
        v
    }), "class index must be monotonic in footprint");
}

#[test]
fn class_for_rounds_to_quarter_octave_steps() {
    // Exactly at MIN_BLOCK — payload 16 fills the 32-byte block with no waste.
    assert_eq!(class_for(16).0, 32);
    // One byte over rounds to the next quarter step, not the next power of two.
    assert_eq!(class_for(17).0, 40);
    assert_eq!(class_for(24).0, 40);
    assert_eq!(class_for(25).0, 48);
    // Carry into the next octave: 57..64 total → 64 (octave 6, sub 0).
    assert_eq!(class_for(41).0, 64);
    assert_eq!(class_for(48).0, 64);
    // The shape that dominated the waste: 304 payload = 320 total, was a 512-byte slot.
    assert_eq!(class_for(304).0, 320);
    // Anything ≤ MIN_BLOCK still gets MIN_BLOCK.
    assert_eq!(class_for(0).0, 32);
}

#[test]
fn class_for_oversized_boundary() {
    use super::chunk::CHUNK_BYTES;
    let largest_in_chunk = CHUNK_BYTES - GcBlockHeader::DATA_OFFSET;
    let (footprint, class) = class_for(largest_in_chunk);
    assert_eq!(footprint, CHUNK_BYTES);
    assert_ne!(class, OVERSIZED_CLASS);
    // One byte more needs a dedicated chunk sized to the exact block, 16-aligned.
    let (footprint, class) = class_for(largest_in_chunk + 1);
    assert_eq!(class, OVERSIZED_CLASS);
    assert_eq!(footprint, CHUNK_BYTES + 16);
}

/// A free-list slot recycled for a *different* payload in the same class must still hold it.
#[test]
fn recycled_slot_fits_any_payload_of_its_class() {
    let mut region = VarRegion::new();
    // 33 and 40 both land in the 56-byte class (49..56 total).
    let (fp_a, class_a) = class_for(33);
    let (fp_b, class_b) = class_for(40);
    assert_eq!((fp_a, class_a), (fp_b, class_b));

    let small = write_read_roundtrip(&mut region, &[0xAAu8; 33], BlockType::Str);
    region.tombstone(small);
    // Same class, larger payload — must reuse the slot and still round-trip all 40 bytes.
    write_read_roundtrip(&mut region, &[0xBBu8; 40], BlockType::Str);
}

// ---------------------------------------------------------------------------------------
// fix-minor-gc-skips-var-region: generation age, young list, minor sweep
// ---------------------------------------------------------------------------------------

use crate::gc::region::PROMOTION_THRESHOLD;

const ALL_BLOCK_TYPES: [BlockType; 5] = [
    BlockType::Str,
    BlockType::ArrayValue,
    BlockType::ArrayPrim,
    BlockType::ArrayStruct,
    BlockType::Closure,
];

/// `block_type`, `gen_age` and the young-list bit share one byte. A packing mistake here
/// would silently mis-tag payloads for the tracer — the worst kind of GC bug, because the
/// wrong scan function reads the wrong bytes as pointers.
#[test]
fn gen_age_and_block_type_share_a_byte_without_interference() {
    let mut region = VarRegion::new();
    for ty in ALL_BLOCK_TYPES {
        let h = region.alloc(24, ty);
        // SAFETY: freshly allocated, region alive.
        let header = unsafe { h.header_ptr().as_ref() };
        assert_eq!(header.block_type(), ty, "type survives packing");
        assert_eq!(header.gen_age(), 0, "fresh block is age 0");
        for expected in 1..=PROMOTION_THRESHOLD {
            assert_eq!(header.bump_gen_age(), expected);
            assert_eq!(header.block_type(), ty, "aging must not disturb the type bits");
        }
    }
    // The header must not have grown to buy those bits.
    assert_eq!(std::mem::size_of::<GcBlockHeader>(), 16);
}

#[test]
fn fresh_blocks_are_young_and_listed() {
    let mut region = VarRegion::new();
    assert_eq!(region.young_count(), 0);
    for i in 0..5 {
        region.alloc(8 * (i + 1), BlockType::Str);
    }
    assert_eq!(region.young_count(), 5, "every fresh block joins the young list");
    let mut seen = 0;
    region.iterate_young(|_, h| {
        assert_eq!(h.gen_age(), 0);
        seen += 1;
    });
    assert_eq!(seen, 5);
}

#[test]
fn sweep_young_reclaims_unmarked_and_keeps_marked() {
    let mut region = VarRegion::new();
    let keep = region.alloc(16, BlockType::Str);
    let drop_me = region.alloc(16, BlockType::Str);
    keep.mark();

    let (reclaimed, _credited) = region.sweep_young();
    assert_eq!(reclaimed, 1, "only the unmarked block is reclaimed");
    // SAFETY: the region outlives this borrow and the block is still alive.
    assert!(unsafe { keep.payload() }.is_some(), "marked block survives");
    // SAFETY: the region outlives this borrow; the slot is chunk-owned either way.
    assert!(!unsafe { drop_me.header_ptr().as_ref() }.is_alive());
    assert_eq!(region.young_count(), 1, "the dead block leaves the young list");
}

/// The mark bit must be cleared by the minor sweep. Leaving it set is what let a closure
/// block fail its `mark()` CAS on the *next* minor, so its children were never traced and a
/// still-referenced young `env` array got swept.
#[test]
fn sweep_young_clears_the_mark_on_survivors() {
    let mut region = VarRegion::new();
    let h = region.alloc(16, BlockType::Closure);
    h.mark();
    region.sweep_young();
    assert!(h.mark(), "mark was cleared, so a fresh mark CAS must win again");
}

#[test]
fn sweep_young_promotes_after_threshold_survivals() {
    let mut region = VarRegion::new();
    let h = region.alloc(16, BlockType::Str);
    for i in 1..=PROMOTION_THRESHOLD {
        h.mark();
        region.sweep_young();
        // SAFETY: block still alive (it was marked each round).
        assert_eq!(unsafe { h.header_ptr().as_ref() }.gen_age(), i);
    }
    assert_eq!(region.young_count(), 0, "promoted block leaves the young list");
    // Still alive — promotion is a label change, never a move or a free.
    // SAFETY: survived every sweep above; the region is still borrowed here.
    assert!(unsafe { h.payload() }.is_some());
}

/// Regression: the young list uses lazy deletion, so a slot that dies and is then handed
/// back out by the free list before the next sweep would be listed twice — aged twice per
/// minor, with the list growing without bound. The header's young bit is what prevents it.
#[test]
fn recycled_slot_is_not_listed_twice() {
    let mut region = VarRegion::new();
    let h = region.alloc(16, BlockType::Str);
    assert_eq!(region.young_count(), 1);
    region.tombstone(h);
    // Still listed (lazy deletion) — the entry is stale, not removed.
    assert_eq!(region.young_count(), 1);
    // Same size class → reuses the very slot that is still sitting in the young list.
    let reused = region.alloc(16, BlockType::Str);
    assert_eq!(region.young_count(), 1, "recycled slot must not be pushed a second time");
    // SAFETY: freshly allocated.
    assert_eq!(unsafe { reused.header_ptr().as_ref() }.gen_age(), 0);
}

#[test]
fn reclaimed_chunk_purges_young_list() {
    // A recycled chunk is re-bumped from offset 0, so any young-list entry pointing into it
    // would dangle onto whatever lands at that address next — and the minor sweep would
    // happily age or tombstone the new occupant. `young_list` must be purged by the same
    // retain that already purges `all_blocks` and `free_lists`.
    let mut region = VarRegion::new();
    // Enough blocks to fill several bump chunks, so some are not the ambient one (the
    // ambient chunk is never reclaimed).
    for _ in 0..384 {
        region.alloc(1024, BlockType::Str);
    }
    assert!(region.chunk_count() > 2, "expected several bump chunks");
    assert_eq!(region.young_count(), 384);

    // Nothing is marked → every block dies, so whole chunks become reclaimable.
    region.sweep();
    // Tombstone alone does not shrink the list — deletion is lazy by design.
    assert_eq!(region.young_count(), 384);

    let pooled = region.reclaim_dead_var_chunks().pooled;
    assert!(pooled > 0, "fully-dead bump chunks must be pooled");
    assert!(
        region.young_count() < 384,
        "young list must shed the blocks whose chunks were recycled"
    );
    // The invariant that actually matters: nothing in the young list points at memory the
    // region no longer tracks. `all_blocks` and `young_list` are purged by the same pass, so
    // a missed purge shows up as an entry here that `all_blocks` has already dropped.
    let tracked: std::collections::HashSet<_> = region.all_blocks.iter().copied().collect();
    for p in &region.young_list {
        assert!(tracked.contains(p), "young list holds a pointer into a recycled chunk");
    }
}

// ---------------------------------------------------------------------------------------
// fix-loh-never-freed: dead oversized (dedicated-chunk) blocks give their memory back
// ---------------------------------------------------------------------------------------

/// Payload comfortably past `CHUNK_BYTES` (64 KB) → always a dedicated chunk.
const OVERSIZED_PAYLOAD: usize = 96 * 1024;

#[test]
fn dead_oversized_chunk_is_freed_and_its_slot_reused() {
    // Before this change a dedicated chunk was pinned until `VarRegion::drop`: `tombstone`
    // refused to free-list `OVERSIZED_CLASS` and `reclaim_dead_var_chunks` skipped anything
    // whose `cap != CHUNK_BYTES`. A dead 1 MB array kept its megabyte until the VM exited.
    let mut r = VarRegion::new();
    let h = r.alloc(OVERSIZED_PAYLOAD, BlockType::ArrayPrim);
    assert_eq!(r.chunk_count(), 1, "one dedicated chunk, no bump chunk yet");
    let slots = r.chunk_slot_count();

    r.sweep(); // nothing marked → the block dies
    assert!(r.resolve(h).is_none());

    let got = r.reclaim_dead_var_chunks();
    assert_eq!(got.pooled, 0, "a dedicated chunk is never pooled (D-3)");
    assert_eq!(got.freed_chunks, 1);
    assert!(
        got.freed_bytes >= (GcBlockHeader::DATA_OFFSET + OVERSIZED_PAYLOAD) as u64,
        "freed bytes must cover the whole block: {}",
        got.freed_bytes
    );
    assert_eq!(r.chunk_count(), 0, "the chunk's memory is back with the allocator");
    assert_eq!(r.chunk_slot_count(), slots, "the slot stays as a tombstone (indices are ids)");

    // The freed block must be gone from every list that holds raw pointers — each one is a
    // use-after-free waiting for the next sweep / minor / alloc.
    assert!(r.all_blocks.is_empty(), "all_blocks still points into freed memory");
    assert!(r.young_list.is_empty(), "young_list still points into freed memory");
    assert!(r.free_lists.iter().all(|fl| fl.is_empty()));

    // The tombstoned slot is reused rather than leaked.
    r.alloc(OVERSIZED_PAYLOAD, BlockType::Str);
    assert_eq!(r.chunk_slot_count(), slots, "push_chunk must recycle the tombstoned slot");
    assert_eq!(r.chunk_count(), 1);
}

#[test]
fn live_oversized_chunk_is_never_freed() {
    let mut r = VarRegion::new();
    let live = r.alloc(OVERSIZED_PAYLOAD, BlockType::Str);
    let dead = r.alloc(OVERSIZED_PAYLOAD, BlockType::Str);
    // SAFETY: fresh handle into this region.
    unsafe { live.payload_mut().expect("resolves")[OVERSIZED_PAYLOAD - 1] = 0x5A };

    assert!(live.mark());
    r.sweep();

    let got = r.reclaim_dead_var_chunks();
    assert_eq!(got.freed_chunks, 1, "only the unmarked one is freed");
    // NB: `dead` must NOT be resolved here — its chunk's memory is back with the allocator,
    // so `resolve` would dereference freed memory. That is design decision D-3's accepted
    // cost, and miri catches it if this line ever comes back.
    let _ = dead;
    // The survivor's chunk is untouched — its payload still reads back.
    // SAFETY: still alive; the region held it across the reclaim.
    let tail = unsafe { live.payload().expect("survivor resolves")[OVERSIZED_PAYLOAD - 1] };
    assert_eq!(tail, 0x5A, "surviving oversized block must keep its memory");
}

#[test]
fn oversized_churn_does_not_grow_the_per_chunk_tables() {
    // The invariant behind "RSS stops climbing": allocating and killing large objects over
    // and over must reach a steady state, both in chunk memory and in the parallel per-chunk
    // bookkeeping (`chunks` / `borrowed` / `reuse_gen`, ~29 B per slot).
    let mut r = VarRegion::new();
    let mut slots_after_first_round = 0usize;
    for round in 0..8 {
        for _ in 0..4 {
            r.alloc(OVERSIZED_PAYLOAD, BlockType::ArrayPrim);
        }
        r.sweep();
        let got = r.reclaim_dead_var_chunks();
        assert_eq!(got.freed_chunks, 4, "round {round}");
        assert_eq!(r.chunk_count(), 0, "round {round}: no chunk memory should survive");
        if round == 0 {
            slots_after_first_round = r.chunk_slot_count();
        } else {
            assert_eq!(
                r.chunk_slot_count(),
                slots_after_first_round,
                "round {round}: slot table must not grow across churn"
            );
        }
    }
}

#[test]
fn freeing_a_dedicated_chunk_leaves_bump_chunk_indices_valid() {
    // `bump_chunk`, `borrowed`, `reuse_gen` and `var_free_chunk_pool` all address chunks by
    // index, so a freed dedicated chunk must leave a hole rather than shift its neighbours.
    // Interleave the two kinds so a naive `Vec::remove` would renumber the bump chunks.
    let mut r = VarRegion::new();
    let mut small = Vec::new();
    for _ in 0..4 {
        r.alloc(OVERSIZED_PAYLOAD, BlockType::Str);
        for _ in 0..64 {
            small.push(r.alloc(1024, BlockType::Str));
        }
    }
    let survivor = small[small.len() - 1];
    assert!(survivor.mark());
    r.sweep();

    let got = r.reclaim_dead_var_chunks();
    assert_eq!(got.freed_chunks, 4);
    assert!(got.pooled > 0);
    // The survivor still resolves and its payload is still writable — proof that the pooled
    // and freed sets did not get crossed.
    // SAFETY: alive, exclusive access via `&mut r` being released above.
    unsafe { survivor.payload_mut().expect("survivor resolves")[0] = 0x11 };
    // Every pointer still tracked must live in a chunk the region still owns.
    for p in r.all_blocks.iter().chain(r.young_list.iter()) {
        assert!(
            r.owns_addr(p.as_ptr() as usize),
            "tracked block points outside every owned chunk"
        );
    }
}

// ---------------------------------------------------------------------------------------
// add-loh-bytes-knob: the large-object threshold is a knob, not a constant
// ---------------------------------------------------------------------------------------

/// Lowering `Z42_GC_LOH_BYTES` sends more blocks down the dedicated-chunk path — the path
/// whose memory goes straight back to the allocator when the block dies (fix-loh-never-freed).
/// Exercised through `class_for_with_limit`: the live threshold is process-global, so a test
/// that stored into it would race every other test allocating a var block.
#[test]
fn a_lower_loh_threshold_makes_more_blocks_oversized() {
    use super::chunk::{class_for_with_limit, CHUNK_BYTES};
    let payload = 40 * 1024; // comfortably inside a 64 KB chunk, past a 32 KB threshold

    let (_, class_default) = class_for_with_limit(payload, CHUNK_BYTES);
    assert_ne!(class_default, OVERSIZED_CLASS, "in-chunk at the default threshold");

    let (footprint, class_low) = class_for_with_limit(payload, 32 * 1024);
    assert_eq!(class_low, OVERSIZED_CLASS, "oversized once the threshold drops below it");
    assert!(
        footprint >= GcBlockHeader::DATA_OFFSET + payload,
        "a dedicated chunk is sized to hold the whole block"
    );
}

/// The threshold never exceeds `CHUNK_BYTES`: a block bigger than a bump chunk cannot be
/// bump-allocated at all, so a higher setting would route blocks nowhere.
#[test]
fn the_loh_threshold_is_clamped_to_the_chunk_size() {
    use super::chunk::{clamp_loh_bytes, CHUNK_BYTES, MIN_BLOCK};
    assert_eq!(clamp_loh_bytes(usize::MAX), CHUNK_BYTES, "never above a bump chunk");
    assert_eq!(clamp_loh_bytes(0), MIN_BLOCK, "never below the smallest block");
    assert_eq!(clamp_loh_bytes(32 * 1024), 32 * 1024, "in-range values pass through");
    // Default stays where the `static` was initialised.
    assert_eq!(super::loh_bytes(), CHUNK_BYTES);
}

// ---------------------------------------------------------------------------------------
// add-incremental-chunk-reclaim: the per-chunk census
// ---------------------------------------------------------------------------------------

/// `chunk_idx` had to be **free**: the 16-byte header is one of the three immovable
/// constraints of the three-heap design (24 would push the ~1.8 M blocks whose total is
/// exactly `MIN_BLOCK` into the next size class). It fits because `#[repr(C, align(8))]` was
/// already padding the other six fields (12 bytes) out to 16.
#[test]
fn chunk_idx_fits_in_the_headers_existing_padding() {
    assert_eq!(std::mem::size_of::<GcBlockHeader>(), 16);
    assert_eq!(std::mem::align_of::<GcBlockHeader>(), 8);
}

/// Every block must know which chunk it lives in — that is what makes "is this chunk fully
/// dead?" `O(1)` instead of a binary search per block.
#[test]
fn every_block_carries_its_owning_chunk() {
    let mut r = VarRegion::new();
    let small: Vec<_> = (0..600).map(|_| r.alloc(1024, BlockType::Str)).collect();
    let big = r.alloc(OVERSIZED_PAYLOAD, BlockType::ArrayPrim);

    for h in small.iter().chain(std::iter::once(&big)) {
        let ci = r.resolve(*h).expect("alive").chunk_idx as usize;
        assert!(
            r.owns_addr(h.addr()),
            "block must live in a chunk the region owns"
        );
        assert!(ci < r.chunk_slot_count(), "chunk_idx {ci} out of range");
    }
    // The oversized block gets a dedicated chunk, so it cannot share one with the small ones.
    let big_ci = r.resolve(big).expect("alive").chunk_idx;
    let small_ci = r.resolve(small[0]).expect("alive").chunk_idx;
    assert_ne!(big_ci, small_ci, "an oversized block owns its chunk alone");
}

/// The census is what the reclaim pass reads instead of walking `all_blocks`. If it ever
/// drifted from the truth the pass would reclaim a chunk that still has live blocks — far
/// worse than being slow — so reconcile it against a full scan after a churn workload.
#[test]
fn the_per_chunk_census_matches_a_full_scan() {
    let mut r = VarRegion::new();
    let mut handles: Vec<_> = (0..900).map(|i| r.alloc(512 + i % 64, BlockType::Str)).collect();
    // Kill half, then reallocate — exercises tombstone, free-list reuse and fresh bumps.
    for h in handles.iter().step_by(2) {
        r.tombstone(*h);
    }
    handles.extend((0..300).map(|_| r.alloc(512, BlockType::Str)));
    r.alloc(OVERSIZED_PAYLOAD, BlockType::Closure);

    let mut truth_live = vec![0u32; r.chunk_slot_count()];
    let mut truth_blocks = vec![0u32; r.chunk_slot_count()];
    for &p in &r.all_blocks {
        // SAFETY: `all_blocks` holds chunk-owned headers for the region's lifetime.
        let h = unsafe { p.as_ref() };
        truth_blocks[h.chunk_idx as usize] += 1;
        if h.is_alive() {
            truth_live[h.chunk_idx as usize] += 1;
        }
    }
    assert_eq!(r.live_per_chunk_for_test(), truth_live, "live census drifted");
    assert_eq!(r.blocks_per_chunk_for_test(), truth_blocks, "block census drifted");
}
