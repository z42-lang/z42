//! objops 单测：错误通道（异常类 + 消息文本）与读写语义。
//!
//! 两个引擎只是这一层的薄适配（interp `exec_array` / `exec_object`，JIT `helpers/array` /
//! `helpers/object_field`），所以这里钉住的类与文本就是两侧的用户可见行为；
//! 适配层各自的映射另有单测（`interp/exec_array_tests.rs`、`jit/helpers/array_tests.rs`、
//! `jit/helpers/object_field_tests.rs`），端到端逐条对照见 `src/tests/exceptions/objops_errors.z42`。

use super::array::{array_get, array_len, array_new, array_new_lit, array_set, packed_data};
use super::field::{field_get, field_set};
use super::{OpError, Throw};
use crate::gc::GcRef;
use crate::metadata::types::{
    ArrayObj, FieldAccess, ObjStorage, ObjectLayout, ScriptObject, TypeDesc, TypeDescCold,
    STRUCT_LEAF_PRIM, TAG_I32, TAG_STR,
};
use crate::metadata::Value;
use crate::metadata::types::ElemType;
use crate::vm_context::VmContext;
use std::sync::Arc;

fn no_fid() -> u32 { panic!("frame id must not be needed here") }

/// 断言是可 catch 的异常，返回 (类, 消息)。
fn thrown(e: OpError) -> Throw {
    match e {
        OpError::Throw(t) => *t,
        other => panic!("expected a catchable exception, got {other:?}"),
    }
}

fn internal(e: OpError) -> String {
    match e {
        OpError::Internal(e) => e.to_string(),
        other => panic!("expected an internal error, got {other:?}"),
    }
}

/// `class Holder { int n; string s; }`：`n` 打包在 bytes（offset 0, 4B），`s` 在 refs 侧表。
fn holder() -> Value {
    let layout = Arc::new(ObjectLayout {
        size: 4,
        field_offsets: Box::new([0, 0]),
        field_sizes:   Box::new([4, 8]),
        field_kinds:   Box::new([STRUCT_LEAF_PRIM, STRUCT_LEAF_PRIM]),
        ref_offsets:   Box::new([0]),
        ref_kinds:     Box::new([TAG_STR]),
        ref_cells:     Box::new([]),
        tparam_cells:  Box::new([]),
        field_access:  Box::new([
            FieldAccess::prim(0, 4, TAG_I32),
            FieldAccess::side(0, TAG_STR, 0),
        ]),
    });
    let mut field_index = crate::metadata::NameIndex::new();
    field_index.insert("n".to_string(), 0);
    field_index.insert("s".to_string(), 1);
    let td = Arc::new(TypeDesc {
        class_flags: 0, visibility: 0, name: "Holder".to_string(), base_name: None,
        fields: Vec::new(), field_index, vtable: Vec::new(),
        vtable_index: crate::metadata::NameIndex::new(),
        cold: Some(Box::new(TypeDescCold { composed_object_layout: Some(layout), ..Default::default() })),
        id: crate::metadata::tokens::TypeId::UNRESOLVED,
    });
    Value::Object(GcRef::new(ScriptObject::new(td, ObjStorage::new(4, 1))))
}

fn long_array(vm: &VmContext, vals: &[i64]) -> Value {
    array_new_lit(vm, vals.iter().map(|&n| Value::I64(n)), ElemType::intern("long"), None::<fn() -> u32>).unwrap()
}

// ── 字段 ─────────────────────────────────────────────────────────────────────

#[test]
fn field_roundtrip_primitive_and_reference() {
    let vm = VmContext::new();
    let h = holder();
    field_set(&vm, &h, "n", &Value::I64(7), None).unwrap();
    field_set(&vm, &h, "s", &Value::Str("hi".into()), None).unwrap();
    assert_eq!(field_get(&vm, &h, "n", None).unwrap(), Value::I64(7));
    assert!(matches!(field_get(&vm, &h, "s", None).unwrap(), Value::Str(_)));
    // 引用字段写 null 合法。
    field_set(&vm, &h, "s", &Value::Null, None).unwrap();
    assert_eq!(field_get(&vm, &h, "s", None).unwrap(), Value::Null);
    // 不存在的字段读出 Null、写入无效果（与历史行为一致）。
    assert_eq!(field_get(&vm, &h, "nope", None).unwrap(), Value::Null);
    field_set(&vm, &h, "nope", &Value::I64(1), None).unwrap();
}

