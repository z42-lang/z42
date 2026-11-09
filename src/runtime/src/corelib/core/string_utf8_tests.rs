use super::*;
use crate::object::{ArrayObj, ElemType, Value};
use crate::vm::exec::call::builtins::{exec_builtin, exec_builtin_value};

fn bytes(ctx: &VmContext, data: &[u8]) -> Value {
    ArrayObj::alloc_typed_iter(ctx.heap(), ElemType::intern("byte"), data.len(),
        data.iter().map(|b| Value::I64(*b as i64)))
}
fn decode(ctx: &VmContext, array: Value, offset: i64, count: i64) -> anyhow::Result<Value> {
    exec_builtin_value(ctx, "__str_from_utf8", &[array, Value::I64(offset), Value::I64(count)])
}
fn copy(ctx: &VmContext, source: Value, si: i64, destination: Value, di: i64, n: i64) -> anyhow::Result<()> {
    let result = exec_builtin(ctx, "__str_copy_utf8", &[source, Value::I64(si), destination, Value::I64(di), Value::I64(n)])?;
    assert!(result.is_none());
    Ok(())
}
fn snapshot(array: Value) -> Vec<u8> {
    let Value::Array(a) = array else { panic!("expected array") };
    a.borrow().byte_snapshot().unwrap()
}
#[test]
fn copy_uses_byte_offsets_and_preserves_guards() {
    let ctx = VmContext::new();
    let dst = bytes(&ctx, &[0x7f; 9]);
    copy(&ctx, Value::Str("A中😀Z".into()), 1, dst, 1, 7).unwrap();
    let mut expected = vec![0x7f]; expected.extend_from_slice("中😀".as_bytes()); expected.push(0x7f);
    assert_eq!(snapshot(dst), expected);
    let split = bytes(&ctx, &[0; 2]);
    copy(&ctx, Value::Str("中".into()), 1, split, 0, 2).unwrap();
    assert_eq!(snapshot(split), vec![0xb8, 0xad]);
    assert!(decode(&ctx, split, 0, 2).unwrap_err().to_string().contains("FormatException"));
}
#[test]
fn ranges_are_checked_before_writes_or_allocating_snapshots() {
    let ctx = VmContext::new();
    let dst = bytes(&ctx, &[9; 4]);
    for (si, di, n) in [(-1, 0, 1), (0, -1, 1), (0, 0, -1), (2, 0, 1), (0, 4, 1),
        (i64::MAX, 0, 1), (0, i64::MAX, 1), (0, 0, i64::MAX), (3, 0, 0)] {
        let e = copy(&ctx, Value::Str("ab".into()), si, dst, di, n).unwrap_err();
        assert!(e.to_string().contains("ArgumentOutOfRangeException"), "{e}");
        assert_eq!(snapshot(dst), vec![9; 4]);
    }
    copy(&ctx, Value::Str("ab".into()), 2, dst, 4, 0).unwrap();
    for (off, n) in [(-1, 0), (0, -1), (5, 0), (4, 1), (i64::MAX, 1), (1, i64::MAX)] {
        assert!(decode(&ctx, dst, off, n).unwrap_err().to_string().contains("ArgumentOutOfRangeException"));
    }
    assert!(matches!(decode(&ctx, dst, 4, 0).unwrap(), Value::Str(s) if s.as_ref() == ""));
}
#[test]
fn constructor_is_strict_for_every_invalid_utf8_family() {
    let ctx = VmContext::new();
    for invalid in [&[0x80][..], &[0xff], &[0xc0, 0x80], &[0xe0, 0x80, 0x80], &[0xf0, 0x80, 0x80, 0x80],
        &[0xed, 0xa0, 0x80], &[0xf4, 0x90, 0x80, 0x80], &[0xc2], &[0xe4, 0xb8], &[0xf0, 0x9f, 0x98], &[0xc2, 0x20]] {
        let e = decode(&ctx, bytes(&ctx, invalid), 0, invalid.len() as i64).unwrap_err();
        assert!(e.to_string().contains("FormatException"), "{e}");
    }
}
#[test]
fn constructor_uses_only_the_requested_snapshot_and_owns_the_result() {
    let ctx = VmContext::new();
    let mut data = vec![0xff]; data.extend_from_slice("Aé中😀\u{feff}\0".as_bytes()); data.push(0xff);
    let src = bytes(&ctx, &data);
    let out = decode(&ctx, src, 1, (data.len() - 2) as i64).unwrap();
    assert!(matches!(out, Value::Str(s) if s.as_ref() == "Aé中😀\u{feff}\0"));
    let Value::Array(a) = src else { panic!("expected array") };
    assert!(a.borrow().write_byte_range(1, b"x"));
    assert!(matches!(out, Value::Str(s) if s.as_ref() == "Aé中😀\u{feff}\0"));
}
#[test]
fn typed_abi_rejects_null_wrong_kinds_and_other_array_storage() {
    let ctx = VmContext::new();
    let src = bytes(&ctx, b"a");
    assert!(decode(&ctx, Value::Null, 0, 0).unwrap_err().to_string().contains("ArgumentNullException"));
    assert!(decode(&ctx, Value::I64(1), 0, 0).unwrap_err().to_string().contains("InvalidCastException"));
    assert!(copy(&ctx, Value::Null, 0, src, 0, 0).unwrap_err().to_string().contains("NullReferenceException"));
    assert!(copy(&ctx, Value::Str("a".into()), 0, Value::Null, 0, 0).unwrap_err().to_string().contains("ArgumentNullException"));
    let ints = ArrayObj::alloc_typed_filled(ctx.heap(), ElemType::intern("int"), 1, Value::I64(65));
    assert!(decode(&ctx, ints, 0, 1).unwrap_err().to_string().contains("InvalidCastException"));
    assert!(copy(&ctx, Value::Str("a".into()), 0, ints, 0, 1).unwrap_err().to_string().contains("InvalidCastException"));
    let Value::Array(a) = ints else { panic!("expected array") };
    assert!(matches!(a.borrow().get_boxed(0), Value::I64(65)));
}

