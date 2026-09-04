/// Unit tests for `metadata::types`: per-tag default value derivation.
///
/// fix-array-default-init, 2026-05-18.

use super::types::*;

#[test]
fn default_for_bool_tag_is_false() {
    assert!(matches!(default_value_for_tag(TAG_BOOL), Value::Bool(false)));
}

#[test]
fn default_for_signed_int_tags_is_zero_i64() {
    for t in [TAG_I8, TAG_I16, TAG_I32, TAG_I64] {
        assert!(matches!(default_value_for_tag(t), Value::I64(0)), "tag {:#x}", t);
    }
}

#[test]
fn default_for_unsigned_int_tags_is_zero_i64() {
    for t in [TAG_U8, TAG_U16, TAG_U32, TAG_U64] {
        assert!(matches!(default_value_for_tag(t), Value::I64(0)), "tag {:#x}", t);
    }
}

#[test]
fn default_for_float_tags_is_zero_f64() {
    for t in [TAG_F32, TAG_F64] {
        match default_value_for_tag(t) {
            Value::F64(v) => assert_eq!(v, 0.0),
            other => panic!("tag {:#x}: expected F64(0.0), got {:?}", t, other),
        }
    }
}

#[test]
fn default_for_char_tag_is_null_char() {
    assert!(matches!(default_value_for_tag(TAG_CHAR), Value::Char('\0')));
}

#[test]
fn default_for_ref_tags_is_null() {
    for t in [TAG_STR, TAG_OBJECT, TAG_ARRAY, TAG_UNKNOWN, 0xFF] {
        assert!(matches!(default_value_for_tag(t), Value::Null), "tag {:#x}", t);
    }
}

// ── is_heap_ref (add-write-barriers, 2026-05-21) ────────────────────────────

use std::sync::Arc;
use crate::gc::GcRef;

fn dummy_type_desc(name: &str) -> Arc<TypeDesc> {
    Arc::new(TypeDesc {
        class_flags: 0,
        visibility: 0,
        name: name.to_string(),
        base_name: None,
        fields: Vec::new(),
        field_index: crate::metadata::NameIndex::new(),
        vtable: Vec::new(),
        vtable_index: crate::metadata::NameIndex::new(),
        cold: None,
        id: crate::metadata::tokens::TypeId::UNRESOLVED,
    })
}

#[test]
fn is_heap_ref_true_for_object() {
    let v = Value::Object(GcRef::new(ScriptObject::new(dummy_type_desc("Foo"), crate::metadata::types::ObjStorage::new(0, 0))));
    assert!(v.is_heap_ref());
}

#[test]
fn is_heap_ref_true_for_array() {
    let v = Value::Array(GcRef::new(crate::metadata::types::ArrayObj::new_leaked(vec![Value::I64(1), Value::I64(2)])));
    assert!(v.is_heap_ref());
}

#[test]
fn is_heap_ref_true_for_closure() {
    let v = Value::Closure(crate::gc::var_region::VarGcRef::leak_for_test(
        ClosureData {
            env:     GcRef::new(crate::metadata::types::ArrayObj::new_leaked(vec![Value::I64(42)])),
            fn_name: crate::metadata::vstr::Str::new_leaked("lambda$0"),
        },
        crate::gc::var_region::BlockType::Closure,
    ));
    assert!(v.is_heap_ref());
}

#[test]
fn is_heap_ref_false_for_ref_handle() {
    // make-value-copy: a `Ref` is now an 8B transient-arena handle (payload — incl. the
    // Array/Field target GcRef — lives in the arena, kept marked by its root scan). The
    // handle itself never escapes into a heap slot, so it is NOT a heap edge here (matches
    // `StructRef` / `StackObject`). Only the arena root scan keeps the target alive.
    let v = Value::Ref { idx: 0, frame_id: 1 };
    assert!(!v.is_heap_ref());
}

