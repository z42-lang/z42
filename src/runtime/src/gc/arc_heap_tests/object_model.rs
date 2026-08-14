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
    let v = heap.alloc_object(
        dummy_type_desc("Foo"),
        vec![Value::I64(1), Value::I64(2), Value::I64(3)],
        NativeData::None,
    );
    let mut count = 0;
    heap.scan_object_refs(&v, &mut |_| count += 1);
    // unify-object-byte-layout (PR-2): the dummy type has 4 reference fields → 4 refs
    // slots (the 3 initial values fill refs[0..3], refs[3] stays Null); scan visits all.
    assert_eq!(count, 4);
}

#[test]
fn scan_object_refs_visits_every_array_elem() {
    let heap = ArcMagrGC::new();
    let v = heap.alloc_array(vec![Value::I64(1); 5]);
    let mut count = 0;
    heap.scan_object_refs(&v, &mut |_| count += 1);
    assert_eq!(count, 5);
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

