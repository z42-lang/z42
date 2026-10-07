//! fix-missing-write-barriers：`verify_card_invariant`——卡表不变量（老对象持有年轻引用 ⇒ 所在卡必脏）
//! 的从头核对。它是 `Z42_GC_VERIFY_CARDS` 的实体，也是 VM 层写屏障回归测试的判据
//! （`objops/write_barrier_tests.rs`），所以这里钉住它两个方向都判得对：漏屏障必报、有屏障不误报。

use super::*;
use crate::gc::region::PROMOTION_THRESHOLD;
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
    g.borrow_mut().set_field_value(0, &young); // SATB only — the card half skipped

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
    g.borrow_mut().set_boxed(2, Value::Str(young));

    let err = heap.verify_card_invariant().expect_err("a missing card must be reported");
    assert!(err.contains("old array (len 3)") && err.contains("string"), "{err}");

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
    g.borrow_mut().set_field_value(0, &child);
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
    g.borrow_mut().set_field_value(0, &young);
    assert_eq!(heap.verify_card_invariant(), Ok(()));
}