// ── add-struct-heap-inline (P3b, D1-a): inline struct fields' reference side-table
//    is GC-traversed (scaffolding is inert in production until the stage-3 format
//    wire populates it, so validate the traversal via manual construction). ──────

#[test]
fn trace_children_visits_inline_struct_refs() {
    // An object whose inline struct field holds a reference leaf (e.g. a nested
    // object) in `struct_refs`. `trace_children` must visit it so the leaf stays
    // marked — exactly the use-after-free P3b closes (`c.pt` holding a string/obj).
    let leaf = Value::Object(GcRef::new(ScriptObject::new(dummy_type_desc("Leaf"), crate::metadata::types::ObjStorage::new(0, 0))));
    let owner = Value::Object(GcRef::new(        // shrink-object-footprint P2: primitives (+ dead ref holes) in the byte
        // region, all reference leaves in the reference region — one block.
        // The inline-struct reference leaf lives among the refs.
ScriptObject::new(dummy_type_desc("Owner"), {
            let mut st = crate::metadata::types::ObjStorage::new(8, 1);
            st.refs_mut()[0] = leaf.clone();
            st
        })));

    let mut visited_object_children = 0usize;
    owner.trace_children(&mut |v: &Value| {
        if matches!(v, Value::Object(_)) { visited_object_children += 1; }
    });
    // Only the reference leaf in `refs` is an object; `bytes` holds no GcRefs.
    assert_eq!(visited_object_children, 1, "reference leaf in refs must be traced");
}

// ── unify-object-byte-layout (PR-3 chunk 2b): a DIRECT object field is byte-inlined as
//    an 8B tagged pointer in `bytes` (out of the side-table). Exercises the real runtime
//    read/write/GC path end-to-end (the compose tests only check the layout metadata). ──
#[test]
fn inline_object_field_roundtrips_and_is_traced() {
    use crate::metadata::types::{ObjectLayout, InlineRef, FieldAccess, TypeDescCold, TAG_OBJECT};

    // `class Holder { object child; }` — one direct object field inlined at byte offset 0.
    let layout = Arc::new(ObjectLayout {
        size: 8,
        field_offsets: Box::new([0]),
        field_sizes:   Box::new([8]),
        field_kinds:   Box::new([STRUCT_LEAF_GCREF]),
        ref_offsets:   Box::new([]),   // inlined → NOT in the side-table
        ref_kinds:     Box::new([]),
        inline_refs:   Box::new([InlineRef { offset: 0, is_array: false }]),
        field_access:  Box::new([FieldAccess { offset: 0, width: 8, tag: TAG_OBJECT, ref_slot: -1 }]),
    });
    let holder_td = Arc::new(TypeDesc {
        class_flags: 0,
        visibility: 0,
        name: "Holder".to_string(),
        base_name: None,
        fields: Vec::new(),
        field_index: crate::metadata::NameIndex::new(),
        vtable: Vec::new(),
        vtable_index: crate::metadata::NameIndex::new(),
        cold: Some(Box::new(TypeDescCold { composed_object_layout: Some(layout), ..Default::default() })),
        id: crate::metadata::tokens::TypeId::UNRESOLVED,
    });

    // The heap object the inlined field will point at.
    let leaf = Value::Object(GcRef::new(ScriptObject::new(dummy_type_desc("Leaf"), crate::metadata::types::ObjStorage::new(0, 0))));
    // A Holder instance: one 8B inline field window, zero-initialized.
    let holder = GcRef::new(ScriptObject::new(holder_td, crate::metadata::types::ObjStorage::new(8, 0)));

    // Zeroed window (`0` sentinel) reads back as `Null`.
    assert!(matches!(holder.borrow().field_value(0), Value::Null), "zeroed inline field = Null");
    // Writing an object inlines its 8B tagged pointer AND reports a reference write
    // (so the caller still fires `write_barrier_field`).
    let wrote_ref = holder.borrow_mut().set_field_value(0, &leaf);
    assert!(wrote_ref, "inlined object field is a reference slot");
    // Reads back by reference identity (address + generation preserved through bytes).
    match holder.borrow().field_value(0) {
        Value::Object(got) => match &leaf {
            Value::Object(l) => assert!(GcRef::ptr_eq(&got, l), "inlined object round-trips by identity"),
            _ => unreachable!(),
        },
        o => panic!("expected the inlined object, got {o:?}"),
    }
    // GC must reach the leaf THROUGH the inlined pointer in `bytes` (side-table is empty).
    let hv = Value::Object(holder);
    let mut visited = 0usize;
    hv.trace_children(&mut |v: &Value| if matches!(v, Value::Object(_)) { visited += 1; });
    assert_eq!(visited, 1, "trace_children visits the byte-inlined object ref");
}

