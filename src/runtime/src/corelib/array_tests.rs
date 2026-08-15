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
