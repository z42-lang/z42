//! Object model R6: objects are variable-length GC blocks — block header, object header and
//! fields in one allocation. Layout and size class, the remembered set that replaces object
//! cards, weak references across chunk reuse (including a large object's dedicated chunk),
//! finalizers / soft references through the side tables, drop glue, and the detached
//! (`OwnedObject`) form used by stack objects.

use super::*;
use crate::gc::region::PROMOTION_THRESHOLD;
use crate::gc::var_region::{class_for, BlockType, GcBlockHeader};
use crate::gc::{GcMode, ObjRef};
use crate::metadata::types::{OwnedObject, ScriptObject, OBJ_HEADER_BYTES};

fn generational_heap() -> ArcMagrGC {
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::GenerationalMarkSweep);
    heap.set_nursery_bytes_for_test(1 << 40);
    heap
}

fn stw_heap() -> ArcMagrGC {
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::StwMarkSweep);
    heap.set_nursery_bytes_for_test(1 << 40);
    heap
}

fn obj_of(v: &Value) -> ObjRef {
    match v {
        Value::Object(o) => *o,
        other => panic!("expected an object, got {other:?}"),
    }
}

/// An object whose field area is exactly `n` bytes (the boxed-primitive shape).
fn sized(heap: &ArcMagrGC, n: usize) -> ObjRef {
    obj_of(&heap.alloc_boxed_prim(dummy_type_desc("Sized"), vec![0u8; n].into_boxed_slice()))
}

#[test]
fn an_object_is_one_block_header_then_fields() {
    let heap = stw_heap();
    let o = sized(&heap, 8);
    let v = o.var_ref();
    assert_eq!(v.block_type(), BlockType::Object);
    assert_eq!(v.payload_size(), OBJ_HEADER_BYTES + 8, "payload = object header + fields");
    let hdr = ObjRef::as_ptr(&o) as usize;
    assert_eq!(hdr, v.addr() + GcBlockHeader::DATA_OFFSET, "the object header opens the payload");
    assert_eq!(o.borrow().cells().base_ptr() as usize, hdr + OBJ_HEADER_BYTES, "fields follow it");
    assert_eq!(o.borrow().payload_len(), 8);
}

#[test]
fn small_objects_land_in_the_smallest_size_classes() {
    // 16 B block header + 16 B object header + fields, rounded to a quarter-octave class.
    assert_eq!(class_for(OBJ_HEADER_BYTES).0, 32, "a field-less object is 32 B");
    assert_eq!(class_for(OBJ_HEADER_BYTES + 4).0, 40, "`{{int}}` is 40 B");
    assert_eq!(class_for(OBJ_HEADER_BYTES + 8).0, 40, "`{{long}}` is 40 B");
    assert_eq!(class_for(OBJ_HEADER_BYTES + 16).0, 48, "`{{int; string}}` is 48 B");
    // And the heap charges exactly the block it carved.
    let heap = stw_heap();
    let before = heap.used_bytes();
    let o = sized(&heap, 4);
    assert_eq!(heap.used_bytes() - before, (GcBlockHeader::DATA_OFFSET + OBJ_HEADER_BYTES + 4) as u64);
    assert_eq!(heap.object_size_bytes(&Value::Object(o)), GcBlockHeader::DATA_OFFSET + OBJ_HEADER_BYTES + 4);
}

#[test]
fn a_dead_object_drops_its_type_descriptor() {
    let heap = stw_heap();
    let td = dummy_type_desc("Counted");
    let base = Arc::strong_count(&td);
    for _ in 0..10 {
        let _ = heap.alloc_object(td.clone(), vec![], NativeData::None);
    }
    assert_eq!(Arc::strong_count(&td), base + 10, "every object header holds the descriptor");
    heap.force_collect();
    assert_eq!(Arc::strong_count(&td), base, "the drop glue released every dead header's Arc");
}

#[test]
fn a_dead_native_backed_object_frees_its_extras() {
    let heap = stw_heap();
    let monitor = Arc::new(crate::corelib::monitor::Monitor::default());
    let probe = Arc::downgrade(&monitor);
    let _ = heap.alloc_object(dummy_type_desc("Mutex"), vec![], NativeData::Monitor(monitor));
    assert!(probe.upgrade().is_some());
    heap.force_collect();
    assert!(probe.upgrade().is_none(), "the extras box (and the Arc it holds) died with the block");
}

