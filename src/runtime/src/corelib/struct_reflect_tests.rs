//! Unit tests for the value-struct field layout replication (`struct_reflect`).
//! Verifies the Rust replication of the compiler's `_compute` matches expected byte
//! offsets, that `validate_against` accepts a matching delivered layout + rejects a
//! tampered one, and that `tag_from_name` is a faithful `Tag.FromName` mirror (the
//! signedness guardrail).

use super::*;
use crate::metadata::types::{
    self as ty, FieldSlot, StructTypeLayout, TypeDesc, TypeDescCold, STRUCT_REF_ARC_STRING,
};
use crate::metadata::tokens::TypeId;
use crate::metadata::NameIndex;
use std::collections::HashMap;
use std::sync::Arc;

/// Build a value-struct `TypeDesc` with the given `(field_name, type_tag)` fields + an
/// authoritative `struct_layout`.
fn struct_td(name: &str, fields: &[(&str, &str)], layout: StructTypeLayout) -> Arc<TypeDesc> {
    let mut cold = TypeDescCold::default();
    cold.struct_layout = Some(Arc::new(layout));
    let mut field_index = NameIndex::new();
    let mut fs = Vec::new();
    for (i, (n, t)) in fields.iter().enumerate() {
        field_index.insert(n.to_string(), i);
        fs.push(FieldSlot { name: (*n).into(), type_tag: (*t).into(), visibility: 0 });
    }
    Arc::new(TypeDesc {
        name: name.to_string(),
        base_name: None,
        class_flags: crate::metadata::bytecode::CLASS_FLAG_STRUCT,
        fields: fs,
        field_index,
        vtable: Vec::new(),
        vtable_index: NameIndex::new(),
        cold: Some(Box::new(cold)),
        id: TypeId::UNRESOLVED,
    })
}

/// Build a **non-struct class** `TypeDesc` with the given `(field_name, type_tag)` fields +
/// an authoritative composed `inline_layout` (object-relative byte region + ref bitmap).
fn class_td(name: &str, fields: &[(&str, &str)], inline: StructTypeLayout) -> Arc<TypeDesc> {
    let mut cold = TypeDescCold::default();
    cold.inline_layout = Some(Arc::new(inline));
    let mut field_index = NameIndex::new();
    let mut fs = Vec::new();
    for (i, (n, t)) in fields.iter().enumerate() {
        field_index.insert(n.to_string(), i);
        fs.push(FieldSlot { name: (*n).into(), type_tag: (*t).into(), visibility: 0 });
    }
    Arc::new(TypeDesc {
        name: name.to_string(),
        base_name: None,
        class_flags: 0, // not a struct
        fields: fs,
        field_index,
        vtable: Vec::new(),
        vtable_index: NameIndex::new(),
        cold: Some(Box::new(cold)),
        id: TypeId::UNRESOLVED,
    })
}

fn layout(size: usize, ref_offsets: &[u32]) -> StructTypeLayout {
    StructTypeLayout {
        size,
        ref_offsets: ref_offsets.to_vec().into_boxed_slice(),
        ref_kinds: vec![STRUCT_REF_ARC_STRING; ref_offsets.len()].into_boxed_slice(),
    }
}

/// Build a resolver closure over a set of types.
fn resolver(types: Vec<Arc<TypeDesc>>) -> impl Fn(&str) -> Option<Arc<TypeDesc>> {
    let map: HashMap<String, Arc<TypeDesc>> =
        types.into_iter().map(|t| (t.name.clone(), t)).collect();
    move |n: &str| map.get(n).cloned()
}

#[test]
fn pure_primitive_struct_offsets() {
    // Demo.Pt { x:i32@0, y:i32@4 } size 8, no refs.
    let pt = struct_td("Demo.Pt", &[("x", "i32"), ("y", "i32")], layout(8, &[]));
    let r = resolver(vec![pt]);
    let c = compute(&r, "Demo.Pt").unwrap();
    assert_eq!(c.size, 8);
    assert_eq!(c.leaves.len(), 2);
    assert_eq!((c.leaves[0].byte_off, c.leaves[0].tag, c.leaves[0].is_ref), (0, ty::TAG_I32, false));
    assert_eq!((c.leaves[1].byte_off, c.leaves[1].tag), (4, ty::TAG_I32));
    assert!(c.ref_offsets.is_empty());
    c.validate_against(&layout(8, &[]), "Demo.Pt").unwrap();
}

