use super::*;
use crate::exec::call::struct_arena::StructArena;
use crate::object::{StructTypeLayout, STRUCT_REF_GCREF};
use std::sync::Arc;

fn layout() -> Arc<StructTypeLayout> {
    Arc::new(StructTypeLayout {
        size: 24,
        ref_offsets: Box::new([8, 16]),
        ref_kinds: Box::new([STRUCT_REF_GCREF, STRUCT_REF_GCREF]),
        fields: Box::new([]),
        float_field_ranges: Default::default(),
    })
}

fn roots(arena: &StructArena) -> Vec<Value> {
    let mut values = Vec::new();
    arena.scan_roots(&mut |value| values.push(*value));
    values
}

#[test]
fn replacing_a_case_drops_its_reference_tail_and_skips_nested_nulls() {
    let mut arena = StructArena::default();
    let slot = arena.alloc(1, Arc::from("$union.Test"), layout(), |_, _| {});
    let first = Value::Str("first".into());
    let second = Value::Str("second".into());
    let args = [Value::Null, Value::I64(1), Value::I64(0), Value::I64(0), first, second];
    arena.with_mut(slot, 1, |s| replace(s, &args[..])).unwrap().unwrap();
    assert_eq!(roots(&arena), vec![first, second]);

    let args = [Value::Null, Value::I64(2), Value::I64(0), Value::I64(42), Value::Null];
    arena.with_mut(slot, 1, |s| replace(s, &args[..])).unwrap().unwrap();
    assert!(roots(&arena).is_empty());
    arena.with(slot, 1, |s| {
        assert_eq!(&s.bytes[..8], &2i64.to_le_bytes());
        assert_eq!(&s.bytes[16..], &42i64.to_le_bytes());
        assert_eq!(&*s.refs, &[Value::Null, Value::Null]);
    }).unwrap();
}

#[test]
fn malformed_arguments_leave_the_old_value_intact() {
    let mut arena = StructArena::default();
    let slot = arena.alloc(1, Arc::from("$union.Test"), layout(), |_, _| {});
    let text = Value::Str("live".into());
    let args = [Value::Null, Value::I64(1), Value::I64(0), Value::I64(0), text];
    arena.with_mut(slot, 1, |s| replace(s, &args[..])).unwrap().unwrap();
    for args in [
        &[Value::Null, Value::I64(2)][..],
        &[Value::Null, Value::I64(2), Value::Null, Value::I64(0)],
        &[Value::Null, Value::I64(2), Value::I64(0), Value::I64(0), Value::I64(7)],
    ] {
        assert!(arena.with_mut(slot, 1, |s| replace(s, args)).unwrap().is_err());
    }
    assert_eq!(roots(&arena), vec![text]);
    assert_eq!(arena.with(slot, 1, |s| s.bytes[0]).unwrap(), 1);
}

#[test]
fn a_value_copy_owns_an_independent_reference_slice() {
    let mut arena = StructArena::default();
    let source = arena.alloc(1, Arc::from("$union.Test"), layout(), |_, _| {});
    let copy = arena.alloc(2, Arc::from("$union.Test"), layout(), |_, _| {});
    let text = Value::Str("live".into());
    let args = [Value::Null, Value::I64(1), Value::I64(0), Value::I64(0), Value::Null, text];
    arena.with_mut(source, 1, |s| replace(s, &args[..])).unwrap().unwrap();
    arena.copy_slots(copy, 2, source, 1, Some(24)).unwrap();
    let empty = [Value::Null, Value::I64(0), Value::I64(0), Value::I64(0)];
    arena.with_mut(source, 1, |s| replace(s, &empty[..])).unwrap().unwrap();
    assert_eq!(roots(&arena), vec![text]);
    arena.with(copy, 2, |s| {
        assert_eq!(s.bytes[0], 1);
        assert_eq!(&*s.refs, &[Value::Null, text]);
    }).unwrap();
}