#[test]
fn field_ic_hit_and_miss_agree() {
    let vm = VmContext::new();
    let ic = crate::metadata::resolver::FieldIC::default();
    let h = holder();
    field_set(&vm, &h, "n", &Value::I64(3), Some(&ic)).unwrap();
    // 第一次 miss 装 IC，第二次命中，结果相同（UNRESOLVED 类型号不入 IC，走的仍是查表路径）。
    assert_eq!(field_get(&vm, &h, "n", Some(&ic)).unwrap(), Value::I64(3));
    assert_eq!(field_get(&vm, &h, "n", Some(&ic)).unwrap(), Value::I64(3));
}

#[test]
fn null_receiver_throws_null_reference() {
    let vm = VmContext::new();
    let t = thrown(field_get(&vm, &Value::Null, "n", None).unwrap_err());
    assert_eq!(t.class, "Std.NullReferenceException");
    assert_eq!(t.msg, "cannot read field `n` of a null reference");
    let t = thrown(field_set(&vm, &Value::Null, "n", &Value::I64(1), None).unwrap_err());
    assert_eq!(t.class, "Std.NullReferenceException");
    assert_eq!(t.msg, "cannot write field `n` of a null reference");
    // `.Length` 落到 FieldGet：接收者为 null 时同样是 NRE（不知道它本该是 string 还是数组）。
    let t = thrown(field_get(&vm, &Value::Null, "Length", None).unwrap_err());
    assert_eq!(t.msg, "cannot read field `Length` of a null reference");
}

#[test]
fn rejected_primitive_store_throws_and_keeps_the_old_value() {
    let vm = VmContext::new();
    let h = holder();
    field_set(&vm, &h, "n", &Value::I64(7), None).unwrap();
    let t = thrown(field_set(&vm, &h, "n", &Value::Null, None).unwrap_err());
    assert_eq!(t.class, "Std.NullReferenceException");
    assert_eq!(t.msg, "cannot store null into primitive field `n`");
    let t = thrown(field_set(&vm, &h, "n", &Value::Str("x".into()), None).unwrap_err());
    assert_eq!(t.class, "Std.InvalidCastException");
    assert_eq!(t.msg, "cannot store string into primitive field `n`");
    assert_eq!(field_get(&vm, &h, "n", None).unwrap(), Value::I64(7), "失败的写入不得改动字段");
}

#[test]
fn length_pseudo_fields_and_unknown_fields() {
    let vm = VmContext::new();
    assert_eq!(field_get(&vm, &Value::Str("héllo".into()), "Length", None).unwrap(), Value::I64(5));
    assert_eq!(field_get(&vm, &Value::Str("héllo".into()), "ByteLength", None).unwrap(), Value::I64(6));
    let a = long_array(&vm, &[1, 2, 3]);
    assert_eq!(field_get(&vm, &a, "Length", None).unwrap(), Value::I64(3));
    assert_eq!(field_get(&vm, &a, "Count", None).unwrap(), Value::I64(3));
    assert_eq!(internal(field_get(&vm, &Value::Str("s".into()), "Foo", None).unwrap_err()),
        "string has no field `Foo`");
    assert_eq!(internal(field_get(&vm, &a, "Foo", None).unwrap_err()), "array has no field `Foo`");
    assert_eq!(internal(field_get(&vm, &Value::I64(1), "x", None).unwrap_err()),
        "FieldGet: expected object, got I64(1) (field `x`)");
    assert_eq!(internal(field_set(&vm, &Value::I64(1), "x", &Value::Null, None).unwrap_err()),
        "FieldSet: expected object, got I64(1) (field `x`)");
}

