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
    let v = Value::Object(GcRef::new(ScriptObject {
        type_desc: dummy_type_desc("Foo"),
        slots: Box::new([]),
        struct_bytes: Box::new([]),
        struct_refs: Box::new([]),
        native: NativeData::None,
        type_args: Box::new([]),
    }));
    assert!(v.is_heap_ref());
}

#[test]
fn is_heap_ref_true_for_array() {
    let v = Value::Array(GcRef::new(crate::metadata::types::ArrayObj::new(vec![Value::I64(1), Value::I64(2)])));
    assert!(v.is_heap_ref());
}

#[test]
fn is_heap_ref_true_for_closure() {
    let v = Value::Closure(Box::new(ClosureData {
        env:     GcRef::new(crate::metadata::types::ArrayObj::new(vec![Value::I64(42)])),
        fn_name: "lambda$0".to_string(),
    }));
    assert!(v.is_heap_ref());
}

#[test]
fn is_heap_ref_true_for_ref_array() {
    let arr = GcRef::new(crate::metadata::types::ArrayObj::new(vec![Value::I64(0)]));
    let v = Value::Ref(Box::new(RefKind::Array { gc_ref: arr, idx: 0 }));
    assert!(v.is_heap_ref());
}

#[test]
fn is_heap_ref_true_for_ref_field() {
    let obj = GcRef::new(ScriptObject {
        type_desc: dummy_type_desc("Foo"),
        slots: Box::new([Value::I64(0)]),
        struct_bytes: Box::new([]),
        struct_refs: Box::new([]),
        native: NativeData::None,
        type_args: Box::new([]),
    });
    let v = Value::Ref(Box::new(RefKind::Field { gc_ref: obj, field_name: "x".to_string() }));
    assert!(v.is_heap_ref());
}

// ── add-struct-heap-inline (P3b, D1-a): inline struct fields' reference side-table
//    is GC-traversed (scaffolding is inert in production until the stage-3 format
//    wire populates it, so validate the traversal via manual construction). ──────

#[test]
fn trace_children_visits_inline_struct_refs() {
    // An object whose inline struct field holds a reference leaf (e.g. a nested
    // object) in `struct_refs`. `trace_children` must visit it so the leaf stays
    // marked — exactly the use-after-free P3b closes (`c.pt` holding a string/obj).
    let leaf = Value::Object(GcRef::new(ScriptObject {
        type_desc: dummy_type_desc("Leaf"),
        slots: Box::new([]),
        struct_bytes: Box::new([]),
        struct_refs: Box::new([]),
        native: NativeData::None,
        type_args: Box::new([]),
    }));
    let owner = Value::Object(GcRef::new(ScriptObject {
        type_desc: dummy_type_desc("Owner"),
        slots: Box::new([Value::I64(7)]),          // an ordinary field
        struct_bytes: Box::new([0u8; 8]),          // inline struct's packed primitives
        struct_refs: Box::new([leaf.clone()]),     // inline struct's reference leaf
        native: NativeData::None,
        type_args: Box::new([]),
    }));

    let mut visited_object_children = 0usize;
    owner.trace_children(&mut |v: &Value| {
        if matches!(v, Value::Object(_)) { visited_object_children += 1; }
    });
    // The ordinary I64 field is not an object; the inline struct_refs leaf is.
    assert_eq!(visited_object_children, 1, "inline struct_refs leaf must be traced");
}

#[test]
fn inline_regions_empty_by_default() {
    // Stage 1: no class carries inline-field metadata → both regions empty, so
    // every existing object is byte-identical to pre-P3b.
    let td = dummy_type_desc("Plain");
    let (bytes, refs) = td.inline_regions();
    assert!(bytes.is_empty() && refs.is_empty());
    assert_eq!(td.inline_region_sizes(), (0, 0));
}

#[test]
fn is_heap_ref_false_for_primitives() {
    assert!(!Value::I64(0).is_heap_ref());
    assert!(!Value::F64(0.0).is_heap_ref());
    assert!(!Value::Bool(true).is_heap_ref());
    assert!(!Value::Char('a').is_heap_ref());
    assert!(!Value::Str("hello".to_string().into()).is_heap_ref());
    assert!(!Value::Null.is_heap_ref());
}

#[test]
fn is_heap_ref_false_for_func_ref() {
    assert!(!Value::FuncRef("Foo.bar".into()).is_heap_ref());
}

#[test]
fn is_heap_ref_false_for_pinned_view() {
    let v = Value::PinnedView(Box::new(PinnedViewData {
        ptr: 0x1000, len: 4, kind: PinSourceKind::ArrayU8,
    }));
    assert!(!v.is_heap_ref());
}

#[test]
fn is_heap_ref_false_for_stack_closure() {
    let v = Value::StackClosure(Box::new(StackClosureData { env_idx: 0, fn_name: "inner".to_string() }));
    assert!(!v.is_heap_ref());
}

#[test]
fn is_heap_ref_false_for_ref_stack() {
    let v = Value::Ref(Box::new(RefKind::Stack { frame_idx: 0, slot: 1 }));
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
}

#[test]
fn value_size_observed() {
    // Diagnostic: pin current Value size. Refactors that hot/cold-split
    // variants (review.md C1) should make this shrink. Update the expected
    // when an intentional shrink lands.
    //
    // 2026-05-27 review.md C1 chunks 1-5 (all cold variants boxed):
    // Value shrunk 48 B → 24 B. Max-payload variant is now
    // Str(Arc<str>) = 16 B → +1 B tag + 7 B align = 24 B.
    assert_eq!(std::mem::size_of::<Value>(), 24,
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
//   * offset 8  — payload (max 16 B, e.g. Arc<str> for Str)

#[test]
fn value_discriminants_pinned() {
    fn tag(v: &Value) -> u8 {
        unsafe { *(v as *const Value as *const u8) }
    }
    assert_eq!(tag(&Value::I64(0)),                            0, "I64 tag");
    assert_eq!(tag(&Value::F64(0.0)),                          1, "F64 tag");
    assert_eq!(tag(&Value::Bool(false)),                       2, "Bool tag");
    assert_eq!(tag(&Value::Char('\0')),                        3, "Char tag");
    assert_eq!(tag(&Value::Str(std::sync::Arc::from(""))),     4, "Str tag");
    assert_eq!(tag(&Value::Null),                              5, "Null tag");
    // Heap variants (Array/Object tags 6/7) need a GcRef — skip cheap test.
    assert_eq!(tag(&Value::PinnedView(Box::new(PinnedViewData {
        ptr: 0, len: 0, kind: PinSourceKind::Str,
    }))), 8, "PinnedView tag");
    assert_eq!(tag(&Value::FuncRef("".into())),                9, "FuncRef tag");
    // Closure tag 10 — needs GcRef, skip.
    assert_eq!(tag(&Value::StackClosure(Box::new(StackClosureData {
        env_idx: 0, fn_name: String::new(),
    }))), 11, "StackClosure tag");
    assert_eq!(tag(&Value::Ref(Box::new(RefKind::Stack {
        frame_idx: 0, slot: 0,
    }))), 12, "Ref tag");
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