#[test]
fn concat_parts_utf8_returns_packed_bytes_for_the_requested_prefix() {
    let ctx = VmContext::new();
    let parts = ctx.heap().alloc_array(vec![
        Value::Str("é".into()),
        Value::Str("中😀".into()),
        Value::Null,
    ]);
    let concat = |count| {
        exec_builtin_value(&ctx, "__str_concat_parts_utf8", &[parts, Value::I64(count)])
    };
    assert_eq!(snapshot(concat(2).unwrap()), "é中😀".as_bytes());
    assert!(snapshot(concat(0).unwrap()).is_empty());
    for count in [-1, 3, 4, i64::MAX] {
        assert!(concat(count).unwrap_err().to_string().contains("ArgumentException"));
    }
    let err = exec_builtin_value(&ctx, "__str_concat_parts_utf8", &[Value::Null, Value::I64(0)])
        .unwrap_err();
    assert!(err.to_string().contains("ArgumentNullException"));
}


#[test]
fn concat_parts_utf8_copies_many_fragments_into_independent_arrays() {
    let ctx = VmContext::new();
    let parts = ctx.heap().alloc_array(
        (0..64).map(|i| Value::Str(if i % 2 == 0 { "é".into() } else { "中😀".into() })).collect(),
    );
    let concat = || exec_builtin_value(&ctx, "__str_concat_parts_utf8", &[parts, Value::I64(64)]).unwrap();
    let first = concat();
    let second = concat();
    let expected = "é中😀".repeat(32);
    assert_eq!(snapshot(first), expected.as_bytes());
    assert_eq!(snapshot(second), expected.as_bytes());
    let (Value::Array(a), Value::Array(b)) = (first, second) else { panic!("expected packed arrays") };
    assert!(!crate::object::ArrRef::ptr_eq(&a, &b));
}

use crate::assembly::context::VmContextFactory as _;