#[test]
fn stack_object_fields_go_through_the_arena() {
    let vm = VmContext::new();
    let Value::Object(rc) = holder() else { unreachable!() };
    let td = rc.type_desc_arc().clone();
    let idx = vm.stack_alloc_obj(5, ScriptObject::new(td, ObjStorage::new(4, 1)));
    let so = Value::StackObject { idx, frame_id: 5 };
    field_set(&vm, &so, "n", &Value::I64(9), None).unwrap();
    assert_eq!(field_get(&vm, &so, "n", None).unwrap(), Value::I64(9));
    // 栈对象同样拒收错类型的基元写入。
    assert_eq!(thrown(field_set(&vm, &so, "n", &Value::Null, None).unwrap_err()).class,
        "Std.NullReferenceException");
}

// ── 数组 ─────────────────────────────────────────────────────────────────────

#[test]
fn array_roundtrip_heap_and_stack() {
    let vm = VmContext::new();
    let a = long_array(&vm, &[10, 20, 30]);
    array_set(&vm, &a, &Value::I64(1), &Value::I64(21)).unwrap();
    assert_eq!(array_get(&vm, &a, &Value::I64(1), no_fid).unwrap(), Value::I64(21));
    assert_eq!(array_len(&vm, &a).unwrap(), 3);

    let s = array_new(&vm, &Value::I64(2), 0, ElemType::intern("long"), None, Some(|| 4u32)).unwrap();
    assert!(matches!(s, Value::StackArray { frame_id: 4, .. }));
    array_set(&vm, &s, &Value::I64(0), &Value::I64(5)).unwrap();
    assert_eq!(array_get(&vm, &s, &Value::I64(0), no_fid).unwrap(), Value::I64(5));
    assert_eq!(array_len(&vm, &s).unwrap(), 2);
}

#[test]
fn null_array_throws_null_reference() {
    let vm = VmContext::new();
    let n = Value::Null;
    let cases = [
        (array_get(&vm, &n, &Value::I64(0), no_fid).map(|_| ()).unwrap_err(), "cannot read an element of a null array"),
        (array_set(&vm, &n, &Value::I64(0), &Value::I64(1)).unwrap_err(), "cannot write an element of a null array"),
        (array_len(&vm, &n).map(|_| ()).unwrap_err(), "cannot read the length of a null array"),
    ];
    for (e, msg) in cases {
        let t = thrown(e);
        assert_eq!(t.class, "Std.NullReferenceException");
        assert_eq!(t.msg, msg);
    }
}

#[test]
fn null_check_comes_before_the_index_check() {
    let vm = VmContext::new();
    let t = thrown(array_get(&vm, &Value::Null, &Value::I64(-1), no_fid).unwrap_err());
    assert_eq!(t.class, "Std.NullReferenceException");
}

#[test]
fn out_of_range_index_throws_index_out_of_range() {
    let vm = VmContext::new();
    for a in [long_array(&vm, &[1, 2, 3]), array_new(&vm, &Value::I64(3), 0, ElemType::intern("long"), None, Some(|| 2u32)).unwrap()] {
        for (i, msg) in [(3, "index 3 is out of range for an array of length 3"),
                         (-1, "index -1 is out of range for an array of length 3")] {
            let t = thrown(array_get(&vm, &a, &Value::I64(i), no_fid).unwrap_err());
            assert_eq!((t.class, t.msg.as_str()), ("Std.IndexOutOfRangeException", msg));
            let t = thrown(array_set(&vm, &a, &Value::I64(i), &Value::I64(0)).unwrap_err());
            assert_eq!((t.class, t.msg.as_str()), ("Std.IndexOutOfRangeException", msg));
        }
    }
    let a = long_array(&vm, &[1]);
    assert_eq!(internal(array_get(&vm, &a, &Value::Str("0".into()), no_fid).unwrap_err()),
        "ArrayGet: array index must be an integer, got Str(\"0\")");
    assert_eq!(internal(array_len(&vm, &Value::I64(0)).unwrap_err()), "ArrayLen: expected array, got I64(0)");
}

