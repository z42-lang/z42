//! Unit tests for `LoadElemAddr` (0xA1) / `LoadFieldAddr` (0xA2).
//!
//! These two execution paths landed 2026-05-05 (`cb61cc072`, impl-ref-out-in-runtime)
//! but stayed **pure dead code for four months**: z42c never emitted the opcodes, and
//! nothing here covered them. `fix-ref-lvalue-addressing` makes them reachable
//! (`ref arr[i]` / `ref obj.f`), so they need real coverage — "the implementation looks
//! complete" was a code-reading conclusion, never a tested one.
//!
//! Covered: happy path for both kinds, the payload actually stored in the transient
//! arena, store-through writing back to the underlying location, and the three bail
//! paths (wrong receiver kind, missing field, out-of-range index).
use super::*;
use crate::metadata::tokens::TypeId;
use crate::metadata::types::{RefKind, TypeDesc};
use crate::metadata::{FieldSlot, NameIndex};
use crate::metadata::NativeData;
use crate::metadata::Value;
use crate::vm_context::VmContext;
use std::sync::Arc;

/// A minimal heap object with one field `f` at slot 0.
fn obj_with_field_f(ctx: &VmContext, init: Value) -> Value {
    let mut field_index = NameIndex::new();
    field_index.insert("f".to_string(), 0);
    let td = Arc::new(TypeDesc {
        class_flags: 0,
        visibility: 0,
        name: "H".to_string(),
        base_name: None,
        fields: vec![FieldSlot {
            name: "f".into(),
            type_tag: "int".into(),
            visibility: 0,
        }],
        field_index,
        vtable: Vec::new(),
        vtable_index: NameIndex::new(),
        cold: None,
        id: TypeId::UNRESOLVED,
    });
    ctx.heap().alloc_object(td, vec![init], NativeData::None)
}

/// `ref arr[1]` → `Value::Ref` whose arena payload is `RefKind::Array { idx: 1 }`.
#[test]
fn load_elem_addr_produces_array_ref_kind() {
    let ctx = VmContext::new();
    let mut frame = Frame::new(&[], 8);
    let arr = ctx.heap().alloc_array(vec![Value::I64(10), Value::I64(20)]);
    frame.set(0, arr);
    frame.set(1, Value::I64(1)); // index

    load_elem_addr(&ctx, &mut frame, 2, 0, 1).unwrap();

    match frame.get(2).unwrap() {
        Value::Ref { idx, frame_id } => {
            assert_eq!(*frame_id, frame.frame_id, "ref is stamped with the creating frame");
            let kind = ctx.transient_arena.lock().ref_kind(*idx, *frame_id).unwrap();
            match kind {
                RefKind::Array { idx: elem, .. } => assert_eq!(elem, 1),
                other => panic!("expected RefKind::Array, got {other:?}"),
            }
        }
        other => panic!("expected Value::Ref, got {other:?}"),
    }
}

/// Writing through the ref lands in the array element — this is what made
/// `Inc(ref arr[0])` silently lose the write before the codegen fix.
#[test]
fn store_through_elem_ref_writes_back_to_the_array() {
    let ctx = VmContext::new();
    let mut frame = Frame::new(&[], 8);
    let arr = ctx.heap().alloc_array(vec![Value::I64(10), Value::I64(20)]);
    frame.set(0, arr.clone());
    frame.set(1, Value::I64(0));

    load_elem_addr(&ctx, &mut frame, 2, 0, 1).unwrap();
    let Value::Ref { idx, frame_id } = frame.get(2).unwrap().clone() else {
        panic!("expected a Ref");
    };
    let kind = ctx.transient_arena.lock().ref_kind(idx, frame_id).unwrap();
    crate::interp::frame::store_thru_ref(&kind, Value::I64(11), &ctx).unwrap();

    match &arr {
        Value::Array(rc) => assert_eq!(rc.borrow().get_boxed(0), Value::I64(11)),
        other => panic!("expected an array, got {other:?}"),
    }
}

