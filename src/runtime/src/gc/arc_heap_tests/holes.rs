//! M12: the TLAB reuses dead slots of partially-live variable-length chunks
//! (`var_region::holes`). These drive an **armed** thread, so allocation takes the TLAB path —
//! the one that reuses holes — and check what reuse must preserve: weak references, finalizers,
//! allocate-black during an incremental cycle, and the census the chunk reclaim relies on.

use super::*;
use crate::gc::GcMode;
use std::collections::HashSet;
use std::sync::Arc as StdArc;

/// Arms the thread for the scope; retires its TLAB into `heap` and disarms on drop.
struct Armed<'a>(&'a ArcMagrGC);
impl<'a> Armed<'a> {
    fn new(heap: &'a ArcMagrGC) -> Self {
        crate::gc::tlab::arm();
        Armed(heap)
    }
}
impl Drop for Armed<'_> {
    fn drop(&mut self) {
        self.0.retire_thread_tlab();
        crate::gc::tlab::disarm();
    }
}

/// `n`, scaled down under Miri (these tests are about shape, not volume).
const fn scaled(n: usize) -> usize {
    if cfg!(miri) { n / 20 } else { n }
}

fn obj(heap: &ArcMagrGC, name: &str) -> Value {
    heap.alloc_object(dummy_type_desc(name), vec![Value::Null], NativeData::None)
}

fn addr_of(v: &Value) -> usize {
    let Value::Object(o) = v else { panic!("object expected") };
    o.var_ref().addr()
}

/// `n` objects, every other one pinned; after a full collection the unpinned half are holes in
/// chunks that stay live. Returns the pins and the dead addresses.
fn half_dead(heap: &ArcMagrGC, n: usize) -> (Vec<crate::gc::RootHandle>, HashSet<usize>) {
    let mut pins = Vec::new();
    let mut dead = HashSet::new();
    for i in 0..n {
        let v = obj(heap, "H");
        if i % 2 == 0 {
            pins.push(heap.pin_root(v));
        } else {
            dead.insert(addr_of(&v));
        }
    }
    heap.force_collect();
    (pins, dead)
}

#[test]
fn the_tlab_fills_holes_before_carving_new_chunks() {
    let heap = ArcMagrGC::new();
    let _g = Armed::new(&heap);
    let (_pins, dead) = half_dead(&heap, scaled(4000));
    let chunks = heap.region_var_for_test().lock().chunk_count();
    let again: Vec<Value> = (0..dead.len()).map(|_| obj(&heap, "H")).collect();
    let reused = again.iter().filter(|v| dead.contains(&addr_of(v))).count();
    assert_eq!(reused, dead.len(), "every new object of the same class lands in a hole");
    heap.retire_thread_tlab();
    assert_eq!(heap.region_var_for_test().lock().chunk_count(), chunks, "no chunk was added");
    #[cfg(debug_assertions)]
    heap.debug_validate_invariants();
}

#[test]
#[cfg_attr(miri, ignore)] // ten full collections over 30 000 objects: a policy check, no new unsafe
fn churn_with_survivors_reaches_a_steady_chunk_count() {
    // A ring of long-lived objects, half of each round's allocations replacing a random member:
    // the replaced ones die scattered over old chunks, which therefore never die whole. Without
    // hole reuse every round carves fresh chunks and the count climbs; with it the count settles.
    const RING: usize = 3000;
    let heap = ArcMagrGC::new();
    let _g = Armed::new(&heap);
    let mut ring: Vec<crate::gc::RootHandle> = (0..RING).map(|_| heap.pin_root(obj(&heap, "C"))).collect();
    let mut seed = 12345u64;
    let mut counts = Vec::new();
    for _ in 0..10 {
        for _ in 0..RING {
            let v = obj(&heap, "C");
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            if (seed >> 33) % 2 == 0 {
                let slot = ((seed >> 40) as usize) % RING;
                heap.unpin_root(std::mem::replace(&mut ring[slot], heap.pin_root(v)));
            }
        }
        heap.collect_full(); // a minor never reaches the ring members that died old
        counts.push(heap.region_var_for_test().lock().chunk_count());
    }
    assert!(counts[9] <= counts[3], "the chunk count keeps climbing under churn: {counts:?}");
}