#[test]
fn struct_array_backing_roundtrip_and_gc_refs() {
    use crate::metadata::types::*;
    // Point[2] with element layout {x:i32@0, y:i32@4, tag:string@8}, size 12, 1 ref leaf.
    let layout = Arc::new(StructTypeLayout {
        size: 12,
        ref_offsets: Box::new([8]),
        ref_kinds: Box::new([STRUCT_REF_ARC_STRING]),
    });
    // unify-gc-heap PR-3: struct[] byte + ref storage now lives in leaked GC blocks
    // (heap-less test) — build via `struct_backed_leaked` + write element blobs through
    // the heap-aware `write_struct_elem`, read back via `struct_bytes()` / `gc_refs()`.
    let mut arr = ArrayObj::struct_backed_leaked("Demo.P", 2, layout.clone());
    assert_eq!(arr.len(), 2, "2 elements of 12 bytes each");

    // add-boxed-struct-identity (P4b): boxing a struct[] element is now a heap alloc
    // (a shared `ScriptObject`), which this heap-less `ArrayObj` unit test can't do —
    // and `get_boxed`/`set_boxed` no longer round-trip a value box. The struct[] byte
    // backing itself (byte storage + independence + GC ref leaves) is what this test
    // covers; the boxed round-trip is exercised end-to-end by the exec-layer /
    // reflection golden tests. Write the element blobs straight into the backing.
    let write = |arr: &mut ArrayObj, i: usize, x: i32, y: i32, tag: &str| {
        let mut b = [0u8; 12];
        b[0..4].copy_from_slice(&x.to_le_bytes());
        b[4..8].copy_from_slice(&y.to_le_bytes());
        arr.write_struct_elem(i, &b, std::slice::from_ref(&Value::Str(tag.into())));
    };
    write(&mut arr, 0, 5, 6, "a");
    write(&mut arr, 1, 7, 8, "b");

    // Element bytes are stored + independent across elements.
    let bytes = arr.struct_bytes().expect("StructBytes backing");
    assert_eq!(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]), 5);
    assert_eq!(i32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]), 6);
    assert_eq!(i32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]), 7);
    match &arr.gc_refs()[0] { Value::Str(s) => assert_eq!(&**s, "a"), o => panic!("{o:?}") }

    // GC must see both elements' string ref leaves (else premature free).
    assert_eq!(arr.gc_refs().len(), 2, "both struct[] element ref leaves are GC roots");
    let strs: Vec<&str> = arr.gc_refs().iter().filter_map(|v| match v {
        Value::Str(s) => Some(&**s), _ => None,
    }).collect();
    assert_eq!(strs, vec!["a", "b"]);
}

#[test]
fn object_storage_empty_for_fieldless_type() {
    // shrink-object-footprint P2: a field-less type with no delivered/synthesized
    // layout has an empty payload — and allocates nothing for it.
    let td = dummy_type_desc("Plain");
    let storage = td.object_storage();
    assert!(storage.bytes().is_empty() && storage.refs().is_empty());
    assert_eq!(td.object_region_sizes(), (0, 0));
}

