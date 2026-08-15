use super::*;
use crate::metadata::{NativeData, TypeDesc, Value};
use crate::vm_context::VmContext;
use std::sync::Arc;

fn s(v: &str) -> Value { Value::Str(v.into()) }
fn i(n: i64) -> Value { Value::I64(n) }
fn i64(n: i64) -> Value { Value::I64(n) }

/// Build a fresh VmContext for each test (heap is fully isolated, fast to construct).
fn ctx() -> std::pin::Pin<Box<VmContext>> { VmContext::new() }

/// A VmContext with a minimal `Std.Type` class seeded (fields `__name` @0,
/// `__fullName` @1). `__obj_get_type` → `make_type_object` → `build_type`
/// requires `Std.Type` to be loaded (in production it comes from z42.core);
/// without it `build_type` returns `Value::Null`. Seed it so these unit tests
/// exercise the real `Std.Type`-producing path. (make-typeof-return-type C2 /
/// add-attribute-reflection C3 made this path z42.core-dependent.)
fn ctx_with_std_type() -> std::pin::Pin<Box<VmContext>> {
    use crate::metadata::{name_index::NameIndex, tokens::TypeId, FieldSlot};
    let c = VmContext::new();
    c.install_lazy_loader(None, 0);
    let mut fi = NameIndex::new();
    fi.insert("__name".to_string(), 0);
    fi.insert("__fullName".to_string(), 1);
    let td = Arc::new(TypeDesc {
        class_flags: 0,
        visibility: 0,
        name: "Std.Type".to_string(),
        id: TypeId::UNRESOLVED,
        base_name: None,
        fields: vec![
            FieldSlot { name: "__name".to_string().into(),     type_tag: "str".to_string().into(), visibility: 0 },
            FieldSlot { name: "__fullName".to_string().into(), type_tag: "str".to_string().into(), visibility: 0 },
        ],
        field_index: fi,
        vtable: Vec::new(),
        vtable_index: NameIndex::new(),
        cold: None,
    });
    let mut types = std::collections::HashMap::new();
    types.insert("Std.Type".to_string(), td);
    c.seed_lazy_loader_types(&types);
    c
}

/// Allocate a minimal Object with the given class name through the heap interface.
fn obj(ctx: &VmContext, class_name: &str) -> Value {
    let type_desc = Arc::new(TypeDesc {
        class_flags: 0,
        visibility: 0,
        name: class_name.to_string(),
        base_name: None,
        fields: Vec::new(),
        field_index: crate::metadata::NameIndex::new(),
        vtable: Vec::new(),
        vtable_index: crate::metadata::NameIndex::new(),
        cold: None,
        id: crate::metadata::tokens::TypeId::UNRESOLVED,
    });
    ctx.heap().alloc_object(type_desc, Vec::new(), NativeData::None)
}

// ── __len ─────────────────────────────────────────────────────────────────────

#[test]
fn len_of_string_is_utf8_bytes() {
    let c = ctx();
    assert_eq!(exec_builtin(&c, "__len", &[s("hello")]).unwrap(), i64(5));
}

#[test]
fn len_of_empty_string() {
    let c = ctx();
    assert_eq!(exec_builtin(&c, "__len", &[s("")]).unwrap(), i64(0));
}

#[test]
fn len_missing_arg_errors() {
    let c = ctx();
    assert!(exec_builtin(&c, "__len", &[]).is_err());
}

// ── __str_char_at (new in simplify-string-stdlib 2026-04-24) ──────────────────

#[test]
fn char_at_returns_nth_scalar() {
    let c = ctx();
    assert_eq!(exec_builtin(&c, "__str_char_at", &[s("hello"), i(1)]).unwrap(), Value::Char('e'));
    assert_eq!(exec_builtin(&c, "__str_char_at", &[s("hello"), i(0)]).unwrap(), Value::Char('h'));
    assert_eq!(exec_builtin(&c, "__str_char_at", &[s("hello"), i(4)]).unwrap(), Value::Char('o'));
}

#[test]
fn char_at_out_of_range_errors() {
    let c = ctx();
    assert!(exec_builtin(&c, "__str_char_at", &[s("abc"), i(5)]).is_err());
}

