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

/// add-struct-heap-inline (P3b, D1-a, route α): a struct field **inlined into a heap
/// object** is read/written through `struct_field_get_prim`/`set_prim` with an
/// `Value::Object` base — primitives land in `ScriptObject::struct_bytes`, reference
/// leaves in `struct_refs`. Exercises the full interp handlers end-to-end.
/// `class C { Point pt; string tag; }` → composite inline region: pt.x@0 / pt.y@4
/// (prims) + tag@8 (string ref leaf); size 12, one ref leaf at offset 8.
#[test]
fn heap_object_inline_struct_field_roundtrips() {
    use crate::vm_context::VmContext;
    use crate::metadata::types::{TypeDesc, TypeDescCold, NativeData};
    use crate::metadata::NameIndex;
    use crate::metadata::tokens::TypeId;

    let inline = Arc::new(StructTypeLayout {
        size: 12,
        ref_offsets: Box::new([8]),
        ref_kinds: Box::new([crate::metadata::types::STRUCT_REF_ARC_STRING]),
    });
    let td = Arc::new(TypeDesc {
        class_flags: 0,
        name: "C".to_string(),
        base_name: None,
        fields: Vec::new(),
        field_index: NameIndex::new(),
        vtable: Vec::new(),
        vtable_index: NameIndex::new(),
        cold: Some(Box::new(TypeDescCold { inline_layout: Some(inline), ..Default::default() })),
        id: TypeId::UNRESOLVED,
    });

    let ctx = VmContext::new();
    let obj = ctx.heap().alloc_object(td, Vec::new(), NativeData::None);
    assert!(matches!(obj, Value::Object(_)), "alloc_object must yield a heap object");

    let mut frame = Frame::new(&[], 8);
    frame.set(0, obj);                 // reg0 = the object (base)
    frame.set(1, Value::I64(42));      // reg1 = value to store into pt.x
    frame.set(2, Value::I64(7));       // reg2 = value to store into pt.y
    frame.set(3, Value::Str("hi".into())); // reg3 = string for the ref leaf `tag`

    // pt.x = 42 (off 0), pt.y = 7 (off 4), tag = "hi" (ref leaf off 8)
    struct_field_set_prim(&ctx, &mut frame, 0, 0, ty::TAG_I32, 1).unwrap();
    struct_field_set_prim(&ctx, &mut frame, 0, 4, ty::TAG_I32, 2).unwrap();
    struct_field_set_prim(&ctx, &mut frame, 0, 8, ty::TAG_STR, 3).unwrap();

    // Read them back into reg4/5/6.
    struct_field_get_prim(&ctx, &mut frame, 4, 0, 0, ty::TAG_I32).unwrap();
    struct_field_get_prim(&ctx, &mut frame, 5, 0, 4, ty::TAG_I32).unwrap();
    struct_field_get_prim(&ctx, &mut frame, 6, 0, 8, ty::TAG_STR).unwrap();

    assert!(matches!(frame.get(4).unwrap(), Value::I64(42)), "pt.x must be 42");
    assert!(matches!(frame.get(5).unwrap(), Value::I64(7)),  "pt.y must be 7");
    match frame.get(6).unwrap() {
        Value::Str(s) => assert_eq!(&**s, "hi", "inline ref leaf `tag` must round-trip"),
        o => panic!("expected the string ref leaf, got {o:?}"),
    }

    // Overwriting pt.x must not disturb pt.y or the ref leaf (independent byte slots).
    frame.set(7, Value::I64(99));
    struct_field_set_prim(&ctx, &mut frame, 0, 0, ty::TAG_I32, 7).unwrap();
    struct_field_get_prim(&ctx, &mut frame, 5, 0, 4, ty::TAG_I32).unwrap();
    assert!(matches!(frame.get(5).unwrap(), Value::I64(7)), "pt.y must stay 7 after pt.x rewrite");
}

/// add-struct-object-boxing (PR2a): 装箱把 blob **拷进 owned `BoxedStructData`** → 脱离 arena 生命周期。
/// arena truncate（模拟创建帧退出）后原 `StructRef` slot 失效，但 boxed 副本仍持有数据（这正是修
/// `object o = struct` 裸拷帧作用域 StructRef → use-after-free 的健全性性质）。同时验装箱是快照。
#[test]
fn boxed_struct_owns_snapshot_and_survives_arena_truncate() {
    use crate::metadata::types::BoxedStructData;
    let mut arena = StructArena::default();
    let ty: Arc<str> = Arc::from("Demo.P");
    let base = arena.base();
    let a = arena.alloc(1, ty.clone(), prim_layout(8));
    arena.with_mut(a, 1, |s| { enc(&mut s.bytes, 0, ty::TAG_I32, Value::I64(1));
                               enc(&mut s.bytes, 4, ty::TAG_I32, Value::I64(2)); }).unwrap();
    // box：把 slot 的 bytes+refs+类型名快照进 owned 堆值（builtin_box_struct 的等价逻辑）。
    let boxed = arena.with(a, 1, |s| BoxedStructData {
        type_name: s.type_name.clone(),
        bytes: s.bytes.clone(),
        refs: s.refs.clone(),
    }).unwrap();
    // 创建帧退出 → arena LIFO 截断；原 StructRef 句柄此刻应 stale。
    arena.truncate(base);
    assert!(arena.with(a, 1, |_| ()).is_err(), "arena slot must be stale after truncate");
    // boxed 副本仍持有数据（owned，无悬垂）。
    assert_eq!(&*boxed.type_name, "Demo.P");
    assert!(matches!(dec(&boxed.bytes, 0, ty::TAG_I32), Value::I64(1)));
    assert!(matches!(dec(&boxed.bytes, 4, ty::TAG_I32), Value::I64(2)));
}
