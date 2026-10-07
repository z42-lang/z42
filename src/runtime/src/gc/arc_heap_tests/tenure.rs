//! M9 tenure (`arc_heap/young_policy.rs`): the young generation promoted without a mark — what it
//! keeps, what it leaves to the major, and how it sits inside an open incremental cycle.
//! Liveness is observed through weak references, never by dereferencing a possibly-swept handle.

use super::*;
use crate::gc::GcMode;

const WHOLE: u64 = u64::MAX;

fn generational_heap() -> ArcMagrGC {
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::GenerationalMarkSweep);
    // No inline auto-collections in the middle of a hand-driven interleaving.
    heap.set_nursery_bytes_for_test(1 << 40);
    heap
}

fn obj(heap: &ArcMagrGC, name: &str) -> Value {
    heap.alloc_object(dummy_type_desc(name), vec![Value::Null], NativeData::None)
}

fn link(from: &Value, to: &Value) {
    let Value::Object(f) = from else { panic!("object") };
    f.borrow_mut().set_ref_slot(0, to);
}

fn raw_alive(weak: &crate::gc::WeakRef) -> bool {
    match &weak.inner {
        crate::gc::types::WeakRefInner::Object(w) => w.upgrade().is_some(),
        crate::gc::types::WeakRefInner::Array(w) => w.upgrade().is_some(),
    }
}

#[test]
fn a_tenure_empties_the_young_generation_and_reclaims_nothing() {
    let heap = generational_heap();
    let live: Vec<_> = (0..40).map(|_| heap.pin_root(obj(&heap, "Live"))).collect();
    let garbage: Vec<_> = (0..60).map(|_| heap.make_weak(&obj(&heap, "G")).unwrap()).collect();
    assert_eq!(heap.young_count(), 100);

    let promoted = heap.run_tenure();
    assert_eq!(heap.young_count(), 0, "everything listed leaves the young generation");
    assert!(promoted > 0 && heap.promoted_bytes_for_test() >= promoted,
        "tenured bytes count towards the next major's trigger");
    assert!(garbage.iter().all(raw_alive), "a tenure reclaims nothing — that is the major's job now");
    #[cfg(debug_assertions)]
    heap.debug_validate_invariants();

    // The next minor has nothing young to look at; the next major takes the tenured garbage.
    heap.run_cycle_collection_minor();
    assert!(garbage.iter().all(raw_alive), "old garbage is out of a minor's reach");
    assert!(heap.run_major_slice_for_test(WHOLE));
    assert!(garbage.iter().all(|w| !raw_alive(w)), "the major reclaims what was tenured dead");
    #[cfg(debug_assertions)]
    heap.debug_validate_invariants();
    drop(live);
}

/// Promotion normally has to dirty a card for every newly-old entry still pointing at something
/// young. A tenure leaves nothing young, so it skips that work — and the card invariant still
/// holds for every write after it: old holder → new young object goes through the barrier.
#[test]
fn after_a_tenure_new_young_objects_are_carried_by_the_write_barrier() {
    let heap = generational_heap();
    let holder = obj(&heap, "Holder");
    let _pin = heap.pin_root(holder.clone());
    heap.run_tenure();
    assert!(ArcMagrGC::gen_age_of(&holder) >= heap.promotion_age(), "the holder is old now");

    let y = obj(&heap, "Young");
    let weak_y = heap.make_weak(&y).unwrap();
    link(&holder, &y);
    heap.write_barrier_field(&holder, 0, &y);
    drop(y);
    heap.run_cycle_collection_minor();
    assert!(raw_alive(&weak_y), "the young child of a tenured holder survives on its card");
}

/// A tenure while the cycle is **marking**: a pre-cycle young object reachable only through a
/// still-grey young holder becomes old and white. The snapshot argument (SATB + the grey queue)
/// still reaches it before the sweep — nothing a tenure does depends on the minor's root set.
#[test]
fn a_tenure_while_marking_does_not_lose_what_the_snapshot_reaches() {
    let heap = generational_heap();
    let root = obj(&heap, "Root");
    let holder = obj(&heap, "Holder");
    let x = obj(&heap, "X");
    link(&root, &holder);
    link(&holder, &x);
    let _pin = heap.pin_root(root.clone());
    let weak_x = heap.make_weak(&x).unwrap();
    drop((holder, x));

    assert!(!heap.run_major_slice_for_test(1), "cycle open, the chain not traced yet");
    heap.run_tenure();
    assert_eq!(heap.young_count(), 0);
    assert!(heap.finish_major_cycle_for_test());
    assert!(raw_alive(&weak_x), "the cycle's marking reaches the tenured chain");
    #[cfg(debug_assertions)]
    heap.debug_validate_invariants();
}