#[test]
fn a_weak_reference_never_resolves_to_the_object_reusing_its_slot() {
    let heap = ArcMagrGC::new();
    let _g = Armed::new(&heap);
    let _keep = heap.pin_root(obj(&heap, "Neighbour")); // keeps the chunk from dying whole
    let victim = obj(&heap, "Victim");
    let victim_addr = addr_of(&victim);
    let weak = heap.make_weak(&victim).unwrap();
    // `victim` is a plain copy of the handle, not a root: the collection below reclaims it.
    heap.force_collect();
    assert!(heap.upgrade_weak(&weak).is_none());
    let mut found = None;
    for _ in 0..64 {
        let v = obj(&heap, "Victim");
        if addr_of(&v) == victim_addr {
            found = Some(v);
            break;
        }
    }
    let successor = found.expect("the victim's slot is reused");
    let _pin = heap.pin_root(successor.clone());
    assert!(heap.upgrade_weak(&weak).is_none(), "the old weak reference stays dead");
    let fresh = heap.make_weak(&successor).unwrap();
    assert!(heap.upgrade_weak(&fresh).is_some());
}

#[test]
fn a_finalized_objects_slot_is_reused_only_after_its_finalizer_ran() {
    let heap = ArcMagrGC::new();
    let _g = Armed::new(&heap);
    let _keep = heap.pin_root(obj(&heap, "Neighbour"));
    let fired = StdArc::new(std::sync::atomic::AtomicUsize::new(0));
    let victim = obj(&heap, "Victim");
    let victim_addr = addr_of(&victim);
    let f = fired.clone();
    heap.register_finalizer(&victim, StdArc::new(move || {
        f.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }));
    // `victim` is a plain copy of the handle, not a root: the collection below reclaims it.
    heap.force_collect();
    assert_eq!(fired.load(std::sync::atomic::Ordering::SeqCst), 1);
    let successor = (0..64).map(|_| obj(&heap, "Victim")).find(|v| addr_of(v) == victim_addr)
        .expect("the finalized object's slot is reused");
    let Value::Object(s) = &successor else { unreachable!() };
    assert!(heap.take_object_finalizer(s).is_none(), "the successor inherits no finalizer");
    let _pin = heap.pin_root(successor.clone());
    heap.force_collect();
    assert_eq!(fired.load(std::sync::atomic::Ordering::SeqCst), 1, "fired once, for the dead object only");
}

#[test]
fn a_hole_filled_during_an_incremental_cycle_is_allocated_black() {
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::GenerationalMarkSweep);
    heap.set_nursery_bytes_for_test(1 << 40);
    let _g = Armed::new(&heap);
    let (_pins, dead) = half_dead(&heap, scaled(2000));
    for _ in 0..scaled(700) {
        obj(&heap, "G"); // garbage enough for a cycle that spans slices
    }
    assert!(!heap.run_major_slice_for_test(1));
    assert!(heap.major_cycle_marking_for_test());
    let born = (0..dead.len()).map(|_| obj(&heap, "Born")).find(|v| dead.contains(&addr_of(v)))
        .expect("an allocation during the mark lands in a hole");
    let Value::Object(b) = &born else { unreachable!() };
    assert!(b.var_ref().is_marked(heap.major_mark_for_test()), "a reused slot is born black");
    let weak = heap.make_weak(&born).unwrap();
    // Unrooted from here: only its allocation color can carry it through the cycle.
    while !heap.run_major_slice_for_test(300) {}
    assert!(heap.upgrade_weak(&weak).is_some(), "it survives the cycle it was born in");
    #[cfg(debug_assertions)]
    heap.debug_validate_invariants();
}