#[test]
fn char_at_unicode_scalar_index() {
    // "α" is one scalar but 2 UTF-8 bytes; script-level API treats it as one unit.
    let c = ctx();
    assert_eq!(exec_builtin(&c, "__str_char_at", &[s("αβγ"), i(1)]).unwrap(), Value::Char('β'));
}

// ── __str_from_chars (new in simplify-string-stdlib 2026-04-24) ──────────────

#[test]
fn from_chars_builds_string() {
    let c = ctx();
    let arr = c.heap().alloc_array(vec![
        Value::Char('h'), Value::Char('i'),
    ]);
    assert_eq!(exec_builtin(&c, "__str_from_chars", &[arr]).unwrap(), s("hi"));
}

#[test]
fn from_chars_empty_array() {
    let c = ctx();
    let arr = c.heap().alloc_array(vec![]);
    assert_eq!(exec_builtin(&c, "__str_from_chars", &[arr]).unwrap(), s(""));
}

// ── __char_is_whitespace / __char_to_lower / __char_to_upper ─────────────────

#[test]
fn char_is_whitespace_ascii() {
    let c = ctx();
    assert_eq!(exec_builtin(&c, "__char_is_whitespace", &[Value::Char(' ')]).unwrap(), Value::Bool(true));
    assert_eq!(exec_builtin(&c, "__char_is_whitespace", &[Value::Char('\t')]).unwrap(), Value::Bool(true));
    assert_eq!(exec_builtin(&c, "__char_is_whitespace", &[Value::Char('\n')]).unwrap(), Value::Bool(true));
    assert_eq!(exec_builtin(&c, "__char_is_whitespace", &[Value::Char('a')]).unwrap(), Value::Bool(false));
}

#[test]
fn char_to_lower_ascii_only() {
    let c = ctx();
    assert_eq!(exec_builtin(&c, "__char_to_lower", &[Value::Char('A')]).unwrap(), Value::Char('a'));
    assert_eq!(exec_builtin(&c, "__char_to_lower", &[Value::Char('Z')]).unwrap(), Value::Char('z'));
    assert_eq!(exec_builtin(&c, "__char_to_lower", &[Value::Char('1')]).unwrap(), Value::Char('1'));
    assert_eq!(exec_builtin(&c, "__char_to_lower", &[Value::Char('a')]).unwrap(), Value::Char('a'));
}

#[test]
fn char_to_upper_ascii_only() {
    let c = ctx();
    assert_eq!(exec_builtin(&c, "__char_to_upper", &[Value::Char('a')]).unwrap(), Value::Char('A'));
    assert_eq!(exec_builtin(&c, "__char_to_upper", &[Value::Char('z')]).unwrap(), Value::Char('Z'));
    assert_eq!(exec_builtin(&c, "__char_to_upper", &[Value::Char('!')]).unwrap(), Value::Char('!'));
}

// ── dispatch table coverage ───────────────────────────────────────────────────

#[test]
fn unknown_builtin_errors() {
    let c = ctx();
    assert!(exec_builtin(&c, "__nonexistent", &[]).is_err());
}

#[test]
fn println_via_dispatch_table() {
    let c = ctx();
    assert!(exec_builtin(&c, "__println", &[s("test")]).is_ok());
}

// ── __obj_get_type ────────────────────────────────────────────────────────────

#[test]
fn obj_get_type_returns_type_object() {
    let c = ctx_with_std_type();
    let result = exec_builtin(&c, "__obj_get_type", &[obj(&c, "Foo")]).unwrap();
    match result {
        Value::Object(rc) => assert_eq!(rc.type_desc().name, "Std.Type"),
        other => panic!("expected Object, got {:?}", other),
    }
}

#[test]
fn obj_get_type_simple_name_no_namespace() {
    let c = ctx_with_std_type();
    let result = exec_builtin(&c, "__obj_get_type", &[obj(&c, "Foo")]).unwrap();
    let Value::Object(rc) = result else { panic!("expected Object") };
    let borrow = rc.borrow();
    assert_eq!(borrow.field_value(0), Value::Str("Foo".into()));
    assert_eq!(borrow.field_value(1), Value::Str("Foo".into()));
}