#[test]
fn is_heap_ref_false_for_primitives() {
    assert!(!Value::I64(0).is_heap_ref());
    assert!(!Value::F64(0.0).is_heap_ref());
    assert!(!Value::Bool(true).is_heap_ref());
    assert!(!Value::Char('a').is_heap_ref());
    assert!(!Value::Null.is_heap_ref());
}

#[test]
fn is_heap_ref_true_for_string_and_func_ref() {
    // unify-gc-heap PR-4: strings are GC blocks now — `Value::Str` / `Value::FuncRef`
    // (which carries a `Str`) are heap refs, so a write into a heap slot fires the
    // barrier (generational card / concurrent mark-queue) that keeps the block marked.
    assert!(Value::Str("hello".to_string().into()).is_heap_ref());
    assert!(Value::FuncRef("Foo.bar".into()).is_heap_ref());
}

#[test]
fn is_heap_ref_false_for_pinned_view() {
    let v = Value::PinnedView { idx: 0, frame_id: 1 };
    assert!(!v.is_heap_ref());
}

#[test]
fn is_heap_ref_false_for_stack_closure() {
    let v = Value::StackClosure { idx: 0, frame_id: 1 };
    assert!(!v.is_heap_ref());
}

#[test]
fn is_heap_ref_false_for_ref_stack() {
    let v = Value::Ref { idx: 0, frame_id: 1 };
    assert!(!v.is_heap_ref(), "stack ref points to stack location, not heap");
}

#[test]
fn default_value_for_string_keys_match_tags() {
    // Sanity: the string-keyed and byte-keyed lookups stay in sync for the
    // primitive types — drift here would silently regress per-type defaults
    // across the two call paths (FieldSlot vs ArrayNew).
    assert!(matches!(default_value_for("bool"), Value::Bool(false)));
    assert!(matches!(default_value_for("int"),  Value::I64(0)));
    assert!(matches!(default_value_for("long"), Value::I64(0)));
    assert!(matches!(default_value_for("byte"), Value::I64(0)));
    assert!(matches!(default_value_for("char"), Value::Char('\0')));
    assert!(matches!(default_value_for("string"), Value::Null));
    match default_value_for("double") {
        Value::F64(v) => assert_eq!(v, 0.0),
        other => panic!("expected F64(0.0), got {:?}", other),
    }
    // fix-type-reflection-names: FQ wrapper names (from reflective method_type_args,
    // e.g. `MakeGenericMethod(typeof(int)).Invoke` → `default(T)`) yield the same
    // value-type zero as the keyword/tag spellings.
    assert!(matches!(default_value_for("Std.Int32"), Value::I64(0)));
    assert!(matches!(default_value_for("Std.Int64"), Value::I64(0)));
    assert!(matches!(default_value_for("Std.Byte"), Value::I64(0)));
    assert!(matches!(default_value_for("Std.Boolean"), Value::Bool(false)));
    assert!(matches!(default_value_for("Std.Char"), Value::Char('\0')));
    assert!(matches!(default_value_for("Std.String"), Value::Null)); // reference → null
    match default_value_for("Std.Double") {
        Value::F64(v) => assert_eq!(v, 0.0),
        other => panic!("expected F64(0.0), got {:?}", other),
    }
    match default_value_for("Std.Single") {
        Value::F64(v) => assert_eq!(v, 0.0),
        other => panic!("expected F64(0.0), got {:?}", other),
    }
}

