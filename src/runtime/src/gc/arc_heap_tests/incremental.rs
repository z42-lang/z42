//! add-incremental-major-gc M2a: the SATB deletion barrier and the weak / soft read barrier.
//!
//! These drive a major cycle **by hand** — open, snapshot the roots, let a "mutator" act, drain,
//! close, sweep — so the interleaving that loses an object without the barrier is deterministic.
//! Liveness is observed through weak references, never by dereferencing a handle that might have
//! been swept.

use super::*;
use crate::gc::GcMode;
use crate::gc::refs::GcRef;

/// Binds the test thread's SATB records to `heap` for the scope (what `VmContext::new` does).
struct Bound;
impl Bound {
    fn to(heap: &ArcMagrGC) -> Self {
        crate::gc::satb::bind_thread(heap.heap_epoch());
        Bound
    }
}
impl Drop for Bound {
    fn drop(&mut self) {
        crate::gc::satb::set_disabled_for_test(false);
        crate::gc::satb::unbind_thread();
    }
}

fn is_alive(heap: &ArcMagrGC, weak: &crate::gc::WeakRef) -> bool {
    // `upgrade_weak` shades while a mark is in progress; every call here is after the cycle closed.
    heap.upgrade_weak(weak).is_some()
}

/// Root → holder, holder.f0 → x. After the snapshot the mutator takes `x` out of `holder` (into a
/// Rust local — a register as far as the collector is concerned, and one it already scanned) and
/// clears the field. Returns whether `x` survived the cycle.
fn run_moved_into_register(barrier_on: bool) -> bool {
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::StwMarkSweep);
    let _bound = Bound::to(&heap);
    crate::gc::satb::set_disabled_for_test(!barrier_on);

    let holder = heap.alloc_object(dummy_type_desc("Holder"), vec![Value::Null], NativeData::None);
    let x = heap.alloc_object(dummy_type_desc("X"), vec![], NativeData::None);
    let Value::Object(holder_gc) = &holder else { panic!() };
    holder_gc.borrow_mut().refs_mut_raw()[0] = x.clone();
    let _root = heap.pin_root(holder.clone());
    let weak_x = heap.make_weak(&x).expect("object");

    heap.open_major_cycle();
    heap.snapshot_roots_into_mark_queue(); // holder is grey, not yet traced
    // The mutator: `x` stays in a local, the only edge to it is cut.
    let held = x;
    holder_gc.borrow_mut().set_ref_slot(0, &Value::Null);
    heap.drain_mark_queue(); // traces holder — its field is Null now
    heap.close_major_marking();
    heap.sweep_phase();

    let alive = is_alive(&heap, &weak_x);
    std::mem::forget(held); // never dereference a possibly-swept handle
    alive
}

#[test]
fn satb_keeps_an_object_moved_from_an_unscanned_field_into_a_register() {
    assert!(run_moved_into_register(true), "the overwritten reference must be recorded and marked");
}

/// Negative control: without the barrier the very same interleaving loses the object. This is the
/// hole a Dijkstra (insertion) barrier alone leaves open when the roots are not rescanned.
#[test]
fn without_satb_the_same_interleaving_sweeps_a_live_object() {
    assert!(!run_moved_into_register(false), "control: the test must be able to lose the object");
}

/// The byte-inlined reference path (`write_inline_ref` inside `set_field_value`) records too.
#[test]
fn satb_records_through_set_field_value() {
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::StwMarkSweep);
    let _bound = Bound::to(&heap);

    let holder = heap.alloc_object(dummy_type_desc("Holder"), vec![Value::Null], NativeData::None);
    let x = heap.alloc_object(dummy_type_desc("X"), vec![], NativeData::None);
    let Value::Object(holder_gc) = &holder else { panic!() };
    holder_gc.borrow_mut().set_field_value(0, &x);
    let _root = heap.pin_root(holder.clone());
    let weak_x = heap.make_weak(&x).expect("object");

    heap.open_major_cycle();
    heap.snapshot_roots_into_mark_queue();
    let held = x;
    holder_gc.borrow_mut().set_field_value(0, &Value::Null);
    heap.drain_mark_queue();
    heap.close_major_marking();
    heap.sweep_phase();

    assert!(is_alive(&heap, &weak_x));
    std::mem::forget(held);
}