#[test]
fn obj_get_type_namespaced_class_splits_name() {
    let c = ctx_with_std_type();
    let result = exec_builtin(&c, "__obj_get_type", &[obj(&c, "geometry.Circle")]).unwrap();
    let Value::Object(rc) = result else { panic!("expected Object") };
    let borrow = rc.borrow();
    assert_eq!(borrow.field_value(0), Value::Str("Circle".into()));
    assert_eq!(borrow.field_value(1), Value::Str("geometry.Circle".into()));
}

#[test]
fn obj_get_type_null_errors() {
    let c = ctx();
    assert!(exec_builtin(&c, "__obj_get_type", &[Value::Null]).is_err());
}

#[test]
fn obj_get_type_primitive_returns_type() {
    // add-primitive-value-boxing: 基元现在也有类型标识——`__obj_get_type` 对裸基元
    // （未装箱 I64 等）返回其基元 Type（Std.Int32…），不再 error。装箱基元走 Boxed 臂
    // 用精确 b.class；此处覆盖裸基元兜底臂。
    let c = ctx();
    assert!(exec_builtin(&c, "__obj_get_type", &[i(42)]).is_ok());
}

// ── __obj_ref_eq ──────────────────────────────────────────────────────────────

#[test]
fn obj_ref_eq_same_rc_is_true() {
    let c = ctx();
    let a = obj(&c, "Foo");
    assert_eq!(
        exec_builtin(&c, "__obj_ref_eq", &[a.clone(), a]).unwrap(),
        Value::Bool(true)
    );
}

#[test]
fn obj_ref_eq_different_allocs_is_false() {
    let c = ctx();
    assert_eq!(
        exec_builtin(&c, "__obj_ref_eq", &[obj(&c, "Foo"), obj(&c, "Foo")]).unwrap(),
        Value::Bool(false)
    );
}

#[test]
fn obj_ref_eq_both_null_is_true() {
    let c = ctx();
    assert_eq!(
        exec_builtin(&c, "__obj_ref_eq", &[Value::Null, Value::Null]).unwrap(),
        Value::Bool(true)
    );
}

#[test]
fn obj_ref_eq_one_null_is_false() {
    let c = ctx();
    assert_eq!(
        exec_builtin(&c, "__obj_ref_eq", &[obj(&c, "Foo"), Value::Null]).unwrap(),
        Value::Bool(false)
    );
    assert_eq!(
        exec_builtin(&c, "__obj_ref_eq", &[Value::Null, obj(&c, "Foo")]).unwrap(),
        Value::Bool(false)
    );
}

// ── __obj_hash_code ───────────────────────────────────────────────────────────

#[test]
fn obj_hash_code_returns_i32() {
    let c = ctx();
    let result = exec_builtin(&c, "__obj_hash_code", &[obj(&c, "Foo")]).unwrap();
    assert!(matches!(result, Value::I64(_)));
}

#[test]
fn obj_hash_code_same_object_is_consistent() {
    let c = ctx();
    let a = obj(&c, "Foo");
    let h1 = exec_builtin(&c, "__obj_hash_code", &[a.clone()]).unwrap();
    let h2 = exec_builtin(&c, "__obj_hash_code", &[a]).unwrap();
    assert_eq!(h1, h2);
}

#[test]
fn obj_hash_code_null_is_zero() {
    let c = ctx();
    assert_eq!(
        exec_builtin(&c, "__obj_hash_code", &[Value::Null]).unwrap(),
        Value::I64(0)
    );
}

#[test]
fn obj_hash_code_non_object_errors() {
    let c = ctx();
    assert!(exec_builtin(&c, "__obj_hash_code", &[i(1)]).is_err());
}

// ── __delegate_eq (2026-05-03 fix-delegate-reference-equality, D-5) ───────────

use crate::gc::GcRef;

fn fn_ref(name: &str) -> Value { Value::FuncRef(name.into()) }

#[test]
fn delegate_eq_same_funcref_equal() {
    let c = ctx();
    let a = fn_ref("Demo.Helper");
    let b = fn_ref("Demo.Helper");
    assert_eq!(exec_builtin(&c, "__delegate_eq", &[a, b]).unwrap(), Value::Bool(true));
}

