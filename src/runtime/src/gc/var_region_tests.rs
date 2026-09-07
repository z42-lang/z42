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

    let pooled = r.reclaim_dead_var_chunks();
    assert!(pooled > 0, "fully-dead bump chunks must be pooled");
    assert_eq!(r.free_chunk_pool_len(), pooled);
    // Dedicated chunks are never pooled (they are not `CHUNK_BYTES` wide), and
    // the survivor's chunk is still live — so not every chunk is reclaimable.
    assert!(pooled < r.chunk_count(), "dedicated + still-live chunks stay out of the pool");
    // The survivor is untouched by the purge of reclaimed chunks' blocks.
    assert!(r.resolve(survivor).is_some(), "live block survives chunk reclaim");

    // Reclaiming again is a no-op: the pooled chunks are already in the pool.
    assert_eq!(r.reclaim_dead_var_chunks(), 0);
    assert_eq!(r.free_chunk_pool_len(), pooled);
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