/// Array elements go through `ArrayObj::set_boxed`.
#[test]
fn satb_records_an_overwritten_array_element() {
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::StwMarkSweep);
    let _bound = Bound::to(&heap);

    let x = heap.alloc_object(dummy_type_desc("X"), vec![], NativeData::None);
    let arr = heap.alloc_array(vec![x.clone()]);
    let _root = heap.pin_root(arr.clone());
    let weak_x = heap.make_weak(&x).expect("object");

    heap.open_major_cycle();
    heap.snapshot_roots_into_mark_queue();
    let held = x;
    let Value::Array(arr_gc) = &arr else { panic!() };
    arr_gc.borrow_mut().set_boxed(0, Value::Null);
    heap.drain_mark_queue();
    heap.close_major_marking();
    heap.sweep_phase();

    assert!(is_alive(&heap, &weak_x));
    std::mem::forget(held);
}

/// Outside a mark the barrier records nothing — the fast path, and no stray queue entries.
#[test]
fn satb_records_nothing_outside_a_mark() {
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::StwMarkSweep);
    let _bound = Bound::to(&heap);

    let holder = heap.alloc_object(dummy_type_desc("Holder"), vec![Value::Null], NativeData::None);
    let x = heap.alloc_object(dummy_type_desc("X"), vec![], NativeData::None);
    let Value::Object(holder_gc) = &holder else { panic!() };
    holder_gc.borrow_mut().set_ref_slot(0, &x);
    holder_gc.borrow_mut().set_ref_slot(0, &Value::Null);
    heap.retire_thread_tlab();
    assert!(heap.satb_queue.lock().is_empty());
}

/// A record belongs to the heap the writing thread is bound to: another heap marking at the same
/// time must not receive it (it would grey a foreign object with its own epoch).
#[test]
fn satb_records_stay_with_the_writing_threads_heap() {
    let mine = ArcMagrGC::new();
    let other = ArcMagrGC::new();
    let _bound = Bound::to(&mine);

    let holder = mine.alloc_object(dummy_type_desc("Holder"), vec![Value::Null], NativeData::None);
    let x = mine.alloc_object(dummy_type_desc("X"), vec![], NativeData::None);
    let Value::Object(holder_gc) = &holder else { panic!() };
    holder_gc.borrow_mut().set_ref_slot(0, &x);

    other.open_major_cycle(); // only the other heap is marking
    holder_gc.borrow_mut().set_ref_slot(0, &Value::Null);
    other.retire_thread_tlab();
    mine.retire_thread_tlab();
    assert!(other.satb_queue.lock().is_empty(), "the other heap must not receive this thread's record");
    assert!(mine.satb_queue.lock().is_empty(), "this heap is not marking — nothing to record");
    other.close_major_marking();
}

/// A weakly-reachable object handed back to a register during a mark must survive the cycle.
fn run_weak_read(read_during_mark: bool) -> bool {
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::StwMarkSweep);
    let x = heap.alloc_object(dummy_type_desc("X"), vec![], NativeData::None);
    let weak_x = heap.make_weak(&x).expect("object");
    std::mem::forget(x); // only the weak reference remains

    heap.open_major_cycle();
    heap.snapshot_roots_into_mark_queue();
    let revived = if read_during_mark { heap.upgrade_weak(&weak_x) } else { None };
    heap.drain_mark_queue();
    heap.close_major_marking();
    heap.sweep_phase();

    let alive = is_alive(&heap, &weak_x);
    std::mem::forget(revived);
    alive
}

#[test]
fn a_weak_read_during_a_mark_keeps_the_object() {
    assert!(run_weak_read(true));
}

#[test]
fn without_the_weak_read_the_object_is_collected() {
    assert!(!run_weak_read(false), "control: nothing else keeps it alive");
}

/// A minor that runs while a major mark is outstanding treats the SATB records as roots — a young
/// object the barrier recorded must not be swept out from under the marker.
fn run_minor_with_record(recorded: bool) -> bool {
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::GenerationalMarkSweep);
    let young = heap.alloc_object(dummy_type_desc("Young"), vec![], NativeData::None);
    let weak = heap.make_weak(&young).expect("object");
    if recorded {
        heap.satb_queue.lock().push(young.clone());
    }
    std::mem::forget(young);

    heap.run_cycle_collection_minor();
    let alive = is_alive(&heap, &weak);
    heap.satb_queue.lock().clear();
    alive
}

