use super::*;
use crate::metadata::Value;

// __str_to_chars: bulk materialise the whole char[] in one native call — the
// array-view primitive backing script-side string ops (String.ToCharArray →
// IndexOf/…). Must yield Unicode scalars (chars), not bytes.
#[test]
fn to_chars_yields_scalars() {
    let ctx = VmContext::new();
    let out = str_to_chars(&ctx, &"héllo".into());
    match out {
        Value::Array(a) => {
            let got: Vec<char> = a.borrow().iter_boxed().map(|v| match v {
                Value::Char(c) => c,
                other => panic!("expected char, got {:?}", other),
            }).collect();
            // "héllo" = 5 scalars (é is one scalar though 2 UTF-8 bytes).
            assert_eq!(got, vec!['h', 'é', 'l', 'l', 'o']);
        }
        other => panic!("expected Array, got {:?}", other),
    }
}

#[test]
fn to_chars_empty() {
    let ctx = VmContext::new();
    let out = str_to_chars(&ctx, &"".into());
    match out {
        Value::Array(a) => assert_eq!(a.borrow().len(), 0),
        other => panic!("expected empty Array, got {:?}", other),
    }
}

fn strings_of(v: Value) -> Vec<String> {
    match v {
        Value::Array(a) => a.borrow().iter_boxed().map(|v| match v {
            Value::Str(s) => s.to_string(),
            other => panic!("expected string, got {:?}", other),
        }).collect(),
        other => panic!("expected Array, got {:?}", other),
    }
}

// __str_split: non-overlapping, left-to-right, empty pieces kept (the script
// `String.Split(string)` contract), multi-byte text split on char boundaries.
#[test]
fn split_keeps_empty_pieces_and_matches_left_to_right() {
    let ctx = VmContext::new();
    let split = |s: &str, sep: &str| str_split(&ctx, &s.into(), sep).unwrap();
    assert_eq!(split("a,b,,c", ","), vec!["a", "b", "", "c"]);
    assert_eq!(split(",a,", ","), vec!["", "a", ""]);
    assert_eq!(split("abc", ","), vec!["abc"]);
    assert_eq!(split("", ","), vec![""]);
    assert_eq!(split("aaa", "aa"), vec!["", "a"]);
    assert_eq!(split("x->y->z", "->"), vec!["x", "y", "z"]);
    assert_eq!(split("你,好,世界", ","), vec!["你", "好", "世界"]);
    assert!(str_split(&ctx, &"a".into(), "").is_err());
}

// __str_join: interleave with the separator, one allocation; non-string elements rejected.
#[test]
fn join_interleaves_and_rejects_non_strings() {
    let ctx = VmContext::new();
    let arr = |xs: &[&str]| ctx.heap().alloc_array_typed(
        "string", xs.iter().map(|s| Value::Str((*s).into())).collect());
    let join = |sep: &str, a: Value| str_join(sep, &a);
    let s = |v: Value| match v { Value::Str(s) => s.to_string(), o => panic!("{:?}", o) };
    assert_eq!(join(";", arr(&["a", "b", "c"])).unwrap(), "a;b;c");
    assert_eq!(join(", ", arr(&["x", "", "y"])).unwrap(), "x, , y");
    assert_eq!(join("-", arr(&["solo"])).unwrap(), "solo");
    assert_eq!(join("-", arr(&[])).unwrap(), "");
    let bad = ctx.heap().alloc_array_typed("string", vec![Value::Str("a".into()), Value::Null]);
    assert!(join(",", bad).is_err());
}

/// null 字符串接收者 → `NullReferenceException`（消息与 `VCall` 撞上 null 时同一条），不是内部错误；
/// null 实参同类。引擎的 builtin 错误出口按 `Throw` 构造异常（`corelib::builtin_error_exception`）。
#[test]
fn null_string_receiver_is_a_null_reference() {
    use crate::objops::Throw;
    let ctx = VmContext::new();
    let nre = |e: anyhow::Error| {
        let t = e.downcast::<Throw>().expect("typed throw");
        assert_eq!(t.class, "Std.NullReferenceException");
        t.msg
    };
    let typed_nre = |name: &str, args: &[Value]| match crate::corelib::exec_builtin_value(&ctx, name, args)
        .unwrap_err().downcast::<crate::corelib::abi::BuiltinError>() {
        Ok(crate::corelib::abi::BuiltinError::NullReference(m)) => m,
        other => panic!("expected a NullReference, got {other:?}"),
    };
    assert_eq!(typed_nre("__str_length", &[Value::Null]), "cannot read property `Length` of a null reference");
    assert_eq!(typed_nre("__str_char_at", &[Value::Null, Value::I64(0)]), "cannot call method `CharAt` on a null reference");
    assert_eq!(typed_nre("__str_to_chars", &[Value::Null]),
        "cannot call method `ToCharArray` on a null reference");
    assert_eq!(typed_nre("__str_substring", &[Value::Null, Value::I64(0), Value::I64(0)]),
        "cannot call method `Substring` on a null reference");
    // `CompareTo(null)` is 1 (.NET: any string sorts after null); a null receiver throws.
    assert_eq!(crate::corelib::exec_builtin_value(&ctx, "__str_compare_to", &[Value::Str("a".into()), Value::Null]).unwrap(),
        Value::I64(1));
    assert!(crate::corelib::exec_builtin_value(&ctx, "__str_compare_to", &[Value::Null, Value::Str("a".into())])
        .unwrap_err().downcast_ref::<crate::corelib::abi::BuiltinError>()
        .is_some_and(|e| matches!(e, crate::corelib::abi::BuiltinError::NullReference(m) if m == "cannot call method `CompareTo` on a null reference")));
    // `Equals` 的实参 null 是合法输入（false），只有接收者 null 才抛。
    assert_eq!(crate::corelib::exec_builtin_value(&ctx, "__str_equals", &[Value::Str("a".into()), Value::Null]).unwrap(), Value::Bool(false));
    assert_eq!(typed_nre("__str_equals", &[Value::Null, Value::Str("a".into())]),
        "cannot call method `Equals` on a null reference");
}

/// `__str_split` 的接收者同样报 `null_call`（不是「第 0 个实参」）。
#[test]
fn null_split_receiver_names_the_method() {
    let ctx = VmContext::new();
    let e = crate::corelib::exec_builtin_value(&ctx, "__str_split", &[Value::Null, Value::Str(",".into())])
        .unwrap_err().downcast::<crate::corelib::abi::BuiltinError>().ok();
    assert!(matches!(e, Some(crate::corelib::abi::BuiltinError::NullReference(m)) if m == "cannot call method `Split` on a null reference"));
}
