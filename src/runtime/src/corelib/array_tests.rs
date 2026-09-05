use super::*;
use crate::gc::GcRef;
use crate::metadata::Value;
use crate::vm_context::VmContext;

fn ctx() -> std::pin::Pin<Box<VmContext>> {
    VmContext::new()
}

#[test]
fn clone_primitives_independent() {
    let ctx = ctx();
    let original = Value::Array(GcRef::new(crate::metadata::types::ArrayObj::new_leaked(vec![Value::I64(1), Value::I64(2), Value::I64(3)])));
    let cloned = builtin_array_clone(&ctx, std::slice::from_ref(&original)).expect("clone ok");

    let (orig_rc, copy_rc) = match (&original, &cloned) {
        (Value::Array(o), Value::Array(c)) => (o, c),
        _ => panic!("expected arrays"),
    };
    assert!(!GcRef::ptr_eq(orig_rc, copy_rc), "clone returns a distinct array reference");
    assert_eq!(copy_rc.borrow().len(), 3);

    copy_rc.borrow_mut().set_boxed(0, Value::I64(99));
    assert!(matches!(orig_rc.borrow().get_boxed(0), Value::I64(1)));
    assert!(matches!(copy_rc.borrow().get_boxed(0), Value::I64(99)));
}

#[test]
fn clone_shares_reference_elements() {
    let ctx = ctx();
    let inner = Value::Array(GcRef::new(crate::metadata::types::ArrayObj::new_leaked(vec![Value::I64(7)])));
    let original = Value::Array(GcRef::new(crate::metadata::types::ArrayObj::new_leaked(vec![inner.clone()])));
    let cloned = builtin_array_clone(&ctx, std::slice::from_ref(&original)).expect("clone ok");

    let (orig_rc, copy_rc) = match (&original, &cloned) {
        (Value::Array(o), Value::Array(c)) => (o, c),
        _ => panic!("expected arrays"),
    };
    let orig_inner = orig_rc.borrow().get_boxed(0).clone();
    let copy_inner = copy_rc.borrow().get_boxed(0).clone();
    match (orig_inner, copy_inner) {
        (Value::Array(a), Value::Array(b)) => assert!(GcRef::ptr_eq(&a, &b),
            "shallow clone shares reference-type elements"),
        _ => panic!("expected nested arrays"),
    }
}

#[test]
fn clone_empty_array() {
    let ctx = ctx();
    let empty = Value::Array(GcRef::new(crate::metadata::types::ArrayObj::new_leaked(Vec::new())));
    let cloned = builtin_array_clone(&ctx, std::slice::from_ref(&empty)).expect("clone ok");

    let (orig_rc, copy_rc) = match (&empty, &cloned) {
        (Value::Array(o), Value::Array(c)) => (o, c),
        _ => panic!("expected arrays"),
    };
    assert_eq!(copy_rc.borrow().len(), 0);
    assert!(!GcRef::ptr_eq(orig_rc, copy_rc));
}

#[test]
fn clone_rejects_non_array() {
    let ctx = ctx();
    let err = builtin_array_clone(&ctx, &[Value::I64(42)]).unwrap_err();
    assert!(err.to_string().contains("expected an array"));
}

#[test]
fn clone_rejects_null() {
    let ctx = ctx();
    let err = builtin_array_clone(&ctx, &[Value::Null]).unwrap_err();
    assert!(err.to_string().contains("null array"));
}

// ── __array_copy (perf-bulk-array-copy) ──────────────────────────────────────

fn ints(v: &[i64]) -> Value {
    Value::Array(GcRef::new(crate::metadata::types::ArrayObj::new_leaked(
        v.iter().map(|n| Value::I64(*n)).collect(),
    )))
}

fn read(v: &Value) -> Vec<i64> {
    match v {
        Value::Array(rc) => {
            let a = rc.borrow();
            (0..a.len())
                .map(|i| match a.get_boxed(i) {
                    Value::I64(n) => n,
                    other => panic!("expected I64, got {other:?}"),
                })
                .collect()
        }
        other => panic!("expected array, got {other:?}"),
    }
}

fn copy(ctx: &VmContext, src: &Value, si: i64, dst: &Value, di: i64, n: i64) -> Result<Value> {
    builtin_array_copy(
        ctx,
        &[src.clone(), Value::I64(si), dst.clone(), Value::I64(di), Value::I64(n)],
    )
}

#[test]
fn copy_between_arrays_moves_range() {
    let ctx = ctx();
    let src = ints(&[1, 2, 3, 4, 5]);
    let dst = ints(&[0, 0, 0, 0, 0]);
    copy(&ctx, &src, 1, &dst, 2, 3).expect("copy ok");
    assert_eq!(read(&dst), vec![0, 0, 2, 3, 4]);
    assert_eq!(read(&src), vec![1, 2, 3, 4, 5], "source untouched");
}

#[test]
fn copy_zero_length_is_a_noop() {
    let ctx = ctx();
    let src = ints(&[1, 2, 3]);
    let dst = ints(&[9, 9, 9]);
    // len 0 must not even bounds-check the (otherwise out-of-range) indices.
    copy(&ctx, &src, 3, &dst, 3, 0).expect("zero-length copy ok");
    assert_eq!(read(&dst), vec![9, 9, 9]);
}

#[test]
fn copy_within_same_array_overlapping_forward() {
    let ctx = ctx();
    // dst above src → must copy backward, else the tail clobbers unread elements.
    let a = ints(&[1, 2, 3, 4, 5]);
    copy(&ctx, &a, 0, &a, 2, 3).expect("self copy ok");
    assert_eq!(read(&a), vec![1, 2, 1, 2, 3]);
}

#[test]
fn copy_within_same_array_overlapping_backward() {
    let ctx = ctx();
    let a = ints(&[1, 2, 3, 4, 5]);
    copy(&ctx, &a, 2, &a, 0, 3).expect("self copy ok");
    assert_eq!(read(&a), vec![3, 4, 5, 4, 5]);
}

#[test]
fn copy_within_same_array_same_index_is_a_noop() {
    let ctx = ctx();
    let a = ints(&[1, 2, 3]);
    copy(&ctx, &a, 1, &a, 1, 2).expect("self copy ok");
    assert_eq!(read(&a), vec![1, 2, 3]);
}

#[test]
fn copy_out_of_bounds_errors() {
    let ctx = ctx();
    let src = ints(&[1, 2, 3]);
    let dst = ints(&[0, 0]);
    assert!(copy(&ctx, &src, 0, &dst, 0, 3).is_err(), "destination too short");
    assert!(copy(&ctx, &src, 2, &dst, 0, 2).is_err(), "source range past end");
    let same = ints(&[1, 2, 3]);
    assert!(copy(&ctx, &same, 1, &same, 0, 3).is_err(), "same-array range past end");
}

#[test]
fn copy_rejects_non_arrays_and_negative_indices() {
    let ctx = ctx();
    let src = ints(&[1, 2]);
    let dst = ints(&[0, 0]);
    assert!(builtin_array_copy(&ctx, &[Value::Null, Value::I64(0), dst.clone(), Value::I64(0), Value::I64(1)]).is_err());
    assert!(copy(&ctx, &src, -1, &dst, 0, 1).is_err());
    assert!(copy(&ctx, &src, 0, &dst, 0, -1).is_err());
}
