use super::*;

// ── 1. Allocation ────────────────────────────────────────────────────────────

#[test]
fn alloc_object_returns_value_object_with_given_fields() {
    let heap = ArcMagrGC::new();
    let td   = dummy_type_desc("Foo");
    let v    = heap.alloc_object(td.clone(), vec![Value::I64(1), Value::I64(2)], NativeData::None);
    let Value::Object(rc) = v else { panic!("expected Value::Object") };
    let borrow = rc.borrow();
    assert_eq!(borrow.type_desc.name, "Foo");
    assert_eq!(borrow.slots.len(), 2);
    assert_eq!(borrow.slots[0], Value::I64(1));
}

#[test]
fn two_alloc_object_calls_return_distinct_rcs() {
    let heap = ArcMagrGC::new();
    let td   = dummy_type_desc("Foo");
    let a    = heap.alloc_object(td.clone(), vec![], NativeData::None);
    let b    = heap.alloc_object(td.clone(), vec![], NativeData::None);
    let (Value::Object(ra), Value::Object(rb)) = (a, b) else { panic!() };
    assert!(!crate::gc::GcRef::ptr_eq(&ra, &rb));
}

#[test]
fn alloc_array_returns_value_array_with_given_elems() {
    let heap = ArcMagrGC::new();
    let v    = heap.alloc_array(vec![Value::I64(7), Value::I64(8), Value::I64(9)]);
    let Value::Array(rc) = v else { panic!("expected Value::Array") };
    let b = rc.borrow();
    assert_eq!(b.len(), 3);
    assert_eq!(b.get_boxed(0), Value::I64(7));
    assert_eq!(b.get_boxed(2), Value::I64(9));
}

#[test]
fn alloc_array_empty_returns_empty_vec() {
    let heap = ArcMagrGC::new();
    let v    = heap.alloc_array(vec![]);
    let Value::Array(rc) = v else { panic!() };
    assert!(rc.borrow().is_empty());
}