#[test]
fn value_size_observed() {
    // Diagnostic: pin current Value size. Refactors that hot/cold-split
    // variants (review.md C1) should make this shrink. Update the expected
    // when an intentional shrink lands.
    //
    // 2026-05-27 review.md C1 chunks 1-5 (all cold variants boxed):
    // Value shrunk 48 B → 24 B. Max-payload variant was
    // Str(Arc<str>) = 16 B → +1 B tag + 7 B align = 24 B.
    //
    // 2026-08-15 unify-object-byte-layout PR-3/4/5: GcRef 16→8 B (PR-3),
    // Str Arc<str> 16→8 B thin (PR-4), FuncRef Box<str> 16→8 B thin (PR-5).
    // Every payload is now ≤ 8 B → tag(1 B, padded to 8) + 8 B = 16 B.
    // This is also enforced at compile time by the `const _: () = assert!`
    // in types.rs; keep both in sync.
    assert_eq!(std::mem::size_of::<Value>(), 16,
        "Value size changed: {}", std::mem::size_of::<Value>());
    assert_eq!(std::mem::align_of::<Value>(), 8, "Value alignment changed");
}

// ── review.md C2 P1 step 0 (2026-05-28): Value layout pin ──────────────
//
// `Value` uses `#[repr(C, u8)]` so the JIT can load/store register
// payloads via raw memory access. These tests pin the discriminant
// values + payload offset so drift fails CI before bad JIT loads emit.
//
// Pinned layout (x86-64 / aarch64, alignment 8):
//   * offset 0  — u8 discriminant (explicit assignments in Value enum)
//   * offset 8  — payload (max 8 B since PR-3/4/5: GcRef / Str / FuncRef
//     are all thin 8 B pointers → Value is 16 B, see `value_size_observed`)

#[test]
fn value_discriminants_pinned() {
    fn tag(v: &Value) -> u8 {
        unsafe { *(v as *const Value as *const u8) }
    }
    assert_eq!(tag(&Value::I64(0)),                            0, "I64 tag");
    assert_eq!(tag(&Value::F64(0.0)),                          1, "F64 tag");
    assert_eq!(tag(&Value::Bool(false)),                       2, "Bool tag");
    assert_eq!(tag(&Value::Char('\0')),                        3, "Char tag");
    assert_eq!(tag(&Value::Str(crate::metadata::vstr::Str::from(""))),     4, "Str tag");
    assert_eq!(tag(&Value::Null),                              5, "Null tag");
    // Heap variants (Array/Object tags 6/7) need a GcRef — skip cheap test.
    assert_eq!(tag(&Value::PinnedView { idx: 0, frame_id: 1 }), 8, "PinnedView tag");
    assert_eq!(tag(&Value::FuncRef("".into())),                9, "FuncRef tag");
    // Closure tag 10 — needs GcRef, skip.
    assert_eq!(tag(&Value::StackClosure { idx: 0, frame_id: 1 }), 11, "StackClosure tag");
    assert_eq!(tag(&Value::Ref { idx: 0, frame_id: 1 }), 12, "Ref tag");
}

#[test]
fn value_i64_payload_at_offset_8() {
    // I64 payload at offset 8 (after u8 tag + 7 B padding to align(8)).
    // C2 P1 JIT will emit `iadd` against values loaded from this offset;
    // drift breaks the fast path silently.
    let v = Value::I64(0x1234_5678_9ABC_DEF0);
    unsafe {
        let base = &v as *const Value as *const u8;
        let payload_ptr = base.add(8) as *const i64;
        assert_eq!(*payload_ptr, 0x1234_5678_9ABC_DEF0_i64);
    }
}

#[test]
fn value_f64_payload_at_offset_8() {
    let v = Value::F64(std::f64::consts::PI);
    unsafe {
        let base = &v as *const Value as *const u8;
        let payload_ptr = base.add(8) as *const f64;
        assert_eq!(*payload_ptr, std::f64::consts::PI);
    }
}

#[test]
fn value_bool_payload_at_offset_8() {
    let v_true  = Value::Bool(true);
    let v_false = Value::Bool(false);
    unsafe {
        let base_true  = &v_true  as *const Value as *const u8;
        let base_false = &v_false as *const Value as *const u8;
        assert_eq!(*base_true.add(8),  1);
        assert_eq!(*base_false.add(8), 0);
    }
}

