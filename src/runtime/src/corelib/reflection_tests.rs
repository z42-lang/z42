//! Unit tests for reflection builtins. These cover the lenient / no-handle
//! paths that don't require z42.core to be loaded. End-to-end enumeration of
//! real fields/methods into populated `FieldInfo`/`MethodInfo` objects is
//! covered by the z42 `[Test]` + golden tests (they run with z42.core present).

use super::*;
use crate::metadata::{tokens::TypeId, NameIndex, NativeData, TypeDesc, Value};
use crate::vm_context::VmContext;
use std::sync::Arc;

fn ctx() -> std::pin::Pin<Box<VmContext>> {
    VmContext::new()
}

fn bare_td(name: &str) -> Arc<TypeDesc> {
    td_with_flags(name, 0)
}

fn td_with_flags(name: &str, class_flags: u8) -> Arc<TypeDesc> {
    Arc::new(TypeDesc {
        name: name.to_string(),
        base_name: None,
        class_flags,
        fields: Vec::new(),
        field_index: NameIndex::new(),
        vtable: Vec::new(),
        vtable_index: NameIndex::new(),
        cold: None,
        id: TypeId::UNRESOLVED,
    })
}

/// A `Std.Type`-like object carrying a `TypeHandle` to `td`.
fn type_obj(ctx: &VmContext, td: Arc<TypeDesc>) -> Value {
    let shell = bare_td("Std.Type");
    ctx.heap()
        .alloc_object(shell, Vec::new(), NativeData::TypeHandle(td))
}

fn array_len(v: &Value) -> usize {
    match v {
        Value::Array(a) => a.borrow().len(),
        _ => panic!("expected Value::Array"),
    }
}

#[test]
fn type_handle_extracts_arc_for_typehandle_object() {
    let c = ctx();
    let t = type_obj(&c, bare_td("Demo.Point"));
    assert!(type_handle(&[t]).is_some());
}

#[test]
fn type_handle_none_for_primitive_plain_object_and_empty() {
    let c = ctx();
    assert!(type_handle(&[Value::I64(1)]).is_none());
    let plain = c
        .heap()
        .alloc_object(bare_td("Demo.X"), Vec::new(), NativeData::None);
    assert!(type_handle(&[plain]).is_none());
    assert!(type_handle(&[]).is_none());
}

#[test]
fn member_builtins_return_empty_for_non_type_arg() {
    let c = ctx();
    assert_eq!(array_len(&builtin_type_fields(&c, &[Value::I64(7)]).unwrap()), 0);
    assert_eq!(array_len(&builtin_type_methods(&c, &[Value::I64(7)]).unwrap()), 0);
    assert_eq!(
        array_len(&builtin_type_generic_args(&c, &[Value::I64(7)]).unwrap()),
        0
    );
}

#[test]
fn type_base_null_for_non_type_arg() {
    let c = ctx();
    assert_eq!(builtin_type_base(&c, &[Value::I64(7)]).unwrap(), Value::Null);
}

#[test]
fn type_fields_empty_for_handle_with_no_fields() {
    // TypeHandle present but zero fields → empty array, and crucially no
    // FieldInfo class lookup (so it works without z42.core loaded).
    let c = ctx();
    let t = type_obj(&c, bare_td("Demo.Empty"));
    assert_eq!(array_len(&builtin_type_fields(&c, &[t]).unwrap()), 0);
}

#[test]
fn type_properties_empty_for_non_type_arg_and_no_accessors() {
    // Lenient: non-Type arg → empty.
    let c = ctx();
    assert_eq!(
        array_len(&builtin_type_properties(&c, &[Value::I64(7)]).unwrap()),
        0
    );
    // Handle present but no methods → no get_/set_ → empty. Crucially no
    // PropertyInfo alloc, so this works without z42.core loaded.
    let t = type_obj(&c, bare_td("Demo.Empty"));
    assert_eq!(array_len(&builtin_type_properties(&c, &[t]).unwrap()), 0);
}

#[test]
fn type_properties_ignores_non_accessor_methods() {
    // A method that isn't `get_` / `set_` must not become a property (and the
    // empty result needs no PropertyInfo class).
    let c = ctx();
    let td = Arc::new(TypeDesc {
        name: "Demo.WithMethod".to_string(),
        base_name: None,
        class_flags: 0,
        fields: Vec::new(),
        field_index: NameIndex::new(),
        vtable: vec![("Foo".to_string(), "Demo.WithMethod.Foo".to_string())],
        vtable_index: NameIndex::new(),
        cold: None,
        id: TypeId::UNRESOLVED,
    });
    let t = type_obj(&c, td);
    assert_eq!(array_len(&builtin_type_properties(&c, &[t]).unwrap()), 0);
}

#[test]
fn type_flags_decode_abstract_and_sealed() {
    use crate::metadata::bytecode::{CLASS_FLAG_ABSTRACT, CLASS_FLAG_SEALED};
    let c = ctx();
    // Abstract-only class.
    let ta = type_obj(&c, td_with_flags("Demo.Shape", CLASS_FLAG_ABSTRACT));
    assert_eq!(builtin_type_is_abstract(&c, &[ta.clone()]).unwrap(), Value::Bool(true));
    assert_eq!(builtin_type_is_sealed(&c, &[ta]).unwrap(), Value::Bool(false));
    // Sealed-only class.
    let ts = type_obj(&c, td_with_flags("Demo.Token", CLASS_FLAG_SEALED));
    assert_eq!(builtin_type_is_abstract(&c, &[ts.clone()]).unwrap(), Value::Bool(false));
    assert_eq!(builtin_type_is_sealed(&c, &[ts]).unwrap(), Value::Bool(true));
    // Plain class (flags = 0).
    let tp = type_obj(&c, bare_td("Demo.Plain"));
    assert_eq!(builtin_type_is_abstract(&c, &[tp.clone()]).unwrap(), Value::Bool(false));
    assert_eq!(builtin_type_is_sealed(&c, &[tp]).unwrap(), Value::Bool(false));
}

