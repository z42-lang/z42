//! fix-missing-write-barriers：`verify_card_invariant`——分代不变量（老对象 / 老数组持有年轻引用 ⇒
//! 它在记忆集里）的从头核对。它是 `Z42_GC_VERIFY_CARDS` 的实体，也是 VM 层写屏障回归测试的判据
//! （`objops/write_barrier_tests.rs`），所以这里钉住它两个方向都判得对：漏屏障必报、有屏障不误报。

use super::*;
use crate::gc::PROMOTION_THRESHOLD;
use crate::gc::{GcMode, MagrGC};

fn gen_heap() -> ArcMagrGC {
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::GenerationalMarkSweep);
    heap
}

/// 让 `v` 经真实的 minor 晋升：pin 住存活够次数再 unpin（老对象不会被 minor 扫）。
fn promote(heap: &ArcMagrGC, v: &Value) {
    let pin = heap.pin_root(v.clone());
    for _ in 0..PROMOTION_THRESHOLD {
        heap.force_collect();
    }
    heap.unpin_root(pin);
}

#[test]
fn an_untouched_heap_satisfies_the_card_invariant() {
    let heap = gen_heap();
    let owner = heap.alloc_object(dummy_type_desc("Owner"), vec![Value::Null], NativeData::None);
    promote(&heap, &owner);
    let _young = heap.alloc_object(dummy_type_desc("Young"), vec![], NativeData::None);
    assert_eq!(heap.verify_card_invariant(), Ok(()));
}

#[test]
fn an_unbarriered_old_to_young_field_store_is_reported_by_owner() {
    let heap = gen_heap();
    let owner = heap.alloc_object(dummy_type_desc("Demo.Owner"), vec![Value::Null], NativeData::None);
    promote(&heap, &owner);
    let young = heap.alloc_object(dummy_type_desc("Young"), vec![], NativeData::None);
    let Value::Object(g) = &owner else { panic!() };
    g.borrow().set_field_value(0, &young); // SATB only — the card half skipped

    let err = heap.verify_card_invariant().expect_err("a missing card must be reported");
    assert!(err.contains("`Demo.Owner`"), "the report names the owner: {err}");

    heap.write_barrier_field(&owner, 0, &young);
    assert_eq!(heap.verify_card_invariant(), Ok(()), "the barrier's card satisfies it");
}

#[test]
fn an_unbarriered_old_to_young_array_store_is_reported() {
    let heap = gen_heap();
    let arr = heap.alloc_array(vec![Value::Null; 3]);
    promote(&heap, &arr);
    let young = heap.alloc_str("young");
    let Value::Array(g) = &arr else { panic!() };
    g.borrow().set_boxed(2, Value::Str(young));

    let err = heap.verify_card_invariant().expect_err("a missing card must be reported");
    assert!(err.contains("old array `[3]`") && err.contains("string"), "{err}");

    heap.write_barrier_array_elem(&arr, 2, &Value::Str(young));
    assert_eq!(heap.verify_card_invariant(), Ok(()));
}

/// Promotion is the invariant's other producer: an owner that becomes old while already holding a
/// young child (stored young→young, no card) must come out of the minor with its card dirty.
#[test]
fn promotion_with_a_young_child_keeps_the_invariant() {
    let heap = gen_heap();
    let owner = heap.alloc_object(dummy_type_desc("Owner"), vec![Value::Null], NativeData::None);
    let pin = heap.pin_root(owner.clone());
    for _ in 1..PROMOTION_THRESHOLD {
        heap.force_collect();
    }
    let child = heap.alloc_object(dummy_type_desc("Child"), vec![], NativeData::None);
    let Value::Object(g) = &owner else { panic!() };
    g.borrow().set_field_value(0, &child);
    heap.write_barrier_field(&owner, 0, &child); // young owner: no card, correctly
    heap.force_collect(); // owner crosses the line holding a still-young child
    heap.unpin_root(pin);
    assert_eq!(heap.verify_card_invariant(), Ok(()));
}