#[test]
fn delegate_eq_diff_funcref_not_equal() {
    let c = ctx();
    let a = fn_ref("Demo.A");
    let b = fn_ref("Demo.B");
    assert_eq!(exec_builtin(&c, "__delegate_eq", &[a, b]).unwrap(), Value::Bool(false));
}

#[test]
fn delegate_eq_same_closure_equal_via_ptr_eq() {
    let c = ctx();
    let env = GcRef::new(crate::metadata::types::ArrayObj::new(vec![Value::I64(1)]));
    let a = Value::Closure(crate::gc::var_region::VarGcRef::leak_for_test(crate::metadata::ClosureData { env: env.clone(), fn_name: "Demo.Lambda".into() }, crate::gc::var_region::BlockType::Closure));
    let b = Value::Closure(crate::gc::var_region::VarGcRef::leak_for_test(crate::metadata::ClosureData { env: env.clone(), fn_name: "Demo.Lambda".into() }, crate::gc::var_region::BlockType::Closure));
    assert_eq!(exec_builtin(&c, "__delegate_eq", &[a, b]).unwrap(), Value::Bool(true));
}

#[test]
fn delegate_eq_diff_closure_env_not_equal() {
    let c = ctx();
    let a = Value::Closure(crate::gc::var_region::VarGcRef::leak_for_test(crate::metadata::ClosureData { env: GcRef::new(crate::metadata::types::ArrayObj::new(vec![Value::I64(1)])), fn_name: "Demo.Lambda".into() }, crate::gc::var_region::BlockType::Closure));
    let b = Value::Closure(crate::gc::var_region::VarGcRef::leak_for_test(crate::metadata::ClosureData { env: GcRef::new(crate::metadata::types::ArrayObj::new(vec![Value::I64(1)])), fn_name: "Demo.Lambda".into() }, crate::gc::var_region::BlockType::Closure));
    assert_eq!(exec_builtin(&c, "__delegate_eq", &[a, b]).unwrap(), Value::Bool(false));
}

#[test]
fn delegate_eq_same_stackclosure_equal() {
    let c = ctx();
    let a = Value::StackClosure(Box::new(crate::metadata::StackClosureData { env_idx: 0, fn_name: "Demo.Stack".into() }));
    let b = Value::StackClosure(Box::new(crate::metadata::StackClosureData { env_idx: 0, fn_name: "Demo.Stack".into() }));
    assert_eq!(exec_builtin(&c, "__delegate_eq", &[a, b]).unwrap(), Value::Bool(true));
}

#[test]
fn delegate_eq_diff_stackclosure_idx_not_equal() {
    let c = ctx();
    let a = Value::StackClosure(Box::new(crate::metadata::StackClosureData { env_idx: 0, fn_name: "Demo.Stack".into() }));
    let b = Value::StackClosure(Box::new(crate::metadata::StackClosureData { env_idx: 1, fn_name: "Demo.Stack".into() }));
    assert_eq!(exec_builtin(&c, "__delegate_eq", &[a, b]).unwrap(), Value::Bool(false));
}

#[test]
fn delegate_eq_funcref_vs_closure_not_equal() {
    let c = ctx();
    let a = fn_ref("Demo.F");
    let b = Value::Closure(crate::gc::var_region::VarGcRef::leak_for_test(crate::metadata::ClosureData { env: GcRef::new(crate::metadata::types::ArrayObj::new(vec![])), fn_name: "Demo.F".into() }, crate::gc::var_region::BlockType::Closure));
    assert_eq!(exec_builtin(&c, "__delegate_eq", &[a, b]).unwrap(), Value::Bool(false));
}

#[test]
fn delegate_eq_closure_vs_stackclosure_not_equal() {
    let c = ctx();
    let a = Value::Closure(crate::gc::var_region::VarGcRef::leak_for_test(crate::metadata::ClosureData { env: GcRef::new(crate::metadata::types::ArrayObj::new(vec![])), fn_name: "Demo.F".into() }, crate::gc::var_region::BlockType::Closure));
    let b = Value::StackClosure(Box::new(crate::metadata::StackClosureData { env_idx: 0, fn_name: "Demo.F".into() }));
    assert_eq!(exec_builtin(&c, "__delegate_eq", &[a, b]).unwrap(), Value::Bool(false));
}