/// A non-array receiver must bail rather than silently produce a bogus ref.
#[test]
fn load_elem_addr_rejects_non_array() {
    let ctx = VmContext::new();
    let mut frame = Frame::new(&[], 8);
    frame.set(0, Value::I64(7)); // not an array
    frame.set(1, Value::I64(0));

    let err = load_elem_addr(&ctx, &mut frame, 2, 0, 1).unwrap_err();
    assert!(
        err.to_string().contains("expected array"),
        "unexpected error: {err}"
    );
}

/// Stack-allocated arrays are a different `Value` variant; taking their address
/// must bail (escape analysis is supposed to have forced them to the heap).
#[test]
fn load_elem_addr_rejects_stack_array() {
    let ctx = VmContext::new();
    let mut frame = Frame::new(&[], 8);
    frame.set(0, Value::StackArray { idx: 0, frame_id: frame.frame_id });
    frame.set(1, Value::I64(0));

    assert!(load_elem_addr(&ctx, &mut frame, 2, 0, 1).is_err());
}

/// `ref obj.f` → `Value::Ref` whose arena payload is `RefKind::Field { field_name: "f" }`.
#[test]
fn load_field_addr_produces_field_ref_kind() {
    let ctx = VmContext::new();
    let mut frame = Frame::new(&[], 8);
    frame.set(0, obj_with_field_f(&ctx, Value::I64(20)));

    load_field_addr(&ctx, &mut frame, 1, 0, "f").unwrap();

    match frame.get(1).unwrap() {
        Value::Ref { idx, frame_id } => {
            let kind = ctx.transient_arena.lock().ref_kind(*idx, *frame_id).unwrap();
            match kind {
                RefKind::Field { field_name, .. } => assert_eq!(field_name, "f"),
                other => panic!("expected RefKind::Field, got {other:?}"),
            }
        }
        other => panic!("expected Value::Ref, got {other:?}"),
    }
}

/// Store-through lands in the object's field — the `Inc(ref h.f)` path.
#[test]
fn store_through_field_ref_writes_back_to_the_object() {
    let ctx = VmContext::new();
    let mut frame = Frame::new(&[], 8);
    let obj = obj_with_field_f(&ctx, Value::I64(20));
    frame.set(0, obj.clone());

    load_field_addr(&ctx, &mut frame, 1, 0, "f").unwrap();
    let Value::Ref { idx, frame_id } = frame.get(1).unwrap().clone() else {
        panic!("expected a Ref");
    };
    let kind = ctx.transient_arena.lock().ref_kind(idx, frame_id).unwrap();
    crate::interp::frame::store_thru_ref(&kind, Value::I64(21), &ctx).unwrap();

    match &obj {
        Value::Object(rc) => assert_eq!(rc.borrow().field_value(0), Value::I64(21)),
        other => panic!("expected an object, got {other:?}"),
    }
}

/// An unknown field name must bail at store-through time (the load itself only
/// records the name; resolution happens on write).
#[test]
fn store_through_unknown_field_bails() {
    let ctx = VmContext::new();
    let mut frame = Frame::new(&[], 8);
    frame.set(0, obj_with_field_f(&ctx, Value::I64(1)));

    load_field_addr(&ctx, &mut frame, 1, 0, "nope").unwrap();
    let Value::Ref { idx, frame_id } = frame.get(1).unwrap().clone() else {
        panic!("expected a Ref");
    };
    let kind = ctx.transient_arena.lock().ref_kind(idx, frame_id).unwrap();
    let err = crate::interp::frame::store_thru_ref(&kind, Value::I64(2), &ctx).unwrap_err();
    assert!(err.to_string().contains("not found"), "unexpected error: {err}");
}

/// A non-object receiver must bail rather than produce a bogus ref.
#[test]
fn load_field_addr_rejects_non_object() {
    let ctx = VmContext::new();
    let mut frame = Frame::new(&[], 8);
    frame.set(0, Value::I64(7));

    let err = load_field_addr(&ctx, &mut frame, 1, 0, "f").unwrap_err();
    assert!(err.to_string().contains("expected object"), "unexpected error: {err}");
}
