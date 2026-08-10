//! Unit tests for the blob value-type primitive codec + byte-level value semantics
//! (add-struct-value-semantics Phase A).
use super::*;
use crate::interp::struct_arena::StructArena;
use crate::metadata::types::StructTypeLayout;
use std::sync::Arc;

/// Pure-primitive layout of `size` bytes (no reference leaves).
fn prim_layout(size: usize) -> Arc<StructTypeLayout> {
    Arc::new(StructTypeLayout { size, ref_offsets: Box::new([]), ref_kinds: Box::new([]) })
}

fn enc(bytes: &mut [u8], off: usize, tag: u8, v: Value) {
    let w = prim_width(tag).unwrap();
    encode_prim(bytes, off, w, tag, &v).unwrap();
}
fn dec(bytes: &[u8], off: usize, tag: u8) -> Value {
    let w = prim_width(tag).unwrap();
    decode_prim(bytes, off, w, tag).unwrap()
}

#[test]
fn codec_roundtrip_integers() {
    for &(tag, n) in &[
        (ty::TAG_I8, -5i64), (ty::TAG_U8, 200), (ty::TAG_I16, -300),
        (ty::TAG_I32, -70000), (ty::TAG_U32, 4_000_000_000i64),
        (ty::TAG_I64, i64::MIN + 1),
    ] {
        let mut b = [0u8; 8];
        enc(&mut b, 0, tag, Value::I64(n));
        match dec(&b, 0, tag) {
            Value::I64(got) => assert_eq!(got, n, "tag {tag:#x}"),
            o => panic!("tag {tag:#x}: expected I64, got {o:?}"),
        }
    }
}

#[test]
fn codec_roundtrip_bool_char_float() {
    let mut b = [0u8; 8];
    enc(&mut b, 0, ty::TAG_BOOL, Value::Bool(true));
    assert!(matches!(dec(&b, 0, ty::TAG_BOOL), Value::Bool(true)));
    enc(&mut b, 0, ty::TAG_CHAR, Value::Char('世'));
    assert!(matches!(dec(&b, 0, ty::TAG_CHAR), Value::Char('世')));
    enc(&mut b, 0, ty::TAG_F64, Value::F64(3.5));
    assert!(matches!(dec(&b, 0, ty::TAG_F64), Value::F64(f) if f == 3.5));
    enc(&mut b, 0, ty::TAG_F32, Value::F64(1.25));
    assert!(matches!(dec(&b, 0, ty::TAG_F32), Value::F64(f) if f == 1.25));
}

#[test]
fn out_of_bounds_access_errors() {
    let b = [0u8; 4];
    assert!(decode_prim(&b, 2, 4, ty::TAG_I32).is_err(), "read past blob end must error");
    let mut m = [0u8; 4];
    assert!(encode_prim(&mut m, 2, 4, ty::TAG_I32, &Value::I64(0)).is_err());
}

/// The core value-semantics property, exercised at the arena+codec layer that the
/// interp handlers use: `var b = a; b.x = 99;` must leave `a.x` unchanged.
/// struct P { x: int @0; y: int @4; }  (size 8)
#[test]
fn value_semantics_copy_then_mutate_leaves_source_unchanged() {
    let mut arena = StructArena::default();
    let ty: Arc<str> = Arc::from("P");
    let a = arena.alloc(1, ty.clone(), prim_layout(8));
    let b = arena.alloc(1, ty, prim_layout(8));

    // a.x = 1; a.y = 7
    arena.with_mut(a, 1, |s| { enc(&mut s.bytes, 0, ty::TAG_I32, Value::I64(1));
                               enc(&mut s.bytes, 4, ty::TAG_I32, Value::I64(7)); }).unwrap();
    // b = a   (StructCopy)
    arena.copy_into(b, 1, a, 1, 8).unwrap();
    // b.x = 99
    arena.with_mut(b, 1, |s| enc(&mut s.bytes, 0, ty::TAG_I32, Value::I64(99))).unwrap();

    // a.x still 1, a.y still 7 — the copy is independent.
    let ax = arena.with(a, 1, |s| dec(&s.bytes, 0, ty::TAG_I32)).unwrap();
    let ay = arena.with(a, 1, |s| dec(&s.bytes, 4, ty::TAG_I32)).unwrap();
    let bx = arena.with(b, 1, |s| dec(&s.bytes, 0, ty::TAG_I32)).unwrap();
    assert!(matches!(ax, Value::I64(1)), "a.x must stay 1, got {ax:?}");
    assert!(matches!(ay, Value::I64(7)), "a.y must stay 7, got {ay:?}");
    assert!(matches!(bx, Value::I64(99)), "b.x must be 99, got {bx:?}");
}
