use super::*;
use crate::metadata::Value;

// __str_to_chars: bulk materialise the whole char[] in one native call — the
// array-view primitive backing script-side string ops (String.ToCharArray →
// IndexOf/…). Must yield Unicode scalars (chars), not bytes.
#[test]
fn to_chars_yields_scalars() {
    let ctx = VmContext::new();
    let out = builtin_str_to_chars(&ctx, &[Value::Str("héllo".into())]).unwrap();
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
    let out = builtin_str_to_chars(&ctx, &[Value::Str("".into())]).unwrap();
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
    let split = |s: &str, sep: &str| strings_of(
        builtin_str_split(&ctx, &[Value::Str(s.into()), Value::Str(sep.into())]).unwrap());
    assert_eq!(split("a,b,,c", ","), vec!["a", "b", "", "c"]);
    assert_eq!(split(",a,", ","), vec!["", "a", ""]);
    assert_eq!(split("abc", ","), vec!["abc"]);
    assert_eq!(split("", ","), vec![""]);
    assert_eq!(split("aaa", "aa"), vec!["", "a"]);
    assert_eq!(split("x->y->z", "->"), vec!["x", "y", "z"]);
    assert_eq!(split("你,好,世界", ","), vec!["你", "好", "世界"]);
    assert!(builtin_str_split(&ctx, &[Value::Str("a".into()), Value::Str("".into())]).is_err());
}

// __str_join: interleave with the separator, one allocation; non-string elements rejected.
#[test]
fn join_interleaves_and_rejects_non_strings() {
    let ctx = VmContext::new();
    let arr = |xs: &[&str]| ctx.heap().alloc_array_typed(
        "string", xs.iter().map(|s| Value::Str((*s).into())).collect());
    let join = |sep: &str, a: Value| builtin_str_join(&ctx, &[Value::Str(sep.into()), a]);
    let s = |v: Value| match v { Value::Str(s) => s.to_string(), o => panic!("{:?}", o) };
    assert_eq!(s(join(";", arr(&["a", "b", "c"])).unwrap()), "a;b;c");
    assert_eq!(s(join(", ", arr(&["x", "", "y"])).unwrap()), "x, , y");
    assert_eq!(s(join("-", arr(&["solo"])).unwrap()), "solo");
    assert_eq!(s(join("-", arr(&[])).unwrap()), "");
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
    assert_eq!(nre(builtin_str_length(&ctx, &[Value::Null]).unwrap_err()),
        "cannot read property `Length` of a null reference");
    assert_eq!(nre(builtin_str_char_at(&ctx, &[Value::Null, Value::I64(0)]).unwrap_err()),
        "cannot call method `CharAt` on a null reference");
    assert_eq!(nre(builtin_str_to_chars(&ctx, &[Value::Null]).unwrap_err()),
        "cannot call method `ToCharArray` on a null reference");
    assert_eq!(nre(builtin_str_substring(&ctx, &[Value::Null, Value::I64(0), Value::I64(0)]).unwrap_err()),
        "cannot call method `Substring` on a null reference");
    assert_eq!(nre(super::super::convert::builtin_str_compare_to(&ctx, &[Value::Str("a".into()), Value::Null]).unwrap_err()),
        "cannot pass null as argument 1 of `String.CompareTo`");
    // `Equals` 的实参 null 是合法输入（false），只有接收者 null 才抛。
    assert_eq!(builtin_str_equals(&ctx, &[Value::Str("a".into()), Value::Null]).unwrap(), Value::Bool(false));
    assert_eq!(nre(builtin_str_equals(&ctx, &[Value::Null, Value::Str("a".into())]).unwrap_err()),
        "cannot call method `Equals` on a null reference");
}

/// `__str_split` 的接收者同样报 `null_call`（不是「第 0 个实参」）。
#[test]
fn null_split_receiver_names_the_method() {
    let ctx = VmContext::new();
    let e = builtin_str_split(&ctx, &[Value::Null, Value::Str(",".into())]).unwrap_err();
    assert_eq!(e.to_string(), "Std.NullReferenceException: cannot call method `Split` on a null reference");
}
