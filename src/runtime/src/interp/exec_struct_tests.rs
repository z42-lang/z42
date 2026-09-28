//! Unit tests for the blob value-type primitive codec + byte-level value semantics
//! (add-struct-value-semantics Phase A).
use super::*;
use crate::interp::struct_arena::StructArena;
use crate::metadata::types::StructTypeLayout;
use std::sync::Arc;

/// Pure-primitive layout of `size` bytes (no reference leaves).
fn prim_layout(size: usize) -> Arc<StructTypeLayout> {
    Arc::new(StructTypeLayout { size, ref_offsets: Box::new([]), ref_kinds: Box::new([]), fields: Box::new([]) })
}

fn enc(bytes: &mut [u8], off: usize, tag: u8, v: Value) {
    let w = prim_width(tag).unwrap();
    encode_prim(bytes, off, w, tag, &v).unwrap();
}
fn dec(bytes: &[u8], off: usize, tag: u8) -> Value {
    let w = prim_width(tag).unwrap();
    decode_prim(bytes, off, w, tag).unwrap()
}

// ── symbolic-struct-field-access P2 之后的转换说明 ─────────────────────────────
//
// 本文件的测试考的是**各 base kind 的字节访问机制**（arena `StructRef` / 堆对象内联字段 /
// `StackObject` / `BoxedStruct` / `StructRefHeap` 数组元素）与叶子 codec —— **不是**路径解析。
// 它们用的是裸 `VmContext::new()`（**没有类型注册表**）+ 本地构造的 `TypeDesc`，从不按名字注册，
// 所以符号化入口（`struct_field_{get,set}_prim`）在这里必然以「type not loaded」失败。
//
// ⇒ 改为直接驱动 `*_val` 内核（它仍收字节偏移），**测的东西一个字没变**。
// 路径解析另有专门的测试（见本文件末尾 `resolve_field_path` 那组），那里会真注册类型。
fn set_leaf(ctx: &VmContext, frame: &mut Frame, base: u32, off: u32, kind: u8, val: u32) {
    let b = frame.get(base).unwrap().clone();
    let v = frame.get(val).unwrap().clone();
    super::struct_field_set_val(ctx, &b, off, kind, &v).unwrap();
}

fn get_leaf(ctx: &VmContext, frame: &mut Frame, dst: u32, base: u32, off: u32, kind: u8) {
    let b = frame.get(base).unwrap().clone();
    let v = super::struct_field_get_val(ctx, &b, off, kind).unwrap();
    frame.set(dst, v);
}

