//! Object model R3: the SATB deletion barrier on every array mode that stores references — REF
//! words, MIXED cells, `struct[]` leaves, and the bulk copy. Each case is a pair: with the barrier
//! the referent survives a cut made during marking, without it the referent is swept (the
//! interleaving of `incremental.rs`: open, snapshot roots, mutate, drain, close, sweep).

use super::*;
use crate::gc::GcMode;
use crate::metadata::types::{ArrayObj, ElemKind, ElemType, StructTypeLayout, STRUCT_REF_GCREF};

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

#[derive(Clone, Copy)]
enum Cut { RefElem, MixedElem, StructLeaf, BulkCopy }

/// Root → array, array[0] → x. After the root snapshot the mutator moves `x` into a register and
/// overwrites the array's only edge to it. Returns whether `x` survived the cycle.
fn run(cut: Cut, barrier_on: bool) -> bool {
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::StwMarkSweep);
    let _bound = Bound::to(&heap);
    let _g = crate::gc::ambient::HeapGuard::enter(&heap);
    crate::gc::satb::set_disabled_for_test(!barrier_on);

    let x = heap.alloc_object(dummy_type_desc("X"), vec![], NativeData::None);
    let layout = std::sync::Arc::new(StructTypeLayout {
        size: 16, ref_offsets: Box::new([8]), ref_kinds: Box::new([STRUCT_REF_GCREF]), fields: Box::new([]),
    });
    let a = match cut {
        Cut::RefElem | Cut::BulkCopy =>
            ArrayObj::from_iter_kind(&heap, ElemType::intern("object"), ElemKind::Ref, 1, [x].into_iter()),
        Cut::MixedElem => ArrayObj::new(&heap, vec![x]),
        Cut::StructLeaf => {
            let mut a = ArrayObj::struct_backed(&heap, ElemType::intern("P"), 1, layout);
            a.write_struct_elem(0, &[0u8; 16], &[x], &mut |_| {}).unwrap();
            a
        }
    };
    let arr = heap.alloc_array_obj(a);
    let Value::Array(arr_gc) = arr else { panic!() };
    let empty = heap.alloc_array_obj(ArrayObj::filled(&heap, ElemType::intern("object"), ElemKind::Ref, 1, Value::Null));
    let Value::Array(empty_gc) = empty else { panic!() };
    let _root = heap.pin_root(arr);
    let _root2 = heap.pin_root(empty);
    let weak_x = heap.make_weak(&x).expect("object");

    heap.open_major_cycle();
    heap.snapshot_roots_into_mark_queue();
    let held = x;
    match cut {
        Cut::RefElem | Cut::MixedElem => { arr_gc.borrow_mut().set_boxed(0, Value::Null); }
        Cut::StructLeaf => { arr_gc.borrow_mut().set_struct_elem_ref(0, 8, &Value::Null).unwrap(); }
        Cut::BulkCopy => arr_gc.borrow_mut().copy_elems_from(&empty_gc.borrow(), 0, 0, 1),
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