#[test]
fn a_minor_keeps_what_the_satb_barrier_recorded() {
    assert!(run_minor_with_record(true));
}

#[test]
fn a_minor_collects_the_same_object_when_nothing_recorded_it() {
    assert!(!run_minor_with_record(false), "control: the record is what keeps it");
}

/// The recorded value is what the cycle marks: it must carry the cycle's epoch afterwards.
#[test]
fn close_major_marking_marks_recorded_values_with_the_cycle_epoch() {
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::StwMarkSweep);
    let x = heap.alloc_object(dummy_type_desc("X"), vec![], NativeData::None);
    let kind = heap.open_major_cycle();
    heap.satb_queue.lock().push(x.clone());
    heap.close_major_marking();
    let Value::Object(gc) = &x else { panic!() };
    assert!(GcRef::is_marked(gc, kind));
    assert!(crate::gc::satb::heap_is_marking(heap.heap_epoch()).is_none(), "recording stopped");
}

/// `ArrayObj::copy_elems_from` (the `Array.Copy` bulk path) overwrites references in one clone —
/// it must record them like `set_boxed` does. Missed by the first barrier audit; found by the
/// `Z42_GC_SLICE_MS=0.05` stress run of the incremental major as a SIGSEGV in the compiler.
fn run_bulk_copy_over_live_element(barrier_on: bool) -> bool {
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::StwMarkSweep);
    let _bound = Bound::to(&heap);
    crate::gc::satb::set_disabled_for_test(!barrier_on);

    let x = heap.alloc_object(dummy_type_desc("X"), vec![], NativeData::None);
    let dst = heap.alloc_array(vec![x.clone(), Value::Null]);
    let src = heap.alloc_array(vec![Value::Null, Value::Null]);
    let _root = heap.pin_root(dst.clone());
    let _src_root = heap.pin_root(src.clone());
    let weak_x = heap.make_weak(&x).expect("object");

    heap.open_major_cycle();
    heap.snapshot_roots_into_mark_queue();
    let held = x;
    let (Value::Array(dst_gc), Value::Array(src_gc)) = (&dst, &src) else { panic!() };
    dst_gc.borrow_mut().copy_elems_from(&src_gc.borrow(), 0, 0, 2);
    heap.drain_mark_queue();
    heap.close_major_marking();
    heap.sweep_phase();

    let alive = is_alive(&heap, &weak_x);
    std::mem::forget(held);
    alive
}

#[test]
fn satb_records_references_overwritten_by_a_bulk_array_copy() {
    assert!(run_bulk_copy_over_live_element(true));
}

#[test]
fn without_satb_a_bulk_array_copy_loses_the_overwritten_reference() {
    assert!(!run_bulk_copy_over_live_element(false), "control");
}

// ── add-incremental-major-gc M2b: the slice scheduler ─────────────────────────────────────────────
//
// Slices are driven by hand with a **work-unit** budget (`run_major_slice_for_test(units)`: one unit
// per traced value, `CHUNK_SIZE` per swept chunk), so every interleaving below is deterministic.
// `u64::MAX` units = run the cycle to completion in one slice.

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

fn raw_alive(weak: &crate::gc::WeakRef) -> bool {
    match &weak.inner {
        crate::gc::types::WeakRefInner::Object(w) => w.upgrade().is_some(),
        crate::gc::types::WeakRefInner::Array(w) => w.upgrade().is_some(),
    }
}