// ── unify Phase 2 R3（装箱统一）: ScriptObject::boxed_prim_i64 —— 整数基元装箱盒的
//    struct_bytes 标量编解码（宽度 + 有无符号扩展）。box 侧存 LE 字节，unbox 侧按 wrapper
//    名的 (width, signed) 还原 i64。见 well_known_names::int_wrapper_scalar_spec。────────

fn boxed_prim(name: &str, bytes: &[u8]) -> GcRef<ScriptObject> {
    GcRef::new(        // shrink-object-footprint P2: a boxed primitive's scalar is its whole byte
        // payload; no reference leaves.
ScriptObject::new(dummy_type_desc(name), crate::metadata::types::ObjStorage::from_bytes(bytes)))
}

#[test]
fn boxed_prim_i64_roundtrip_widths_and_sign() {
    // Int32: 4 bytes, signed. +5 与 -5（符号扩展）
    assert_eq!(boxed_prim("Std.Int32", &5i32.to_le_bytes()).borrow().boxed_prim_i64(), Some(5));
    assert_eq!(boxed_prim("Std.Int32", &(-5i32).to_le_bytes()).borrow().boxed_prim_i64(), Some(-5));
    // Int64: 8 bytes, signed
    assert_eq!(boxed_prim("Std.Int64", &(-1i64).to_le_bytes()).borrow().boxed_prim_i64(), Some(-1));
    assert_eq!(boxed_prim("Std.Int64", &9i64.to_le_bytes()).borrow().boxed_prim_i64(), Some(9));
    // Byte(unsigned 1B): 255 零扩展；SByte(signed 1B): 0xFF → -1
    assert_eq!(boxed_prim("Std.Byte",  &[255u8]).borrow().boxed_prim_i64(), Some(255));
    assert_eq!(boxed_prim("Std.SByte", &[0xFFu8]).borrow().boxed_prim_i64(), Some(-1));
    // UInt32 max: 零扩展（不当负数）
    assert_eq!(boxed_prim("Std.UInt32", &u32::MAX.to_le_bytes()).borrow().boxed_prim_i64(),
               Some(u32::MAX as i64));
    // Int16: -1000 符号扩展
    assert_eq!(boxed_prim("Std.Int16", &(-1000i16).to_le_bytes()).borrow().boxed_prim_i64(), Some(-1000));
}

#[test]
fn boxed_prim_i64_none_for_non_int_wrapper_and_short_bytes() {
    // 非整数 wrapper（普通 struct 装箱盒 / 未知名）→ None（调用方据此走 struct-blob 分流）
    assert_eq!(boxed_prim("some.Point", &[1, 2, 3, 4]).borrow().boxed_prim_i64(), None);
    assert_eq!(boxed_prim("Std.Double", &8u64.to_le_bytes()).borrow().boxed_prim_i64(), None);
    // struct_bytes 短于 wrapper 宽度（不该发生）→ None，不 panic
    assert_eq!(boxed_prim("Std.Int32", &[1, 2]).borrow().boxed_prim_i64(), None);
}

// ── unify-object-byte-layout (PR-2, task 2.0): compose_object_layout ──────────

#[test]
fn compose_object_layout_root_is_identity() {
    let own = crate::metadata::bytecode::ObjectLayoutDesc {
        size: 24,
        field_offsets: Box::new([0, 8, 16]),
        field_sizes:   Box::new([8, 8, 8]),
        field_kinds:   Box::new([0, 1, 2]), // Prim @0, ArcString @8, GcRef(object) @16
        ref_offsets:   Box::new([8, 16]),
        ref_kinds:     Box::new([1, 2]),
    };
    let composed = compose_object_layout(None, &own, &[]);
    // No base → identity (base_shift 0).
    assert_eq!(composed.size, 24);
    assert_eq!(&*composed.field_offsets, &[0, 8, 16]);
    // PR-3 chunk 2b: the string leaf @8 stays in the side-table; the direct object
    // (GcRef) leaf @16 is byte-inlined and removed from `ref_offsets`.
    assert_eq!(&*composed.ref_offsets, &[8], "only the string ref stays side-table");
    assert_eq!(composed.ref_count(), 1);
    assert_eq!(composed.inline_refs.len(), 1, "the object ref @16 inlined");
    assert_eq!(composed.inline_refs[0].offset, 16);
    assert!(!composed.inline_refs[0].is_array);
}

