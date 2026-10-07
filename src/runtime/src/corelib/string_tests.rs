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