/// A tenure while the cycle is **sweeping**: entries without the cycle's epoch are garbage the
/// sweep has not reached. They stay young (promoting them would make unmarked old entries — the
/// `promote_black` debug check), and the sweep reclaims them. Objects born during the cycle carry
/// the epoch and are tenured.
#[test]
fn a_tenure_while_sweeping_leaves_the_doomed_young_for_the_sweep() {
    let heap = generational_heap();
    let keep = obj(&heap, "Keep");
    let _pin = heap.pin_root(keep);
    let garbage: Vec<_> = (0..700).map(|_| heap.make_weak(&obj(&heap, "G")).unwrap()).collect();

    assert!(!heap.run_major_slice_for_test(300));
    assert!(heap.major_cycle_sweeping_for_test());
    let unswept = garbage.iter().filter(|w| raw_alive(w)).count();
    assert!(unswept > 0, "the budget must leave some garbage unswept for this test to mean anything");
    let born = obj(&heap, "Born");
    let weak_born = heap.make_weak(&born).unwrap();
    let _born_pin = heap.pin_root(born.clone());

    heap.run_tenure();
    assert_eq!(heap.young_count(), unswept, "only the doomed stay listed");
    assert!(ArcMagrGC::gen_age_of(&born) >= heap.promotion_age(), "born black, tenured");
    #[cfg(debug_assertions)]
    heap.debug_validate_invariants();

    assert!(heap.finish_major_cycle_for_test());
    assert!(garbage.iter().all(|w| !raw_alive(w)), "the sweep reclaims the doomed it left young");
    assert!(raw_alive(&weak_born));
    assert_eq!(heap.young_count(), 0);
    #[cfg(debug_assertions)]
    heap.debug_validate_invariants();
}

/// Only a minor the **policy** asked for can become a tenure: an explicit collection keeps its
/// promise to reclaim young garbage, and does not use up the policy's turn.
#[test]
fn only_a_policy_minor_is_served_as_a_tenure() {
    use super::super::incremental::GenWork;
    use super::super::young_policy::MinorYield;
    use std::sync::atomic::Ordering::Relaxed;
    let heap = generational_heap();
    heap.young_policy.observe_minor(MinorYield { freed: 0, pause_us: 1000, gate: 1 << 20 });
    assert_eq!(heap.choose_generational_work(false), GenWork::Minor, "explicit: a real minor");
    heap.incremental.pending_minor.store(true, Relaxed);
    assert_eq!(heap.choose_generational_work(false), GenWork::Tenure, "the turn is still there");
    heap.incremental.pending_minor.store(true, Relaxed);
    assert_eq!(heap.choose_generational_work(false), GenWork::Minor, "one turn, then the probe");
}

/// While nothing is dying anywhere — the last major and the last real minor both freed
/// essentially nothing — the old generation gets one more allowance floor before the next major: a
/// fixed amount, not a multiplier, so the gate cannot run away with the live set.
#[test]
fn a_major_waits_one_allowance_floor_longer_while_nothing_dies() {
    use super::super::young_policy::MinorYield;
    let heap = generational_heap();
    let floor = heap.allowance_floor();
    let allowance = 3 * floor;
    let minor = |freed| MinorYield { freed, pause_us: 5_000, gate: 16 << 20 };
    assert_eq!(heap.promoted_gate(allowance), allowance, "no major yet: the plain allowance");
    heap.observe_major_work(0, 20_000, true);
    heap.young_policy.observe_minor(minor(8 << 20));
    assert_eq!(heap.promoted_gate(allowance), allowance, "minors reclaim: garbage is being made");
    heap.young_policy.observe_minor(minor(0));
    assert_eq!(heap.promoted_gate(allowance), allowance + floor, "both futile: one floor more");
    heap.observe_major_work(floor, 20_000, true);
    assert_eq!(heap.promoted_gate(allowance), allowance, "a productive major clears it");
}
