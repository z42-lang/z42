use super::*;

// ── 4. Object Model ──────────────────────────────────────────────────────────

#[test]
fn object_size_bytes_atomic_returns_value_size() {
    let heap = ArcMagrGC::new();
    let primitives = vec![
        Value::I64(0), Value::F64(0.0), Value::Bool(true), Value::Char('a'), Value::Null,
    ];
    let expected = std::mem::size_of::<Value>();
    for p in primitives {
        assert_eq!(heap.object_size_bytes(&p), expected);
    }
}

#[test]
fn object_size_bytes_string_includes_capacity() {
    let heap = ArcMagrGC::new();
    let s = Value::Str("hello".to_string().into());
    assert!(heap.object_size_bytes(&s) > std::mem::size_of::<Value>());
}

#[test]
fn scan_object_refs_visits_every_slot() {
    let heap = ArcMagrGC::new();
    let a = heap.alloc_object(dummy_type_desc("A"), vec![], NativeData::None);
    let v = heap.alloc_object(
        dummy_type_desc("Foo"),
        vec![a, Value::I64(2), a],
        NativeData::None,
    );
    let mut count = 0;
    heap.scan_object_refs(&v, &mut |_| count += 1);
    // The dummy type's 4 reference fields are type-parameter cells: the scan visits the
    // cells holding a reference (fields 0 and 2) — a primitive or `null` is no edge.
    assert_eq!(count, 2);
}

#[test]
fn scan_object_refs_visits_every_array_elem() {
    let heap = ArcMagrGC::new();
    let a = heap.alloc_object(dummy_type_desc("A"), vec![], NativeData::None);
    let v = heap.alloc_array(vec![a, Value::I64(1), a, Value::Null, a]);
    let mut count = 0;
    heap.scan_object_refs(&v, &mut |_| count += 1);
    // An untyped array is MIXED: only the cells holding a reference are edges.
    assert_eq!(count, 3);
}

#[test]
fn scan_object_refs_no_op_on_atomic() {
    let heap = ArcMagrGC::new();
    let mut count = 0;
    heap.scan_object_refs(&Value::I64(42), &mut |_| count += 1);
    heap.scan_object_refs(&Value::Str("x".into()), &mut |_| count += 1);
    heap.scan_object_refs(&Value::Null, &mut |_| count += 1);
    assert_eq!(count, 0);
}