// ── remembered set ───────────────────────────────────────────────────────────

/// An old object reachable only from another old object: a minor traces neither (a pinned
/// root is traced *through*, but its old children are not queued), so the remembered set is the
/// only thing that can reach what the inner one holds.
fn old_pair(heap: &ArcMagrGC) -> (Value, Value) {
    let holder = heap.alloc_object(dummy_type_desc("Holder"), vec![], NativeData::None);
    let owner = heap.alloc_object(dummy_type_desc("Owner"), vec![], NativeData::None);
    obj_of(&holder).borrow().set_ref_raw_for_test(0, &owner);
    let _ = heap.pin_root(holder);
    for _ in 0..PROMOTION_THRESHOLD {
        heap.force_collect();
    }
    assert!(ObjRef::gen_age(&obj_of(&owner)) >= PROMOTION_THRESHOLD, "owner is old");
    (holder, owner)
}

#[test]
fn barrier_remembers_an_old_object_once_and_a_minor_keeps_its_young_child() {
    let heap = generational_heap();
    let (_holder, owner) = old_pair(&heap);
    let o = obj_of(&owner);
    assert!(!o.var_ref().is_remembered());

    let child = heap.alloc_object(dummy_type_desc("Child"), vec![], NativeData::None);
    let weak_child = heap.make_weak(&child).expect("object");
    o.borrow().set_ref_raw_for_test(0, &child);
    heap.write_barrier_field(&owner, 0, &child);
    heap.write_barrier_field(&owner, 0, &child);
    assert!(o.var_ref().is_remembered(), "old → young store remembers the owner");
    assert_eq!(heap.remembered_len_for_test(), 1, "the bit makes the push happen once");

    // Nothing reaches the child but the old owner, which nothing young reaches.
    heap.force_collect();
    assert!(heap.upgrade_weak(&weak_child).is_some(), "the minor re-rooted from the remembered owner");
    assert!(o.var_ref().is_remembered(), "still holds a young child → stays remembered");
    assert!(heap.verify_card_invariant().is_ok());

    // Once the child is old too, a minor drops the owner from the set.
    for _ in 0..PROMOTION_THRESHOLD {
        heap.force_collect();
    }
    assert!(!o.var_ref().is_remembered(), "nothing young left → forgotten");
    assert_eq!(heap.remembered_len_for_test(), 0);
    assert!(heap.upgrade_weak(&weak_child).is_some());
}

#[test]
fn promotion_remembers_an_object_that_becomes_old_holding_a_young_one() {
    let heap = generational_heap();
    // An old, pinned holder; a young owner stored into it (the holder is remembered for it).
    let holder = heap.alloc_object(dummy_type_desc("Holder"), vec![], NativeData::None);
    let _pin = heap.pin_root(holder);
    for _ in 0..PROMOTION_THRESHOLD {
        heap.force_collect();
    }
    let owner = heap.alloc_object(dummy_type_desc("Owner"), vec![], NativeData::None);
    obj_of(&holder).borrow().set_ref_raw_for_test(0, &owner);
    heap.write_barrier_field(&holder, 0, &owner);
    // One minor short of promoting the owner, then hand it a fresh child — young → young, so
    // no barrier record.
    for _ in 0..(PROMOTION_THRESHOLD - 1) {
        heap.force_collect();
    }
    let child = heap.alloc_object(dummy_type_desc("Child"), vec![], NativeData::None);
    let weak_child = heap.make_weak(&child).expect("object");
    obj_of(&owner).borrow().set_ref_raw_for_test(0, &child);
    heap.write_barrier_field(&owner, 0, &child);
    assert!(!obj_of(&owner).var_ref().is_remembered(), "young owner: nothing to remember");

    heap.force_collect(); // promotes the owner while its child is age 1
    assert!(obj_of(&owner).var_ref().is_remembered(), "promotion did the barrier's job");
    assert!(heap.verify_card_invariant().is_ok());
    heap.force_collect();
    assert!(heap.upgrade_weak(&weak_child).is_some(),
        "the child survives the next minor — only the remembered owner reaches it");
}