#[test]
fn an_incremental_cycle_spans_several_slices_and_collects_garbage() {
    let heap = generational_heap();
    // A rooted chain of 50 and 600 unreachable objects.
    let head = obj(&heap, "Node");
    let _root = heap.pin_root(head.clone());
    let mut tail = head.clone();
    let mut chain = Vec::new();
    for _ in 0..50 {
        let next = obj(&heap, "Node");
        let Value::Object(t) = &tail else { panic!() };
        t.borrow_mut().set_ref_slot(0, &next);
        chain.push(heap.make_weak(&next).unwrap());
        tail = next;
    }
    let garbage: Vec<_> = (0..600).map(|_| heap.make_weak(&obj(&heap, "G")).unwrap()).collect();

    let mut slices = 1;
    while !heap.run_major_slice_for_test(16) {
        slices += 1;
        assert!(heap.major_cycle_active());
    }
    assert!(slices > 4, "a 16-unit budget must split this cycle (took {slices} slices)");
    assert!(!heap.major_cycle_active());
    assert!(chain.iter().all(|w| heap.upgrade_weak(w).is_some()), "the rooted chain survives");
    assert!(garbage.iter().all(|w| !raw_alive(w)), "everything unreachable is reclaimed");
    heap.debug_validate_invariants();
}

/// Root → holder → x. The first slice traces only the root (holder is grey, unscanned); between
/// slices the mutator moves `x` out of holder into a register and clears the field.
fn run_satb_across_slices(barrier_on: bool) -> bool {
    let heap = generational_heap();
    let _bound = Bound::to(&heap);
    crate::gc::satb::set_disabled_for_test(!barrier_on);
    let root = obj(&heap, "Root");
    let holder = obj(&heap, "Holder");
    let x = obj(&heap, "X");
    let (Value::Object(root_gc), Value::Object(holder_gc)) = (&root, &holder) else { panic!() };
    root_gc.borrow_mut().set_ref_slot(0, &holder);
    holder_gc.borrow_mut().set_ref_slot(0, &x);
    let _pin = heap.pin_root(root.clone());
    let weak_x = heap.make_weak(&x).unwrap();

    assert!(!heap.run_major_slice_for_test(1), "one unit: root traced, holder still grey");
    let held = x;
    holder_gc.borrow_mut().set_ref_slot(0, &Value::Null);
    drop(holder);
    while !heap.run_major_slice_for_test(1) {}
    let alive = raw_alive(&weak_x);
    std::mem::forget(held);
    alive
}

#[test]
fn satb_keeps_an_object_moved_into_a_register_between_real_slices() {
    assert!(run_satb_across_slices(true));
}

#[test]
fn without_satb_the_same_slice_interleaving_loses_the_object() {
    assert!(!run_satb_across_slices(false), "control: the test must be able to lose it");
}

#[test]
fn objects_born_while_marking_or_sweeping_survive_their_cycle() {
    let heap = generational_heap();
    let keep = obj(&heap, "Keep");
    let _pin = heap.pin_root(keep);
    for _ in 0..700 {
        obj(&heap, "G"); // enough garbage for a sweep that spans slices
    }
    assert!(!heap.run_major_slice_for_test(1));
    assert!(heap.major_cycle_marking_for_test());
    let born_marking = heap.make_weak(&obj(&heap, "BornMarking")).unwrap();

    assert!(!heap.run_major_slice_for_test(300), "marking completes, the sweep does not");
    assert!(heap.major_cycle_sweeping_for_test());
    let born_sweeping = heap.make_weak(&obj(&heap, "BornSweeping")).unwrap();

    while !heap.run_major_slice_for_test(300) {}
    assert!(raw_alive(&born_marking) && raw_alive(&born_sweeping), "allocate-black covers the whole cycle");

    // One cycle of retention, not forever: the next cycle sees them white.
    assert!(heap.run_major_slice_for_test(WHOLE));
    assert!(!raw_alive(&born_marking) && !raw_alive(&born_sweeping));
}

#[test]
fn a_doomed_object_is_not_handed_out_while_the_sweep_has_not_reached_it() {
    let heap = generational_heap();
    let keep = obj(&heap, "Keep");
    let _pin = heap.pin_root(keep);
    let garbage: Vec<_> = (0..700).map(|_| heap.make_weak(&obj(&heap, "G")).unwrap()).collect();

    assert!(!heap.run_major_slice_for_test(300));
    assert!(heap.major_cycle_sweeping_for_test());
    let unswept = garbage.iter().filter(|w| raw_alive(w)).count();
    assert!(unswept > 0, "the budget must leave some garbage unswept for this test to mean anything");
    assert!(garbage.iter().all(|w| heap.upgrade_weak(w).is_none()), "weak reads refuse doomed objects");
    let mut listed = 0;
    heap.iterate_live_objects(&mut |_| listed += 1);
    assert_eq!(listed, 1, "heap iteration lists only the live object");

    assert!(heap.finish_major_cycle_for_test());
    assert!(garbage.iter().all(|w| !raw_alive(w)));
}