/// No cards outside generational mode ⇒ nothing to check.
#[test]
fn non_generational_modes_always_pass() {
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::StwMarkSweep);
    let owner = heap.alloc_object(dummy_type_desc("Owner"), vec![Value::Null], NativeData::None);
    let young = heap.alloc_object(dummy_type_desc("Young"), vec![], NativeData::None);
    let Value::Object(g) = &owner else { panic!() };
    g.borrow().set_field_value(0, &young);
    assert_eq!(heap.verify_card_invariant(), Ok(()));
}

/// Object model R3: the verifier walks every array mode through `ArrayObj::visit_refs` — REF
/// reference words, MIXED type-parameter cells and `struct[]` reference leaves inside the blob.
/// Each mode on its own heap: an unbarriered old→young store is reported, the barrier for what
/// landed satisfies it.
#[test]
fn every_array_mode_is_checked() {
    use crate::object::{ArrayObj, ElemKind, ElemType, StructTypeLayout, STRUCT_REF_GCREF};
    for mode in ["REF", "MIXED", "STRUCT"] {
        let heap = gen_heap();
        let a = match mode {
            "REF" => ArrayObj::alloc_filled(&heap, ElemType::intern("object"), ElemKind::Ref, 2, Value::Null),
            "MIXED" => ArrayObj::alloc_filled(&heap, ElemType::intern("T"), ElemKind::Mixed, 2, Value::Null),
            _ => ArrayObj::alloc_struct(&heap, ElemType::intern("Demo.S"), 2, std::sync::Arc::new(StructTypeLayout {
                size: 16, ref_offsets: Box::new([8]), ref_kinds: Box::new([STRUCT_REF_GCREF]), fields: Box::new([]),
            })),
        };
        let arr = a;
        let _pin = heap.pin_root(arr.clone());
        promote(&heap, &arr);
        let young = heap.alloc_object(dummy_type_desc("Young"), vec![], NativeData::None);
        let Value::Array(g) = &arr else { panic!() };
        let mut landed = Vec::new();
        if mode == "STRUCT" {
            g.borrow().write_struct_elem(1, &[0u8; 16], &[young], &mut |v| landed.push(*v)).unwrap();
        } else {
            let wrote = g.borrow().set_boxed(1, young);
            wrote.with_barrier_value(&young, |v| landed.push(*v));
        }
        let err = heap.verify_card_invariant().expect_err(&format!("{mode}: a missing card must be reported"));
        assert!(err.contains("[2]` holds young") && err.contains("not remembered"), "{mode}: {err}");
        for v in &landed {
            heap.write_barrier_array_elem(&arr, 1, v);
        }
        assert_eq!(heap.verify_card_invariant(), Ok(()), "{mode}: the barrier's card satisfies it");
    }
}

/// MIXED: a cell claimed for `int` that then receives a `double` stores a young kind-7 box in its
/// tag word. The verifier sees the box, and the `FieldWrite` hands the barrier the box (not the
/// `double`, which is no heap reference and would skip the card).
#[test]
fn a_mixed_cells_kind7_box_is_checked() {
    use crate::object::{ArrayObj, ElemKind, ElemType};
    let heap = gen_heap();
    let _g = crate::gc::ambient::HeapGuard::enter(&heap); // the box is allocated on the ambient heap
    let arr = ArrayObj::alloc_filled(&heap, ElemType::intern("T"), ElemKind::Mixed, 1, Value::Null);
    let _pin = heap.pin_root(arr.clone());
    let Value::Array(g) = &arr else { panic!() };
    let _ = g.borrow().set_boxed(0, Value::I64(7));
    promote(&heap, &arr);
    assert_eq!(heap.verify_card_invariant(), Ok(()), "a primitive needs no card");
    let wrote = g.borrow().set_boxed(0, Value::F64(1.5));
    let mut boxed = Vec::new();
    wrote.with_barrier_value(&Value::F64(1.5), |v| boxed.push(*v));
    assert_eq!(boxed.len(), 1, "a mismatched primitive lands as a box");
    heap.verify_card_invariant().expect_err("the young box on a clean card must be reported");
    heap.write_barrier_array_elem(&arr, 0, &boxed[0]);
    assert_eq!(heap.verify_card_invariant(), Ok(()));
}