#[test]
fn struct_with_string_ref_leaf_alignment() {
    // Demo.P { x:i32@0, y:i32@4, tag:str@8(16B,align8) } → size 24, ref@8.
    let p = struct_td(
        "Demo.P",
        &[("x", "i32"), ("y", "i32"), ("tag", "str")],
        layout(24, &[8]),
    );
    let r = resolver(vec![p]);
    let c = compute(&r, "Demo.P").unwrap();
    assert_eq!(c.size, 24);
    assert_eq!(c.leaves[2].byte_off, 8);
    assert!(c.leaves[2].is_ref, "string leaf is a reference");
    assert_eq!(c.ref_offsets, vec![8]);
    c.validate_against(&layout(24, &[8]), "Demo.P").unwrap();
}

#[test]
fn nested_struct_flattens_ref_leaves() {
    // Demo.Seg { p:Demo.P, n:i32 } where P{x:i32,y:i32,tag:str} size24 ref@8.
    // p@0 size24; n:i32 align4 → @24 size4 → offset28; size = align_up(28,8)=32; ref@8.
    let p = struct_td(
        "Demo.P",
        &[("x", "i32"), ("y", "i32"), ("tag", "str")],
        layout(24, &[8]),
    );
    let seg = struct_td("Demo.Seg", &[("p", "Demo.P"), ("n", "i32")], layout(32, &[8]));
    let r = resolver(vec![p, seg]);
    let c = compute(&r, "Demo.Seg").unwrap();
    assert_eq!(c.size, 32);
    assert!(c.leaves[0].is_struct, "p is a nested struct field");
    assert_eq!(c.leaves[0].byte_off, 0);
    assert_eq!(c.leaves[1].byte_off, 24, "n follows the 24-byte nested struct");
    assert_eq!(c.ref_offsets, vec![8], "nested P's string ref flattens into parent at offset 8");
    c.validate_against(&layout(32, &[8]), "Demo.Seg").unwrap();
}

#[test]
fn mixed_alignment_padding() {
    // Demo.Mix { a:i8@0, b:i64@8, c:bool@16 } → size align_up(17,8)=24, no refs.
    let mix = struct_td(
        "Demo.Mix",
        &[("a", "i8"), ("b", "i64"), ("c", "bool")],
        layout(24, &[]),
    );
    let r = resolver(vec![mix]);
    let c = compute(&r, "Demo.Mix").unwrap();
    assert_eq!(c.leaves[0].byte_off, 0);
    assert_eq!(c.leaves[1].byte_off, 8, "i64 aligns to 8");
    assert_eq!(c.leaves[2].byte_off, 16);
    assert_eq!(c.size, 24);
    c.validate_against(&layout(24, &[]), "Demo.Mix").unwrap();
}

#[test]
fn validate_rejects_wrong_size() {
    let pt = struct_td("Demo.Pt", &[("x", "i32"), ("y", "i32")], layout(8, &[]));
    let r = resolver(vec![pt]);
    let c = compute(&r, "Demo.Pt").unwrap();
    // Delivered size disagrees → replication-drift bail.
    assert!(c.validate_against(&layout(12, &[]), "Demo.Pt").is_err());
}

#[test]
fn validate_rejects_wrong_ref_bitmap() {
    let p = struct_td(
        "Demo.P",
        &[("x", "i32"), ("y", "i32"), ("tag", "str")],
        layout(24, &[8]),
    );
    let r = resolver(vec![p]);
    let c = compute(&r, "Demo.P").unwrap();
    // Delivered bitmap says a ref at offset 4 (wrong) → bail.
    assert!(c.validate_against(&layout(24, &[4]), "Demo.P").is_err());
    // Delivered bitmap empty while we computed a ref → bail.
    assert!(c.validate_against(&layout(24, &[]), "Demo.P").is_err());
}