/// Root → holder → young `y`. Between slices the mutator cuts the edge (the barrier records `y`)
/// and a minor runs before the marker gets to it.
fn run_minor_between_slices(barrier_on: bool) -> bool {
    let heap = generational_heap();
    let _bound = Bound::to(&heap);
    crate::gc::satb::set_disabled_for_test(!barrier_on);
    let root = obj(&heap, "Root");
    let holder = obj(&heap, "Holder");
    let y = obj(&heap, "Young");
    let (Value::Object(root_gc), Value::Object(holder_gc)) = (&root, &holder) else { panic!() };
    root_gc.borrow_mut().set_ref_slot(0, &holder);
    holder_gc.borrow_mut().set_ref_slot(0, &y);
    let _pin = heap.pin_root(root.clone());
    let weak_y = heap.make_weak(&y).unwrap();

    assert!(!heap.run_major_slice_for_test(1));
    let held = y;
    holder_gc.borrow_mut().set_ref_slot(0, &Value::Null);
    heap.run_cycle_collection_minor();
    let after_minor = raw_alive(&weak_y);
    while !heap.run_major_slice_for_test(1) {}
    let alive = after_minor && raw_alive(&weak_y);
    std::mem::forget(held);
    alive
}

#[test]
fn a_minor_between_slices_keeps_what_the_barrier_recorded() {
    assert!(run_minor_between_slices(true));
}

#[test]
fn without_satb_a_minor_between_slices_reclaims_it() {
    assert!(!run_minor_between_slices(false), "control");
}

/// Regression for `Region::sweep_chunks(delist_young = true)`: the incremental sweep has no aging
/// pass after it, so a dead entry left in `young_list` gets listed a second time once its slot is
/// reused — and the next minor keeps the new object on the first visit, then reclaims it on the
/// second.
#[test]
fn slots_freed_by_an_incremental_sweep_are_safe_to_reuse_before_the_next_minor() {
    let heap = generational_heap();
    for _ in 0..300 {
        obj(&heap, "YoungGarbage");
    }
    assert!(heap.run_major_slice_for_test(WHOLE));
    let reused: Vec<Value> = (0..300).map(|_| obj(&heap, "Reused")).collect();
    let pins: Vec<_> = reused.iter().map(|v| heap.pin_root(v.clone())).collect();
    let weaks: Vec<_> = reused.iter().map(|v| heap.make_weak(v).unwrap()).collect();
    heap.debug_validate_invariants();
    heap.run_cycle_collection_minor();
    assert!(weaks.iter().all(raw_alive), "a pinned object in a reused slot must survive the minor");
    heap.debug_validate_invariants();
    drop(pins);
}

#[test]
fn an_explicit_collection_finishes_the_open_cycle() {
    let heap = generational_heap();
    let garbage: Vec<_> = (0..300).map(|_| heap.make_weak(&obj(&heap, "G")).unwrap()).collect();
    assert!(!heap.run_major_slice_for_test(1));
    assert!(heap.finish_major_cycle_for_test());
    assert!(!heap.major_cycle_active());
    assert!(garbage.iter().all(|w| !raw_alive(w)));
}