/// Lowering the promotion line makes a cohort old at once; one of them already holding a young
/// child got there with no barrier record (it was young when it was written). The lowering must
/// remember it — and only needs to look at the cohort, not at objects that were old before.
#[test]
fn lowering_the_promotion_line_remembers_the_cohort_that_holds_young_children() {
    let heap = generational_heap();
    let old = heap.alloc_object(dummy_type_desc("Old"), vec![], NativeData::None);
    let _p0 = heap.pin_root(old);
    for _ in 0..PROMOTION_THRESHOLD {
        heap.force_collect();
    }
    let owner = heap.alloc_object(dummy_type_desc("Owner"), vec![], NativeData::None);
    let _p1 = heap.pin_root(owner);
    for _ in 0..(PROMOTION_THRESHOLD - 1) {
        heap.force_collect(); // owner at the age just below the line
    }
    let child = heap.alloc_object(dummy_type_desc("Child"), vec![], NativeData::None);
    obj_of(&owner).borrow().set_ref_raw_for_test(0, &child);
    heap.write_barrier_field(&owner, 0, &child);
    assert!(!obj_of(&owner).var_ref().is_remembered(), "young → young: no barrier record");

    heap.apply_promotion_age_for_test(PROMOTION_THRESHOLD - 1);
    assert!(obj_of(&owner).var_ref().is_remembered(), "the cohort's old → young edge is remembered");
    assert!(!obj_of(&old).var_ref().is_remembered(), "an object with no young child is not");
    assert_eq!(heap.remembered_len_for_test(), 1);
    assert!(heap.verify_card_invariant().is_ok());
}

#[test]
fn a_remembered_object_that_dies_leaves_the_set_before_its_chunk_is_reclaimed() {
    let heap = generational_heap();
    let owner = heap.alloc_object(dummy_type_desc("Owner"), vec![], NativeData::None);
    let pin = heap.pin_root(owner);
    for _ in 0..PROMOTION_THRESHOLD {
        heap.force_collect();
    }
    let child = heap.alloc_object(dummy_type_desc("Child"), vec![], NativeData::None);
    obj_of(&owner).borrow().set_ref_raw_for_test(0, &child);
    heap.write_barrier_field(&owner, 0, &child);
    assert_eq!(heap.remembered_len_for_test(), 1);
    heap.unpin_root(pin);
    // A major reclaims the old owner; the flagged-dead drain drops it from the set.
    heap.run_cycle_collection_major();
    assert_eq!(heap.remembered_len_for_test(), 0, "a dead object is not left in the remembered set");
    heap.force_collect(); // a minor must not trip over anything stale
}

// ── weak references ─────────────────────────────────────────────────────────

#[test]
fn a_weak_reference_to_a_dead_object_reads_none_after_its_chunk_is_reused() {
    let heap = stw_heap();
    let dead = heap.alloc_object(dummy_type_desc("Dead"), vec![], NativeData::None);
    let weak = heap.make_weak(&dead).expect("object");
    assert!(heap.upgrade_weak(&weak).is_some());
    heap.force_collect(); // the object dies: its slot is free-listed or its chunk pooled
    assert!(heap.upgrade_weak(&weak).is_none());
    // Refill: the old address now holds another object, or sits inside another block.
    let keep: Vec<_> = (0..4000)
        .map(|_| heap.pin_root(heap.alloc_object(dummy_type_desc("Fill"), vec![], NativeData::None)))
        .collect();
    assert!(heap.upgrade_weak(&weak).is_none(), "the chunk's reuse epoch moved — never resurrected");
    let _ = keep;
}

#[test]
fn a_weak_reference_to_a_dead_large_object_reads_none_after_its_chunk_is_freed() {
    let heap = stw_heap();
    // Past the large-object threshold: the object gets a dedicated chunk, returned to the
    // allocator outright when it dies.
    let big = sized(&heap, 2 * crate::gc::var_region::loh_bytes());
    let addr = big.var_ref().addr();
    let weak = heap.make_weak(&Value::Object(big)).expect("object");
    assert!(heap.upgrade_weak(&weak).is_some());
    assert!(heap.region_var_for_test().lock().owns_addr(addr));
    heap.force_collect();
    assert!(!heap.region_var_for_test().lock().owns_addr(addr), "the dedicated chunk was freed");
    assert!(heap.upgrade_weak(&weak).is_none(), "upgrade reads the epoch, never the freed memory");
    // The chunk slot is reused by the next large object; the old reference stays dead.
    let again = sized(&heap, 2 * crate::gc::var_region::loh_bytes());
    let _pin = heap.pin_root(Value::Object(again));
    assert!(heap.upgrade_weak(&weak).is_none());
}