#[test]
fn tag_from_name_signedness_guardrail() {
    // Every primitive spelling maps to the exact zbc Tag the codegen bakes, so reflection
    // decodes with matching width + signedness. Aliases + canon forms both covered.
    let cases: &[(&str, u8)] = &[
        ("bool", ty::TAG_BOOL),
        ("i8", ty::TAG_I8),
        ("i16", ty::TAG_I16),
        ("i32", ty::TAG_I32),
        ("int", ty::TAG_I32),
        ("i64", ty::TAG_I64),
        ("long", ty::TAG_I64),
        ("u8", ty::TAG_U8),
        ("u16", ty::TAG_U16),
        ("u32", ty::TAG_U32),
        ("u64", ty::TAG_U64),
        ("f32", ty::TAG_F32),
        ("float", ty::TAG_F32),
        ("f64", ty::TAG_F64),
        ("double", ty::TAG_F64),
        ("char", ty::TAG_CHAR),
        ("str", ty::TAG_STR),
    ];
    for &(spelling, expected) in cases {
        // A one-field struct isolates the leaf's tag.
        let is_ref = matches!(expected, ty::TAG_STR);
        let (size, refs): (usize, &[u32]) = if is_ref { (16, &[0]) } else {
            let w = match expected {
                ty::TAG_BOOL | ty::TAG_I8 | ty::TAG_U8 => 1,
                ty::TAG_I16 | ty::TAG_U16 => 2,
                ty::TAG_I32 | ty::TAG_U32 | ty::TAG_F32 | ty::TAG_CHAR => 4,
                _ => 8,
            };
            (w, &[])
        };
        let td = struct_td("Demo.One", &[("v", spelling)], layout(size, refs));
        let r = resolver(vec![td]);
        let c = compute(&r, "Demo.One").unwrap();
        assert_eq!(c.leaves[0].tag, expected, "tag for `{spelling}`");
        c.validate_against(&layout(size, refs), "Demo.One").unwrap();
    }
}

// ── add-object-inline-struct-reflection (P4b-B): class inline layout replication ─────────

#[test]
fn class_inline_layout_packs_only_struct_fields() {
    // class Demo.C { int id; Point pt; string label; } where Point{x:i32,y:i32,tag:str}
    // size24 ref@8. Only `pt` is byte-packed: id/label keep real slots. Inline region:
    // pt@0 (align8) size24, ref leaf flattened to 8 → region size 24, ref@8.
    let p = struct_td(
        "Demo.Point",
        &[("x", "i32"), ("y", "i32"), ("tag", "str")],
        layout(24, &[8]),
    );
    let c = class_td(
        "Demo.C",
        &[("id", "i32"), ("pt", "Demo.Point"), ("label", "str")],
        layout(24, &[8]),
    );
    let r = resolver(vec![p, c]);
    let inline = compute_class_inline(&r, "Demo.C").unwrap();
    assert_eq!(inline.leaves.len(), 1, "only the struct field is inlined");
    assert_eq!(inline.leaves[0].name, "pt");
    assert!(inline.leaves[0].is_struct);
    assert_eq!(inline.leaves[0].byte_off, 0);
    assert_eq!(inline.leaves[0].size, 24);
    assert_eq!(inline.ref_offsets, vec![8], "nested Point's string ref flattens to region offset 8");
    assert_eq!(inline.size, 24);
    // Validates against the authoritative delivered composed inline layout.
    inline.validate_against(&layout(24, &[8]), "Demo.C").unwrap();
    // struct_field_fq classifies fields correctly.
    assert_eq!(struct_field_fq(&r, "Demo.C", "pt").as_deref(), Some("Demo.Point"));
    assert!(struct_field_fq(&r, "Demo.C", "id").is_none(), "primitive field is not inline struct");
    assert!(struct_field_fq(&r, "Demo.C", "label").is_none(), "string field is not inline struct");
}

#[test]
fn class_inline_layout_two_struct_fields_pack_contiguously() {
    // class Demo.D { Point a; int n; Point b; } → a@0 size24 (ref@8), b@24 size24 (ref@32),
    // n stays a slot. Region size 48, refs @8,@32.
    let p = struct_td(
        "Demo.Point",
        &[("x", "i32"), ("y", "i32"), ("tag", "str")],
        layout(24, &[8]),
    );
    let d = class_td(
        "Demo.D",
        &[("a", "Demo.Point"), ("n", "i32"), ("b", "Demo.Point")],
        layout(48, &[8, 32]),
    );
    let r = resolver(vec![p, d]);
    let inline = compute_class_inline(&r, "Demo.D").unwrap();
    assert_eq!(inline.leaves.len(), 2);
    assert_eq!((inline.leaves[0].name.as_str(), inline.leaves[0].byte_off), ("a", 0));
    assert_eq!((inline.leaves[1].name.as_str(), inline.leaves[1].byte_off), ("b", 24));
    assert_eq!(inline.ref_offsets, vec![8, 32]);
    assert_eq!(inline.size, 48);
    inline.validate_against(&layout(48, &[8, 32]), "Demo.D").unwrap();
}

#[test]
fn class_with_no_struct_fields_has_empty_inline_layout() {
    let c = class_td("Demo.Plain", &[("id", "i32"), ("label", "str")], layout(0, &[]));
    let r = resolver(vec![c]);
    let inline = compute_class_inline(&r, "Demo.Plain").unwrap();
    assert!(inline.leaves.is_empty());
    assert_eq!(inline.size, 0);
    assert!(struct_field_fq(&r, "Demo.Plain", "id").is_none());
}