#[test]
fn the_pause_does_what_the_policy_asked_for() {
    use super::super::incremental::GenWork;
    use std::sync::atomic::Ordering::Relaxed;
    let heap = generational_heap();
    let s = &heap.incremental;
    // No cycle open.
    assert_eq!(heap.choose_generational_work(false), GenWork::Minor);
    assert_eq!(heap.choose_generational_work(true), GenWork::Slice, "a major trip opens a cycle");
    s.pending_minor.store(true, Relaxed);
    assert_eq!(heap.choose_generational_work(true), GenWork::Minor,
        "opening a cycle never displaces a requested minor (minors do the aging)");
    assert_eq!(heap.choose_generational_work(false), GenWork::Slice, "the deferred cycle opens next");
    s.pending_slice.store(true, Relaxed);
    assert_eq!(heap.choose_generational_work(false), GenWork::Minor,
        "a slice request with no cycle open is stale and opens nothing");
    // A cycle open.
    obj(&heap, "G");
    assert!(!heap.run_major_slice_for_test(1));
    s.pending_minor.store(true, Relaxed);
    assert_eq!(heap.choose_generational_work(false), GenWork::Minor);
    s.pending_slice.store(true, Relaxed);
    assert_eq!(heap.choose_generational_work(false), GenWork::Slice);
    assert_eq!(heap.choose_generational_work(true), GenWork::Slice, "escalation while open = keep slicing");
    assert_eq!(heap.choose_generational_work(false), GenWork::Finish, "nothing asked for = GC.Collect()");
    s.pending_minor.store(true, Relaxed);
    s.pending_finish.store(true, Relaxed);
    assert_eq!(heap.choose_generational_work(false), GenWork::Finish, "soft cap beats everything");
    heap.finish_major_cycle_for_test();
}

/// Random writes, register moves, minors and tiny slices, interleaved for several cycles. Every
/// object reachable from the roots at the end must still be dereferenceable (a dangling handle
/// panics in `GcRef::entry_ref` under debug assertions), and the heap invariants must hold.
#[test]
fn incremental_cycles_interleaved_with_writes_and_minors_keep_the_graph_intact() {
    const SLOTS: usize = 48;
    let heap = generational_heap();
    let _bound = Bound::to(&heap);
    let roots = heap.alloc_array(vec![Value::Null; SLOTS]);
    let _roots_pin = heap.pin_root(roots.clone());
    let Value::Array(roots_gc) = &roots else { panic!() };
    let mut registers: Vec<(Value, crate::gc::RootHandle)> = Vec::new();
    let mut rng = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = || { rng ^= rng << 13; rng ^= rng >> 7; rng ^= rng << 17; rng };
    let (mut cycles, mut minors) = (0, 0);

    for _ in 0..6000 {
        let r = next();
        let slot = (r >> 8) as usize % SLOTS;
        let pick = roots_gc.borrow().get_boxed((r >> 20) as usize % SLOTS);
        match r % 16 {
            0..=5 => {
                let o = heap.alloc_object(dummy_type_desc("N"), vec![pick.clone(), Value::Null], NativeData::None);
                roots_gc.borrow_mut().set_boxed(slot, o.clone());
                heap.write_barrier_array_elem(&roots, slot, &o);
            }
            6..=8 => {
                if let Value::Object(target) = roots_gc.borrow().get_boxed(slot) {
                    let field = (r >> 32) as usize % 2;
                    target.borrow_mut().set_field_value(field, &pick);
                    if pick.is_heap_ref() {
                        heap.write_barrier_field(&Value::Object(target.clone()), field, &pick);
                    }
                }
            }
            9 => {
                // Load a field into a register and clear the field (the SATB case).
                if let Value::Object(target) = roots_gc.borrow().get_boxed(slot) {
                    let v = target.borrow().refs()[0].clone();
                    if v.is_heap_ref() {
                        registers.push((v.clone(), heap.pin_root(v)));
                    }
                    target.borrow_mut().set_field_value(0, &Value::Null);
                }
            }
            10 => {
                if !registers.is_empty() {
                    let (_, h) = registers.swap_remove((r >> 40) as usize % registers.len());
                    heap.unpin_root(h);
                }
            }
            11..=13 => {
                if heap.run_major_slice_for_test(1 + (r >> 48) % 40) {
                    cycles += 1;
                }
            }
            _ => {
                heap.run_cycle_collection_minor();
                minors += 1;
            }
        }
    }
    heap.finish_major_cycle_for_test();
    heap.debug_validate_invariants();
    assert!(cycles >= 3 && minors >= 50, "the interleaving must actually happen ({cycles} cycles, {minors} minors)");

    let mut stack: Vec<Value> = roots_gc.borrow().iter_boxed().collect();
    stack.extend(registers.iter().map(|(v, _)| v.clone()));
    let mut seen = std::collections::HashSet::new();
    while let Some(v) = stack.pop() {
        let Value::Object(gc) = &v else { continue };
        if !seen.insert(gc.entry_ptr().as_ptr() as usize) {
            continue;
        }
        v.visit_gc_children(None, &mut |c| stack.push(c.clone()));
    }
    assert!(!seen.is_empty());
    // One more whole cycle over the surviving graph, validated again.
    assert!(heap.run_major_slice_for_test(WHOLE));
    heap.debug_validate_invariants();
}