#[test]
fn a_weak_handle_slot_to_an_object_clears_when_it_dies() {
    let heap = stw_heap();
    let o = heap.alloc_object(dummy_type_desc("W"), vec![], NativeData::None);
    let slot = heap.handle_alloc(&o, GcHandleKind::Weak);
    assert!(matches!(heap.handle_target(slot), Some(Value::Object(_))));
    heap.force_collect();
    assert!(heap.handle_target(slot).is_none());
}

// ── finalizers / soft references ────────────────────────────────────────────

#[test]
fn an_object_finalizer_fires_once_when_a_minor_reclaims_it() {
    let heap = generational_heap();
    let hits = Arc::new(AtomicUsize::new(0));
    let o = heap.alloc_object(dummy_type_desc("F"), vec![], NativeData::None);
    let h = Arc::clone(&hits);
    heap.register_finalizer(&o, Arc::new(move || { h.fetch_add(1, Ordering::SeqCst); }));
    assert_eq!(heap.stats().finalizers_pending, 1);
    heap.force_collect(); // minor
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    assert_eq!(heap.stats().finalizers_pending, 0);
    heap.force_collect();
    assert_eq!(hits.load(Ordering::SeqCst), 1, "fire-once");
}

#[test]
fn a_cancelled_object_finalizer_never_fires_and_finalize_now_fires_it_eagerly() {
    let heap = stw_heap();
    let hits = Arc::new(AtomicUsize::new(0));
    let a = heap.alloc_object(dummy_type_desc("A"), vec![], NativeData::None);
    let h = Arc::clone(&hits);
    heap.register_finalizer(&a, Arc::new(move || { h.fetch_add(1, Ordering::SeqCst); }));
    heap.cancel_finalizer(&a);
    heap.force_collect();
    assert_eq!(hits.load(Ordering::SeqCst), 0);

    let b = heap.alloc_object(dummy_type_desc("B"), vec![], NativeData::None);
    let _pin = heap.pin_root(b);
    let h = Arc::clone(&hits);
    heap.register_finalizer(&b, Arc::new(move || { h.fetch_add(1, Ordering::SeqCst); }));
    assert!(heap.finalize_now(&b));
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    assert!(!obj_of(&b).var_ref().is_live(), "finalize_now tombstones the object");
}

#[test]
fn a_soft_referenced_object_survives_without_pressure_and_its_entry_dies_with_it() {
    let heap = stw_heap();
    let o = heap.alloc_object(dummy_type_desc("S"), vec![], NativeData::None);
    let key = heap.register_soft_ref(&o);
    heap.force_collect(); // no cap ⇒ no pressure ⇒ revived
    assert!(matches!(heap.soft_ref_get(key), Value::Object(_)));
    // Under pressure (a cap already exceeded) the soft target is reclaimed and its entry with it.
    heap.set_max_heap_bytes(Some(1));
    heap.force_collect();
    assert!(matches!(heap.soft_ref_get(key), Value::Null));
    heap.unregister_soft_ref(key);
}

// ── detached objects (stack-allocated) ──────────────────────────────────────

#[test]
fn an_owned_object_reads_like_a_heap_object() {
    let heap = stw_heap();
    let td = dummy_type_desc("Twin");
    let leaf = heap.alloc_object(dummy_type_desc("Leaf"), vec![], NativeData::None);
    let _pin = heap.pin_root(leaf);

    let owned = OwnedObject::of_type(td.clone());
    owned.set_field_value(0, &leaf);
    owned.set_field_value(1, &Value::I64(42));
    let heaped = obj_of(&heap.alloc_object(td, vec![leaf, Value::I64(42)], NativeData::None));

    let (a, b) = (&*owned, heaped.borrow());
    assert_eq!(a.payload_len(), b.payload_len());
    assert_eq!(
        a.cells().base_ptr() as usize - (a as *const ScriptObject).cast::<u8>() as usize,
        b.cells().base_ptr() as usize - (b as *const ScriptObject).cast::<u8>() as usize,
        "fields sit at the same offset from the header"
    );
    assert!(a.field_value(0) == b.field_value(0));
    assert!(matches!(a.field_value(1), Value::I64(42)) && matches!(b.field_value(1), Value::I64(42)));
}