#[test]
fn static_fields_accessor_reads_cold() {
    use crate::metadata::bytecode::FieldDesc;
    use crate::metadata::TypeDesc;
    // No cold box → empty.
    assert!(bare_td("Demo.Plain").static_fields().is_empty());
    // Cold box with one static field → accessor returns it.
    let mut td = TypeDesc {
        name: "Demo.Cfg".to_string(),
        base_name: None,
        class_flags: 0,
        fields: Vec::new(),
        field_index: NameIndex::new(),
        vtable: Vec::new(),
        vtable_index: NameIndex::new(),
        cold: None,
        id: TypeId::UNRESOLVED,
    };
    td.cold_mut().static_fields = Box::new([FieldDesc {
        name: "count".to_string(),
        type_tag: "int".to_string(),
        attributes: Box::new([]),
        visibility: 0,
    }]);
    assert_eq!(td.static_fields().len(), 1);
    assert_eq!(td.static_fields()[0].name, "count");
}

#[test]
fn field_custom_attributes_lenient_for_bad_qualified() {
    // Non-string / dotless / unknown-class qualified names → empty, never bail.
    let c = ctx();
    assert_eq!(array_len(&builtin_field_custom_attributes(&c, &[Value::I64(7)]).unwrap()), 0);
    assert_eq!(
        array_len(&builtin_field_custom_attributes(&c, &[Value::Str("nodot".into())]).unwrap()),
        0
    );
    assert_eq!(
        array_len(&builtin_field_custom_attributes(&c, &[Value::Str("Unknown.field".into())]).unwrap()),
        0
    );
}

#[test]
fn field_attributes_accessor_reads_cold() {
    use crate::metadata::bytecode::AttributeRef;
    use crate::metadata::TypeDesc;
    // No cold → empty.
    assert!(bare_td("Demo.Plain").field_attributes().is_empty());
    // Cold with one field's attrs → accessor finds it by name.
    let mut td = TypeDesc {
        name: "Demo.C".to_string(),
        base_name: None,
        class_flags: 0,
        fields: Vec::new(),
        field_index: NameIndex::new(),
        vtable: Vec::new(),
        vtable_index: NameIndex::new(),
        cold: None,
        id: TypeId::UNRESOLVED,
    };
    td.cold_mut().field_attributes = Box::new([(
        "x".into(),
        Box::new([AttributeRef {
            type_name: "Demo.A".to_string(),
            factory_func: "__attr$fld$C$x$0".to_string(),
        }]) as Box<[_]>,
    )]);
    let fa = td.field_attributes();
    assert_eq!(fa.len(), 1);
    assert_eq!(fa[0].0.as_ref(), "x");
    assert_eq!(fa[0].1.len(), 1);
}

#[test]
fn type_fields_empty_for_non_type_arg_with_static() {
    // builtin_type_fields stays lenient for a non-Type arg even after the
    // static-field merge (no handle → empty, never bail).
    let c = ctx();
    assert_eq!(array_len(&builtin_type_fields(&c, &[Value::I64(7)]).unwrap()), 0);
}

#[test]
fn type_flags_false_for_handle_less() {
    // Non-Type arg / no handle → false, never bail (lenient).
    let c = ctx();
    assert_eq!(builtin_type_is_abstract(&c, &[Value::I64(7)]).unwrap(), Value::Bool(false));
    assert_eq!(builtin_type_is_sealed(&c, &[]).unwrap(), Value::Bool(false));
    assert_eq!(builtin_type_is_value_type(&c, &[Value::I64(7)]).unwrap(), Value::Bool(false));
    assert_eq!(builtin_type_is_record(&c, &[]).unwrap(), Value::Bool(false));
}

#[test]
fn type_flags_decode_struct_and_record() {
    use crate::metadata::bytecode::{CLASS_FLAG_RECORD, CLASS_FLAG_STRUCT};
    let c = ctx();
    // Struct-only.
    let tv = type_obj(&c, td_with_flags("Demo.Point", CLASS_FLAG_STRUCT));
    assert_eq!(builtin_type_is_value_type(&c, &[tv.clone()]).unwrap(), Value::Bool(true));
    assert_eq!(builtin_type_is_record(&c, &[tv]).unwrap(), Value::Bool(false));
    // Record-only.
    let tr = type_obj(&c, td_with_flags("Demo.Pair", CLASS_FLAG_RECORD));
    assert_eq!(builtin_type_is_value_type(&c, &[tr.clone()]).unwrap(), Value::Bool(false));
    assert_eq!(builtin_type_is_record(&c, &[tr]).unwrap(), Value::Bool(true));
    // Plain class (flags 0).
    let tp = type_obj(&c, bare_td("Demo.Plain"));
    assert_eq!(builtin_type_is_value_type(&c, &[tp.clone()]).unwrap(), Value::Bool(false));
    assert_eq!(builtin_type_is_record(&c, &[tp]).unwrap(), Value::Bool(false));
}

#[test]
fn member_builtins_are_lenient_with_handle_but_no_core() {
    // With a handle but z42.core absent, methods/base/generics must not error
    // (they degrade — base resolves to a null Std.Type, etc.).
    let c = ctx();
    let t = type_obj(&c, bare_td("Demo.Foo"));
    assert!(builtin_type_methods(&c, &[t.clone()]).is_ok());
    assert!(builtin_type_generic_args(&c, &[t.clone()]).is_ok());
    assert!(builtin_type_base(&c, &[t]).is_ok());
}