fn get_leaf_err(ctx: &VmContext, frame: &mut Frame, base: u32, off: u32, kind: u8) -> String {
    let b = frame.get(base).unwrap().clone();
    super::struct_field_get_val(ctx, &b, off, kind).unwrap_err().to_string()
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

    // unify-object-byte-layout (PR-2): the object stores the inline struct's leaves in
    // its `bytes`/`refs` via the composed object layout (`ref_index` maps a leaf's
    // object-relative offset to a `refs` slot). Point{x:i32@0, y:i32@4, tag:str@8},
    // size 12, one reference leaf at offset 8.
    let composed = Arc::new(crate::metadata::types::ObjectLayout {
        size: 12,
        field_offsets: Box::new([]),
        field_sizes: Box::new([]),
        field_kinds: Box::new([]),
        ref_offsets: Box::new([8]),
        ref_kinds: Box::new([crate::metadata::types::STRUCT_REF_ARC_STRING]),
        inline_refs: Box::new([]), // string leaf stays in the side-table (not inlined)
        field_access: Box::new([]),
    });
    let td = Arc::new(TypeDesc {
        class_flags: 0,
        visibility: 0,
        name: "C".to_string(),
        base_name: None,
        fields: Vec::new(),
        field_index: NameIndex::new(),
        vtable: Vec::new(),
        vtable_index: NameIndex::new(),
        cold: Some(Box::new(TypeDescCold { composed_object_layout: Some(composed), ..Default::default() })),
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
    set_leaf(&ctx, &mut frame, 0, 0, ty::TAG_I32, 1);
    set_leaf(&ctx, &mut frame, 0, 4, ty::TAG_I32, 2);
    set_leaf(&ctx, &mut frame, 0, 8, ty::TAG_STR, 3);

    // Read them back into reg4/5/6.
    get_leaf(&ctx, &mut frame, 4, 0, 0, ty::TAG_I32);
    get_leaf(&ctx, &mut frame, 5, 0, 4, ty::TAG_I32);
    get_leaf(&ctx, &mut frame, 6, 0, 8, ty::TAG_STR);

    assert!(matches!(frame.get(4).unwrap(), Value::I64(42)), "pt.x must be 42");
    assert!(matches!(frame.get(5).unwrap(), Value::I64(7)),  "pt.y must be 7");
    match frame.get(6).unwrap() {
        Value::Str(s) => assert_eq!(&**s, "hi", "inline ref leaf `tag` must round-trip"),
        o => panic!("expected the string ref leaf, got {o:?}"),
    }

    // Overwriting pt.x must not disturb pt.y or the ref leaf (independent byte slots).
    frame.set(7, Value::I64(99));
    set_leaf(&ctx, &mut frame, 0, 0, ty::TAG_I32, 7);
    get_leaf(&ctx, &mut frame, 5, 0, 4, ty::TAG_I32);
    assert!(matches!(frame.get(5).unwrap(), Value::I64(7)), "pt.y must stay 7 after pt.x rewrite");
}

/// add-struct-heap-inline (P3b, D1-a, route α): a `Point[]` **element** leaf is read/
/// written through `struct_field_get_prim`/`set_prim` with a `Value::StructRefHeap`
/// handle (`arr[index]`). Exercises the array-backing branch of the interp handlers.
/// Element layout {x:i32@0, y:i32@4, tag:string@8}, size 12, one ref leaf.
#[test]
fn struct_array_element_leaf_access_via_handle() {
    use crate::vm_context::VmContext;
    use crate::metadata::types::{ArrayObj, StructArrayElem, STRUCT_REF_ARC_STRING};
    use crate::gc::GcRef;

    let layout = Arc::new(StructTypeLayout {
        size: 12,
        ref_offsets: Box::new([8]),
        ref_kinds: Box::new([STRUCT_REF_ARC_STRING]),
        fields: Box::new([]),
    });
    // unify-gc-heap PR-3: struct[] byte + ref storage lives in leaked GC blocks (heap-less test).
    let arr_gc = GcRef::new(ArrayObj::struct_backed_leaked("Demo.P", 2, layout));

    let ctx = VmContext::new();
    let mut frame = Frame::new(&[], 8);
    // make-value-copy: StructRefHeap payload lives in the per-context transient arena; the
    // register holds an 8B handle. Alloc both element handles into `ctx`'s arena (frame_id 7).
    let mk_sr = |idx: u32| {
        let hidx = ctx.transient_alloc(
            7,
            crate::interp::transient_arena::TransientPayload::StructElem(
                StructArrayElem { arr: arr_gc, index: idx },
            ),
        );
        Value::StructRefHeap { idx: hidx, frame_id: 7 }
    };
    // reg0 = handle to arr[0], reg1 = handle to arr[1]
    frame.set(0, mk_sr(0));
    frame.set(1, mk_sr(1));
    frame.set(2, Value::I64(11));
    frame.set(3, Value::Str("zero".into()));
    frame.set(4, Value::I64(22));

    // arr[0].x = 11, arr[0].tag = "zero"; arr[1].x = 22
    set_leaf(&ctx, &mut frame, 0, 0, ty::TAG_I32, 2);
    set_leaf(&ctx, &mut frame, 0, 8, ty::TAG_STR, 3);
    set_leaf(&ctx, &mut frame, 1, 0, ty::TAG_I32, 4);

    // Read back: arr[0].x == 11, arr[0].tag == "zero", arr[1].x == 22 (independent elements).
    get_leaf(&ctx, &mut frame, 5, 0, 0, ty::TAG_I32);
    get_leaf(&ctx, &mut frame, 6, 0, 8, ty::TAG_STR);
    get_leaf(&ctx, &mut frame, 7, 1, 0, ty::TAG_I32);

    assert!(matches!(frame.get(5).unwrap(), Value::I64(11)), "arr[0].x must be 11");
    match frame.get(6).unwrap() {
        Value::Str(s) => assert_eq!(&**s, "zero"),
        o => panic!("arr[0].tag expected string, got {o:?}"),
    }
    assert!(matches!(frame.get(7).unwrap(), Value::I64(22)), "arr[1].x must be 22, independent of arr[0]");
}

/// add-struct-object-boxing (PR2a) / add-boxed-struct-identity (P4b): 装箱把 blob **快照拷出** arena
/// slot → 脱离 arena 生命周期。P4b 后快照落在共享 `ScriptObject`（heap）而非 owned `BoxedStructData`，
/// 但「装箱时从 arena slot 拷出的快照独立于 arena」这条不变量不变——`builtin_box_struct` 先从 slot 抽
/// `(type_name, bytes, refs)`（本测试验的这步），再 `box_struct_blob` 拷进堆对象。arena truncate（模拟
/// 创建帧退出）后原 slot 失效，但快照仍持有数据（修 `object o = struct` use-after-free 的健全性性质）。
#[test]
fn boxed_struct_owns_snapshot_and_survives_arena_truncate() {
    let mut arena = StructArena::default();
    let ty: Arc<str> = Arc::from("Demo.P");
    let base = arena.base();
    let a = arena.alloc(1, ty.clone(), prim_layout(8));
    arena.with_mut(a, 1, |s| { enc(&mut s.bytes, 0, ty::TAG_I32, Value::I64(1));
                               enc(&mut s.bytes, 4, ty::TAG_I32, Value::I64(2)); }).unwrap();
    // box 第一步：从 slot 快照 bytes+refs+类型名（builtin_box_struct 抽取的等价逻辑，随后喂 box_struct_blob）。
    let (snap_ty, snap_bytes, _snap_refs): (Arc<str>, Vec<u8>, Vec<Value>) =
        arena.with(a, 1, |s| (s.type_name.clone(), s.bytes.to_vec(), s.refs.to_vec())).unwrap();
    // 创建帧退出 → arena LIFO 截断；原 StructRef 句柄此刻应 stale。
    arena.truncate(base);
    assert!(arena.with(a, 1, |_| ()).is_err(), "arena slot must be stale after truncate");
    // 快照仍持有数据（owned，无悬垂）——box_struct_blob 会把它拷进共享 ScriptObject。
    assert_eq!(&*snap_ty, "Demo.P");
    assert!(matches!(dec(&snap_bytes, 0, ty::TAG_I32), Value::I64(1)));
    assert!(matches!(dec(&snap_bytes, 4, ty::TAG_I32), Value::I64(2)));
}

/// fix-stackobj-inline-struct-leaf: the same inline-struct-field round-trip as
/// `heap_object_inline_struct_field_roundtrips`, but the object lives in the **stack
/// arena** (`Value::StackObject`) because escape analysis stack-allocated it.
///
/// 🔴 Before the fix both handlers had a `Value::Object` arm and **no** `StackObject`
/// arm, so the base fell through to `as_struct_ref` and bailed
/// `StructFieldSetPrim base: expected a struct value (StructRef), got StackObject`.
/// A 12-line program — one class with one struct field — built with
/// `z42c build --release` crashed at run time under `--mode interp`; JIT was fine,
/// which is why no jit lane caught it. This test is the mode-independent guard.
#[test]
fn stack_object_inline_struct_field_roundtrips() {
    use crate::vm_context::VmContext;
    use crate::metadata::types::{TypeDesc, TypeDescCold, ScriptObject};
    use crate::metadata::NameIndex;
    use crate::metadata::tokens::TypeId;

    // Same shape as the heap test: Point{x:i32@0, y:i32@4, tag:str@8}, size 12,
    // one reference leaf at offset 8.
    let composed = Arc::new(crate::metadata::types::ObjectLayout {
        size: 12,
        field_offsets: Box::new([]),
        field_sizes: Box::new([]),
        field_kinds: Box::new([]),
        ref_offsets: Box::new([8]),
        ref_kinds: Box::new([crate::metadata::types::STRUCT_REF_ARC_STRING]),
        inline_refs: Box::new([]),
        field_access: Box::new([]),
    });
    let td = Arc::new(TypeDesc {
        class_flags: 0,
        visibility: 0,
        name: "C".to_string(),
        base_name: None,
        fields: Vec::new(),
        field_index: NameIndex::new(),
        vtable: Vec::new(),
        vtable_index: NameIndex::new(),
        cold: Some(Box::new(TypeDescCold { composed_object_layout: Some(composed), ..Default::default() })),
        id: TypeId::UNRESOLVED,
    });

    let ctx = VmContext::new();
    let mut frame = Frame::new(&[], 8);
    let storage = td.object_storage();
    let obj = ScriptObject::new(td, storage);
    let idx = ctx.stack_alloc_obj(frame.frame_id, obj);
    let base = Value::StackObject { idx, frame_id: frame.frame_id };

    frame.set(0, base);
    frame.set(1, Value::I64(42));
    frame.set(2, Value::I64(7));
    frame.set(3, Value::Str("hi".into()));

    // Writes: two prim leaves + the ref leaf (the ref arm is the risky half — a
    // stack object takes **no** write barrier, unlike the heap arm).
    set_leaf(&ctx, &mut frame, 0, 0, ty::TAG_I32, 1);
    set_leaf(&ctx, &mut frame, 0, 4, ty::TAG_I32, 2);
    set_leaf(&ctx, &mut frame, 0, 8, ty::TAG_STR, 3);

    get_leaf(&ctx, &mut frame, 4, 0, 0, ty::TAG_I32);
    get_leaf(&ctx, &mut frame, 5, 0, 4, ty::TAG_I32);
    get_leaf(&ctx, &mut frame, 6, 0, 8, ty::TAG_STR);

    assert!(matches!(frame.get(4).unwrap(), Value::I64(42)), "pt.x must be 42 on a stack object");
    assert!(matches!(frame.get(5).unwrap(), Value::I64(7)),  "pt.y must be 7 on a stack object");
    match frame.get(6).unwrap() {
        Value::Str(s) => assert_eq!(&**s, "hi", "inline ref leaf must round-trip on a stack object"),
        o => panic!("expected the string ref leaf, got {o:?}"),
    }

    // Independent byte slots: rewriting x disturbs neither y nor the ref leaf.
    frame.set(7, Value::I64(99));
    set_leaf(&ctx, &mut frame, 0, 0, ty::TAG_I32, 7);
    get_leaf(&ctx, &mut frame, 5, 0, 4, ty::TAG_I32);
    get_leaf(&ctx, &mut frame, 6, 0, 8, ty::TAG_STR);
    assert!(matches!(frame.get(5).unwrap(), Value::I64(7)), "pt.y must stay 7 after pt.x rewrite");
    match frame.get(6).unwrap() {
        Value::Str(s) => assert_eq!(&**s, "hi", "ref leaf must survive a prim-leaf rewrite"),
        o => panic!("expected the string ref leaf, got {o:?}"),
    }
}

// ── symbolic-struct-field-access P2 (T12)：纯隔离的符号化开销 ────────────────
//
// 提案承诺过：P2 落地后必须实测**纯隔离**的开销 —— 先前只有一个「含 arena↔堆存储差」
// 的上界。隔离的含义是只量 `resolve_field_path` 本身，不量它周围那次 helper 调用、
// 不量字节 codec、不量寄存器读写 —— 那些**符号化前后完全一样**。
//
// ⚠️ 这个数**不等于**「P2 的代价」，两条限定必须一起引用：
//   ① struct 字段访问在 JIT 里**恒是 helper 调用**（烘焙偏移只是个 iconst 实参）
//      ⇒ P2 的**边际**代价 = 那次本来就要付的调用里多出的这一段。
//   ② 本测量给不出**动态**权重。静态发射占比是深度 1 = 85.1%（全 e2e 语料 800 次），
//      但一个深度 3 的热循环能压倒 2% 的站点占比。那个数要真实负载 A/B（两个工具链）。
//
// `#[ignore]`：这是**一次性的「符号化值不值」**测量，不是持续护栏，不该进常规 CI。
//   跑法：cargo test --release --lib symbolization_cost -- --ignored --nocapture
#[test]
#[ignore]
fn symbolization_cost_by_path_depth() {
    use crate::metadata::name_index::NameIndex;
    use crate::metadata::tokens::TypeId;
    use crate::metadata::types::{FieldSlot, StructFieldLayout, StructTypeLayout, TypeDesc, TypeDescCold};
    use std::sync::Arc;
    use std::time::Instant;

    fn struct_desc(name: &str, n: usize, nest: Option<&str>) -> Arc<TypeDesc> {
        Arc::new(TypeDesc {
            name: name.into(),
            class_flags: crate::metadata::bytecode::CLASS_FLAG_STRUCT,
            fields: (0..n).map(|i| FieldSlot {
                name: format!("f{i}").into(),
                type_tag: if i == 0 { nest.unwrap_or("long").into() } else { "long".into() },
                visibility: 0,
            }).collect(),
            field_index: NameIndex::new(),
            vtable: Vec::new(),
            vtable_index: NameIndex::new(),
            base_name: None,
            visibility: 0,
            cold: Some(Box::new(TypeDescCold {
                struct_layout: Some(Arc::new(StructTypeLayout {
                    size: n * 8,
                    ref_offsets: Box::new([]),
                    ref_kinds: Box::new([]),
                    fields: (0..n).map(|i| StructFieldLayout {
                        offset: (i * 8) as u32, size: 8, kind: 0,
                    }).collect(),
                })),
                ..Default::default()
            })),
            id: TypeId::UNRESOLVED,
        })
    }

    const ITERS: u32 = 200_000;
    println!("\n路径解析的纯隔离开销（{ITERS} 次/档，release 跑才有意义）");
    println!("{:<12} {:>12} {:>14}", "深度", "总耗时", "每次");

    for depth in 1..=4usize {
        let vm = VmContext::new();
        vm.install_lazy_loader(None, 0);
        if let Some(l) = vm.core.lazy_loader.write().as_mut() {
            for i in 0..depth {
                let nest = if i + 1 < depth { Some(format!("Bench.L{}", i + 1)) } else { None };
                l.insert_type(format!("Bench.L{i}"), struct_desc(&format!("Bench.L{i}"), 4, nest.as_deref()));
            }
        }
        let path: Vec<u16> = vec![0u16; depth];
        // 预热（首次会触发 lazy 查找的慢路）。
        for _ in 0..1000 { let _ = super::resolve_field_path(&vm, "Bench.L0", &path); }

        let t0 = Instant::now();
        let mut acc = 0u64;
        for _ in 0..ITERS {
            acc += super::resolve_field_path(
                std::hint::black_box(&vm),
                std::hint::black_box("Bench.L0"),
                std::hint::black_box(&path),
            ).unwrap() as u64;
        }
        let el = t0.elapsed();
        std::hint::black_box(acc);
        println!("{:<12} {:>12?} {:>13.1}ns", depth, el, el.as_nanos() as f64 / ITERS as f64);
    }
}