/// The doomed-entry hazard in a minor's card seeding. An old object `d` in a dirty card points at a
/// young `y`; both become garbage. The incremental sweep reaches `y`'s chunk first and reclaims it,
/// the slot is reused, and a minor runs before the sweep reaches `d`. Tracing `d` through its card
/// would follow a dangling edge into the new object.
#[test]
fn a_minor_during_the_sweep_does_not_trace_through_a_doomed_card_entry() {
    let heap = generational_heap();
    // Chunk 0: garbage fillers plus a few survivors (so the chunk is not pooled and its free
    // slots are reused in place).
    let fillers: Vec<Value> = (0..250).map(|_| obj(&heap, "Filler")).collect();
    let keepers: Vec<_> = (0..6).map(|_| heap.pin_root(obj(&heap, "Keeper"))).collect();
    drop(fillers);
    // Chunk 1: `d`, made old by three minors.
    let d = obj(&heap, "Old");
    let d_pin = heap.pin_root(d.clone());
    for _ in 0..3 {
        heap.run_cycle_collection_minor();
    }
    assert!(ArcMagrGC::gen_age_of(&d) >= heap.promotion_age(), "d must be old");
    // `y` reuses a filler slot in chunk 0; `d.f0 = y` dirties d's card.
    let y = obj(&heap, "Young");
    let Value::Object(d_gc) = &d else { panic!() };
    d_gc.borrow_mut().set_field_value(0, &y);
    heap.write_barrier_field(&d, 0, &y);
    drop(y);
    heap.unpin_root(d_pin);
    std::mem::forget(d);

    // Mark the 6 keepers, then sweep exactly chunk 0 (y reclaimed; d doomed, unswept).
    assert!(!heap.run_major_slice_for_test(200));
    assert!(heap.major_cycle_sweeping_for_test());
    // Refill chunk 0's free slots — one of them is y's.
    let refill: Vec<_> = (0..256).map(|_| heap.pin_root(obj(&heap, "Refill"))).collect();
    heap.run_cycle_collection_minor();
    assert!(heap.finish_major_cycle_for_test());
    heap.debug_validate_invariants();
    drop((keepers, refill));
}

/// A minor that runs inside an open cycle must not reclaim what the marker has already marked.
///
/// The marker commits to everything reachable at its snapshot, and the mutator may hold a value
/// it read out of the heap before the edge leading to it was cut (that is what the SATB barrier
/// is for). A minor's own reachability view is narrower — it sees roots, dirty cards and the two
/// queues — so without this rule it frees objects the cycle still owns, and the mutator is left
/// with a handle into a recycled slot. Found by the `Z42_GC_SLICE_MS=0.05` stress run of
/// `z42.net`: with the rule off the suite wedges after ~6 tests, with it on all 50 files pass.
fn run_minor_inside_cycle(mark_it: bool) -> bool {
    let heap = generational_heap();
    let _bound = Bound::to(&heap);
    let root = obj(&heap, "Root");
    let _pin = heap.pin_root(root.clone());

    // A young object the cycle marks, reachable from nothing a minor would find.
    let x = obj(&heap, "X");
    let weak_x = heap.make_weak(&x).expect("object");
    assert!(!heap.run_major_slice_for_test(1), "cycle open");
    if mark_it {
        assert!(heap.mark_if_unmarked_for_test(&x), "the marker takes it for this cycle");
    }
    let held = x;
    heap.run_cycle_collection_minor();

    let alive = raw_alive(&weak_x);
    std::mem::forget(held);
    heap.finish_major_cycle_for_test();
    alive
}

#[test]
fn a_minor_inside_a_cycle_keeps_what_the_marker_already_marked() {
    assert!(run_minor_inside_cycle(true));
}

#[test]
fn a_minor_inside_a_cycle_still_reclaims_what_the_marker_never_marked() {
    assert!(!run_minor_inside_cycle(false), "control: the rule must not keep everything alive");
}