#[test]
fn delegate_eq_both_null_equal() {
    let c = ctx();
    assert_eq!(exec_builtin(&c, "__delegate_eq", &[Value::Null, Value::Null]).unwrap(), Value::Bool(true));
}

#[test]
fn delegate_eq_null_vs_funcref_not_equal() {
    let c = ctx();
    assert_eq!(
        exec_builtin(&c, "__delegate_eq", &[Value::Null, fn_ref("Demo.F")]).unwrap(),
        Value::Bool(false)
    );
}

#[test]
fn delegate_eq_non_delegate_values_returns_false() {
    let c = ctx();
    assert_eq!(exec_builtin(&c, "__delegate_eq", &[i(5), s("foo")]).unwrap(), Value::Bool(false));
    assert_eq!(exec_builtin(&c, "__delegate_eq", &[obj(&c, "Foo"), i(0)]).unwrap(), Value::Bool(false));
}

// ── __obj_make_weak / __obj_upgrade_weak (2026-05-04 expose-weak-ref-builtin, D-1a) ─

#[test]
fn make_weak_object_returns_handle_object() {
    let c = ctx();
    let target = obj(&c, "Foo");
    let handle = exec_builtin(&c, "__obj_make_weak", &[target]).unwrap();
    assert!(matches!(handle, Value::Object(_)));
}

#[test]
fn make_weak_primitive_returns_null() {
    let c = ctx();
    assert_eq!(exec_builtin(&c, "__obj_make_weak", &[i(5)]).unwrap(), Value::Null);
    assert_eq!(exec_builtin(&c, "__obj_make_weak", &[s("foo")]).unwrap(), Value::Null);
    assert_eq!(exec_builtin(&c, "__obj_make_weak", &[Value::Bool(true)]).unwrap(), Value::Null);
}

#[test]
fn upgrade_weak_alive_returns_original() {
    let c = ctx();
    let target = obj(&c, "Foo");
    let handle = exec_builtin(&c, "__obj_make_weak", &[target.clone()]).unwrap();
    let upgraded = exec_builtin(&c, "__obj_upgrade_weak", &[handle]).unwrap();
    assert_eq!(
        exec_builtin(&c, "__obj_ref_eq", &[target, upgraded]).unwrap(),
        Value::Bool(true)
    );
}

#[test]
fn upgrade_weak_non_handle_returns_null() {
    let c = ctx();
    assert_eq!(exec_builtin(&c, "__obj_upgrade_weak", &[Value::Null]).unwrap(), Value::Null);
    assert_eq!(exec_builtin(&c, "__obj_upgrade_weak", &[obj(&c, "NotAHandle")]).unwrap(), Value::Null);
    assert_eq!(exec_builtin(&c, "__obj_upgrade_weak", &[i(5)]).unwrap(), Value::Null);
}

#[test]
fn make_weak_then_upgrade_array() {
    let c = ctx();
    let arr = Value::Array(crate::gc::GcRef::new(crate::metadata::types::ArrayObj::new(vec![i(1), i(2)])));
    let handle = exec_builtin(&c, "__obj_make_weak", &[arr.clone()]).unwrap();
    assert!(matches!(handle, Value::Object(_)));
    let upgraded = exec_builtin(&c, "__obj_upgrade_weak", &[handle]).unwrap();
    assert!(matches!(upgraded, Value::Array(_)));
}

// ── __delegate_target / __delegate_fn_name / __make_closure (D-1b, 2026-05-04) ──

fn closure(env: Vec<Value>, fn_name: &str) -> Value {
    Value::Closure(crate::gc::var_region::VarGcRef::leak_for_test(
        crate::metadata::ClosureData {
            env: crate::gc::GcRef::new(crate::metadata::types::ArrayObj::new(env)),
            fn_name: fn_name.to_string(),
        },
        crate::gc::var_region::BlockType::Closure,
    ))
}

#[test]
fn delegate_target_extracts_object_from_closure_env() {
    let c = ctx();
    let receiver = obj(&c, "Listener");
    let cl = closure(vec![receiver.clone()], "thunk_OnTick");
    let target = exec_builtin(&c, "__delegate_target", &[cl]).unwrap();
    // 提取出来的对象与原 receiver 引用相等
    assert_eq!(
        exec_builtin(&c, "__obj_ref_eq", &[target, receiver]).unwrap(),
        Value::Bool(true)
    );
}