#[test]
fn negative_array_size_throws_overflow() {
    let vm = VmContext::new();
    let t = thrown(array_new(&vm, &Value::I64(-2), 0, ElemType::intern("long"), None, None::<fn() -> u32>).unwrap_err());
    assert_eq!(t.class, "Std.OverflowException");
    assert_eq!(t.msg, "array size cannot be negative (got -2)");
    assert_eq!(internal(array_new(&vm, &Value::Null, 0, ElemType::intern("long"), None, None::<fn() -> u32>).unwrap_err()),
        "ArrayNew: array size must be an integer, got Null");
}

#[test]
fn packed_data_never_throws() {
    let vm = VmContext::new();
    let a = array_new(&vm, &Value::I64(4), crate::metadata::types::TAG_I64, ElemType::intern("long"), None, None::<fn() -> u32>).unwrap();
    let (ptr, len, width) = packed_data(&a);
    assert!(!ptr.is_null());
    assert_eq!((len, width), (4, 8));
    for v in [Value::Null, Value::I64(1), Value::Str("s".into())] {
        assert_eq!(packed_data(&v).2, 0, "非打包数组 → 宽度 0（无快路）");
    }
    let boxed = Value::Array(GcRef::new(ArrayObj::new_leaked(vec![Value::Null])));
    assert_eq!(packed_data(&boxed).2, 0);
}

// ── 物化 ─────────────────────────────────────────────────────────────────────

/// 没有 stdlib（模块缺异常类）时两个引擎拿到同一条 `<类名>: <消息>` 文本：
/// interp 作内部错误返回，JIT 作字符串异常。
#[test]
fn materialization_without_stdlib_is_one_text() {
    let vm = VmContext::new();
    let e = OpError::null_field_read("x");
    assert_eq!(e.into_exception(&vm, None).unwrap_err().to_string(),
        "Std.NullReferenceException: cannot read field `x` of a null reference");
    assert_eq!(OpError::index_out_of_range(5, 2).into_anyhow().to_string(),
        "Std.IndexOutOfRangeException: index 5 is out of range for an array of length 2");
}

// ── 调用的 null 接收者 ─────────────────────────────────────────────────────────

/// `VCall` 的方法名带编译器附加的重载键 / 特化后缀，消息里只留源码名；访问器报成属性读写。
#[test]
fn null_call_names_the_source_member() {
    let t = thrown(OpError::null_call("Speak"));
    assert_eq!(t.class, "Std.NullReferenceException");
    assert_eq!(t.msg, "cannot call method `Speak` on a null reference");
    assert_eq!(thrown(OpError::null_call("Substring$2$int$int")).msg,
        "cannot call method `Substring` on a null reference");
    assert_eq!(thrown(OpError::null_call("Second:P2")).msg,
        "cannot call method `Second` on a null reference");
    assert_eq!(thrown(OpError::null_call("get_Length")).msg,
        "cannot read property `Length` of a null reference");
    assert_eq!(thrown(OpError::null_call("set_Name")).msg,
        "cannot write property `Name` of a null reference");
}

/// builtin 经 `anyhow` 抛出的 `Throw` 原样取得回来（引擎据此按类构造），文本与 `into_anyhow` 同一条。
#[test]
fn builtin_error_keeps_the_throw() {
    let e = OpError::null_call("CharAt").into_builtin_error();
    assert_eq!(e.to_string(), "Std.NullReferenceException: cannot call method `CharAt` on a null reference");
    let t = e.downcast::<Throw>().expect("typed throw survives anyhow");
    assert_eq!(t.class, "Std.NullReferenceException");
    // 内部错误照旧是内部错误。
    assert!(OpError::internal("boom".into()).into_builtin_error().downcast::<Throw>().is_err());
}
