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