#[test]
fn delegate_target_returns_null_for_empty_env() {
    let c = ctx();
    let cl = closure(vec![], "lambda_no_capture");
    assert_eq!(exec_builtin(&c, "__delegate_target", &[cl]).unwrap(), Value::Null);
}

#[test]
fn delegate_target_returns_null_for_non_object_env_first() {
    let c = ctx();
    let cl = closure(vec![i(42), s("x")], "lambda_int_capture");
    assert_eq!(exec_builtin(&c, "__delegate_target", &[cl]).unwrap(), Value::Null);
}

#[test]
fn delegate_target_returns_null_for_funcref() {
    let c = ctx();
    let f = Value::FuncRef("Helper".into());
    assert_eq!(exec_builtin(&c, "__delegate_target", &[f]).unwrap(), Value::Null);
}

#[test]
fn delegate_target_returns_null_for_stack_closure() {
    let c = ctx();
    let sc = Value::StackClosure(Box::new(crate::metadata::StackClosureData { env_idx: 0, fn_name: "stack_lambda".into() }));
    assert_eq!(exec_builtin(&c, "__delegate_target", &[sc]).unwrap(), Value::Null);
}

#[test]
fn delegate_fn_name_returns_string() {
    let c = ctx();
    let cl = closure(vec![obj(&c, "Foo")], "thunk_Bar");
    assert_eq!(exec_builtin(&c, "__delegate_fn_name", &[cl]).unwrap(), s("thunk_Bar"));
}

#[test]
fn delegate_fn_name_works_for_funcref_and_stack_closure() {
    let c = ctx();
    assert_eq!(
        exec_builtin(&c, "__delegate_fn_name", &[Value::FuncRef("free_fn".into())]).unwrap(),
        s("free_fn"));
    let sc = Value::StackClosure(Box::new(crate::metadata::StackClosureData { env_idx: 3, fn_name: "stk".into() }));
    assert_eq!(exec_builtin(&c, "__delegate_fn_name", &[sc]).unwrap(), s("stk"));
}

#[test]
fn delegate_fn_name_returns_null_for_non_delegate() {
    let c = ctx();
    assert_eq!(exec_builtin(&c, "__delegate_fn_name", &[i(5)]).unwrap(), Value::Null);
    assert_eq!(exec_builtin(&c, "__delegate_fn_name", &[Value::Null]).unwrap(), Value::Null);
}

#[test]
fn make_closure_constructs_value_closure() {
    let c = ctx();
    let receiver = obj(&c, "Listener");
    let env_arr = Value::Array(crate::gc::GcRef::new(crate::metadata::types::ArrayObj::new(vec![receiver.clone()])));
    let cl = exec_builtin(&c, "__make_closure", &[s("thunk_X"), env_arr]).unwrap();
    match cl {
        Value::Closure(cd) => {
            let data = crate::metadata::types::closure_data_of(&cd);
            assert_eq!(data.fn_name, "thunk_X");
            // env[0] 应是同一 receiver
            let env_ref = data.env.borrow();
            assert_eq!(env_ref.len(), 1);
            match &env_ref.get_boxed(0) {
                Value::Object(_) => {
                    let upgraded = env_ref.get_boxed(0);
                    assert_eq!(
                        exec_builtin(&c, "__obj_ref_eq", &[upgraded, receiver]).unwrap(),
                        Value::Bool(true)
                    );
                }
                _ => panic!("env[0] should be Object"),
            }
        }
        _ => panic!("expected Closure, got {:?}", cl),
    }
}

#[test]
fn make_closure_returns_null_for_invalid_args() {
    let c = ctx();
    // fn_name 不是 string
    let env_arr = Value::Array(crate::gc::GcRef::new(crate::metadata::types::ArrayObj::new(vec![])));
    assert_eq!(exec_builtin(&c, "__make_closure", &[i(5), env_arr.clone()]).unwrap(), Value::Null);
    // env 不是 array
    assert_eq!(exec_builtin(&c, "__make_closure", &[s("x"), i(5)]).unwrap(), Value::Null);
}
