//! Object model R3: the SATB deletion barrier on every array mode that stores references — REF
//! words, MIXED cells, `struct[]` leaves, and the bulk copy. Each case is a pair: with the barrier
//! the referent survives a cut made during marking, without it the referent is swept (the
//! interleaving of `incremental.rs`: open, snapshot roots, mutate, drain, close, sweep).

use super::*;
use crate::gc::GcMode;
use crate::object::{ArrayObj, ElemKind, ElemType, StructTypeLayout, STRUCT_REF_GCREF};

struct Bound;
impl Bound {
    fn to(heap: &Heap) -> Self {
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

#[derive(Clone, Copy)]
enum Cut { RefElem, MixedElem, StructLeaf, BulkCopy, ClearRef(usize), ClearStruct(usize, usize) }

/// Root → array, one array element → x. After the root snapshot the mutator moves `x` into a register and
/// overwrites the array's only edge to it. Returns whether `x` survived the cycle.
fn run(cut: Cut, barrier_on: bool) -> bool {
    let heap = Heap::default();
    heap.set_mode(GcMode::StwMarkSweep);
    let _bound = Bound::to(&heap);
    let _g = crate::vm::exec::ambient::HeapGuard::enter(&heap);
    crate::gc::satb::set_disabled_for_test(!barrier_on);

    let x = heap.alloc_object(dummy_type_desc("X"), vec![], NativeData::None);
    let (size, offsets) = if matches!(cut, Cut::ClearStruct(_, _)) { (24, vec![16, 8]) } else { (16, vec![8]) };
    let layout = std::sync::Arc::new(StructTypeLayout {
        size, ref_kinds: vec![STRUCT_REF_GCREF; offsets.len()].into_boxed_slice(),
        ref_offsets: offsets.into_boxed_slice(), fields: Box::new([]),
    });
    let length = if matches!(cut, Cut::ClearRef(_) | Cut::ClearStruct(_, _)) { 9 } else { 1 };
    let a = match cut {
        Cut::RefElem | Cut::BulkCopy =>
            ArrayObj::alloc_from_iter(&heap, ElemType::intern("object"), ElemKind::Ref, 1, [x].into_iter()),
        Cut::ClearRef(slot) => {
            let array = ArrayObj::alloc_filled(&heap, ElemType::intern("object"), ElemKind::Ref, length, Value::Null);
            let Value::Array(value) = array else { panic!("expected array") };
            value.borrow().set_boxed(slot, x);
            array
        },
        Cut::MixedElem => ArrayObj::alloc_untyped(&heap, vec![x]),
        Cut::ClearStruct(element, leaf) => {
            let a = ArrayObj::alloc_struct(&heap, ElemType::intern("P"), length, layout);
            let Value::Array(g) = a else { panic!() };
            let mut refs = [Value::Null; 2]; refs[leaf] = x;
            g.borrow().write_struct_elem(element, &[0u8; 24], &refs, &mut |_| {}).unwrap();
            a
        }
        Cut::StructLeaf => {
            let a = ArrayObj::alloc_struct(&heap, ElemType::intern("P"), 1, layout);
            let Value::Array(g) = &a else { panic!() };
            g.borrow().write_struct_elem(0, &[0u8; 16], &[x], &mut |_| {}).unwrap();
            a
        }
    };
    let arr = a;
    let Value::Array(arr_gc) = arr else { panic!() };
    let empty = ArrayObj::alloc_filled(&heap, ElemType::intern("object"), ElemKind::Ref, 1, Value::Null);
    let Value::Array(empty_gc) = empty else { panic!() };
    let _root = heap.pin_root(arr);
    let _root2 = heap.pin_root(empty);
    let weak_x = heap.make_weak(&x).expect("object");

    heap.open_major_cycle();
    heap.snapshot_roots_into_mark_queue();
    let held = x;
    match cut {
        Cut::RefElem | Cut::MixedElem => { arr_gc.borrow().set_boxed(0, Value::Null); }
        Cut::StructLeaf => { arr_gc.borrow().set_struct_elem_ref(0, 8, &Value::Null).unwrap(); }
        Cut::BulkCopy => arr_gc.borrow().copy_elems_from(&empty_gc.borrow(), 0, 0, 1),
        Cut::ClearRef(_) | Cut::ClearStruct(_, _) => assert!(arr_gc.borrow().try_clear_elems(0, length).unwrap()),
    }
    heap.drain_mark_queue();
    heap.close_major_marking();
    heap.sweep_phase();

    let alive = heap.upgrade_weak(&weak_x).is_some();
    std::mem::forget(held);
    alive
}

#[test]
fn satb_keeps_a_referent_cut_from_any_array_mode() {
    for cut in [Cut::RefElem, Cut::MixedElem, Cut::StructLeaf, Cut::BulkCopy] {
        assert!(run(cut, true), "barrier on: the cut referent must survive");
    }
}

#[test]
fn without_satb_a_referent_cut_from_any_array_mode_is_swept() {
    for cut in [Cut::RefElem, Cut::MixedElem, Cut::StructLeaf, Cut::BulkCopy] {
        assert!(!run(cut, false), "barrier off: the negative control must lose the referent");
    }
}

#[test]
fn bulk_clear_records_removed_references_and_struct_leaves() {
    for cut in (0..9).flat_map(|element| (0..2).map(move |leaf| Cut::ClearStruct(element, leaf)))
        .chain((0..9).map(Cut::ClearRef)) {
        assert!(run(cut, true), "barrier on: the cut referent must survive");
        assert!(!run(cut, false), "barrier off: the negative control must lose the referent");
    }
}

#[test]
fn bulk_clear_leaves_unsupported_array_modes_untouched() {
    let heap = Heap::default();
    let value = Value::I64(12);
    for array in [ArrayObj::alloc_untyped(&heap, vec![value]), ArrayObj::alloc_frozen(&heap, &[value])] {
        let Value::Array(array) = array else { panic!("expected array") };
        let array = array.borrow();
        assert!(!array.try_clear_elems(0, 1).unwrap());
        assert!(matches!(array.get_boxed(0), Value::I64(12)));
    }
}

// ── Object model R9: an array is one block ─────────────────────────────────────

fn stw_heap() -> Heap {
    let heap = Heap::default();
    heap.set_mode(GcMode::StwMarkSweep);
    heap
}

/// A dead value-struct array releases its element layout (the counted word ahead of its
/// elements) through the drop glue — and a refused one (strict OOM) releases it too.
#[test]
fn a_dead_struct_array_releases_its_layout() {
    let heap = stw_heap();
    let layout = std::sync::Arc::new(StructTypeLayout {
        size: 16, ref_offsets: Box::new([8]), ref_kinds: Box::new([STRUCT_REF_GCREF]), fields: Box::new([]),
    });
    let a = ArrayObj::alloc_struct(&heap, ElemType::intern("P"), 3, layout.clone());
    assert_eq!(std::sync::Arc::strong_count(&layout), 2, "the array holds one count");
    let pin = heap.pin_root(a);
    heap.force_collect();
    assert_eq!(std::sync::Arc::strong_count(&layout), 2, "a live array keeps it");
    heap.unpin_root(pin);
    heap.force_collect();
    assert_eq!(std::sync::Arc::strong_count(&layout), 1, "the drop glue released it");

    heap.set_max_heap_bytes(Some(1));
    heap.set_strict_oom(true);
    assert!(matches!(ArrayObj::alloc_struct(&heap, ElemType::intern("P"), 3, layout.clone()), Value::Null));
    assert_eq!(std::sync::Arc::strong_count(&layout), 1, "a refused allocation releases it at once");
}

/// A weak reference to an array records its chunk's reuse epoch (like an object's): once the
/// array died and its slot / chunk was reused, it never resurrects.
#[test]
fn a_weak_reference_to_a_dead_array_reads_none_after_its_chunk_is_reused() {
    let heap = stw_heap();
    let dead = heap.alloc_array(vec![Value::Null; 4]);
    let weak = heap.make_weak(&dead).expect("array");
    assert!(heap.upgrade_weak(&weak).is_some());
    heap.force_collect();
    assert!(heap.upgrade_weak(&weak).is_none());
    let keep: Vec<_> = (0..4000).map(|_| heap.pin_root(heap.alloc_array(vec![Value::Null; 4]))).collect();
    assert!(heap.upgrade_weak(&weak).is_none(), "never resurrected by a later occupant");
    let _ = keep;
}

/// A finalizer registered on an array fires when the array dies (side table, keyed by the block).
#[test]
fn an_array_finalizer_fires_when_it_dies() {
    let heap = stw_heap();
    let fired = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let a = heap.alloc_array(vec![Value::I64(1)]);
    let f = fired.clone();
    heap.register_finalizer(&a, std::sync::Arc::new(move || { f.fetch_add(1, std::sync::atomic::Ordering::SeqCst); }));
    let pin = heap.pin_root(a);
    heap.force_collect();
    assert_eq!(fired.load(std::sync::atomic::Ordering::SeqCst), 0);
    heap.unpin_root(pin);
    heap.force_collect();
    assert_eq!(fired.load(std::sync::atomic::Ordering::SeqCst), 1);
}

/// A large array gets a dedicated chunk (past the block-size threshold), is fully usable, and its
/// chunk goes back to the allocator when it dies.
#[test]
fn a_large_array_lives_in_its_own_chunk() {
    let heap = stw_heap();
    let n = crate::gc::var_region::loh_bytes() / 8 + 1;
    let a = ArrayObj::alloc_typed_filled(&heap, ElemType::intern("long"), n, Value::I64(0));
    let Value::Array(r) = a else { panic!() };
    let addr = r.var_ref().addr();
    r.borrow().set_boxed(n - 1, Value::I64(42));
    assert_eq!(r.borrow().get_boxed(n - 1), Value::I64(42));
    assert_eq!(r.borrow().len(), n);
    assert!(heap.region_var_for_test().lock().owns_addr(addr));
    heap.force_collect();
    assert!(!heap.region_var_for_test().lock().owns_addr(addr), "the dedicated chunk was freed");
}