#[test]
fn compose_object_layout_pads_base_to_8() {
    // Base size 1 (single bool) must pad to 8 before the own region starts —
    // the unified 8B inheritance boundary keeps 8-aligned refs aligned.
    let base = ObjectLayout {
        size: 1,
        field_offsets: Box::new([0]),
        field_sizes:   Box::new([1]),
        field_kinds:   Box::new([0]),
        ref_offsets:   Box::new([]),
        ref_kinds:     Box::new([]),
        inline_refs:   Box::new([]),
        field_access: Box::new([]),
    };
    let own = crate::metadata::bytecode::ObjectLayoutDesc {
        size: 8,
        field_offsets: Box::new([0]),
        field_sizes:   Box::new([8]),
        field_kinds:   Box::new([2]), // STRUCT_LEAF_GCREF (object) → PR-3 chunk 2b inlines it
        ref_offsets:   Box::new([0]),
        ref_kinds:     Box::new([2]),
    };
    let composed = compose_object_layout(Some(&base), &own, &[]);
    // base_shift = align_up(1, 8) = 8.
    assert_eq!(composed.size, 16, "8 (padded base) + 8 (own)");
    assert_eq!(&*composed.field_offsets, &[0, 8], "own field shifted to 8, not 1");
    // PR-3 chunk 2b: the own direct object field is byte-inlined at the shifted offset 8
    // (pulled out of the side-table ref bitmap), so `ref_offsets` is empty and the leaf
    // shows up in `inline_refs` instead.
    assert!(composed.ref_offsets.is_empty(), "own GCREF field inlined, not in side-table");
    assert_eq!(composed.ref_index(8), None, "inlined ref has no side-table slot");
    assert_eq!(composed.inline_refs.len(), 1, "one inlined direct object ref");
    assert_eq!(composed.inline_refs[0].offset, 8, "inlined at the shifted offset 8");
    assert!(!composed.inline_refs[0].is_array, "object field → Value::Object, not Array");
}

#[test]
fn compose_object_layout_already_aligned_base_no_extra_pad() {
    let base = ObjectLayout {
        size: 16,
        field_offsets: Box::new([0, 8]),
        field_sizes:   Box::new([8, 8]),
        field_kinds:   Box::new([0, 0]),
        ref_offsets:   Box::new([]),
        ref_kinds:     Box::new([]),
        inline_refs:   Box::new([]),
        field_access: Box::new([]),
    };
    let own = crate::metadata::bytecode::ObjectLayoutDesc {
        size: 4,
        field_offsets: Box::new([0]),
        field_sizes:   Box::new([4]),
        field_kinds:   Box::new([0]),
        ref_offsets:   Box::new([]),
        ref_kinds:     Box::new([]),
    };
    let composed = compose_object_layout(Some(&base), &own, &[]);
    // align_up(16, 8) == 16 — no extra padding.
    assert_eq!(composed.size, 20);
    assert_eq!(&*composed.field_offsets, &[0, 8, 16]);
}

#[test]
fn script_object_stays_small() {
    // shrink-object-footprint: `ScriptObject` is the payload of every
    // `RegionEntry`, so its size is multiplied by the live object count. Pin it
    // so an added field is a deliberate decision with a measurement, not a drift.
    assert_eq!(std::mem::size_of::<ScriptObject>(), 32,
        "ScriptObject grew — re-measure per-object RSS before updating this");
}
