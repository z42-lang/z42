//! Array element modes (object model R3): REF / MIXED / PRIM / STRUCT cells.

use super::*;
use crate::gc::ambient::HeapGuard;
use crate::gc::ArcMagrGC;
use crate::gc::heap::MagrGC;
use std::sync::atomic::Ordering;

fn heap() -> ArcMagrGC {
    ArcMagrGC::new()
}

fn obj(h: &ArcMagrGC) -> Value {
    let td = std::sync::Arc::new(TypeDesc {
        class_flags: 0, visibility: 0, name: "O".into(), base_name: None, fields: Vec::new(),
        field_index: crate::metadata::NameIndex::new(), vtable: Vec::new(),
        vtable_index: crate::metadata::NameIndex::new(), cold: None,
        id: crate::metadata::tokens::TypeId::UNRESOLVED,
    });
    h.alloc_object(td, Vec::new(), NativeData::None)
}

fn arr(h: &ArcMagrGC, a: ArrayObj) -> crate::gc::GcRef<ArrayObj> {
    match h.alloc_array_obj(a) {
        Value::Array(r) => r,
        other => panic!("{other:?}"),
    }
}

#[test]
fn element_kind_by_name() {
    assert_eq!(ElemKind::of("int"), ElemKind::I32);
    assert_eq!(ElemKind::of("double"), ElemKind::F64);
    assert_eq!(ElemKind::of("string"), ElemKind::Ref);
    assert_eq!(ElemKind::of("object"), ElemKind::Ref);
    assert_eq!(ElemKind::of("int[]"), ElemKind::Ref, "an array of arrays holds references");
    assert_eq!(ElemKind::of("Demo.Point"), ElemKind::Mixed, "a bare name needs the type registry");
    assert_eq!(ElemKind::of("T"), ElemKind::Mixed);
    assert_eq!(ElemKind::of(""), ElemKind::Mixed, "unknown element type");
}

#[test]
fn ref_array_is_eight_bytes_per_element_and_round_trips_every_reference_kind() {
    let h = heap();
    let _g = HeapGuard::enter(&h);
    let o = obj(&h);
    let s = Value::Str(h.alloc_str("hi"));
    let inner = h.alloc_array(vec![Value::I64(1)]);
    let a = arr(&h, ArrayObj::filled(&h, ElemType::intern("object"), ElemKind::Ref, 4, Value::Null));
    assert!(matches!(a.borrow().backing, ArrayBacking::Refs { .. }));
    assert_eq!(a.borrow().elem_storage_bytes(), 4 * 8);
    for (i, v) in [o, s, inner, Value::Null].into_iter().enumerate() {
        let w = a.borrow_mut().set_boxed(i, v);
        assert!(matches!(w, FieldWrite::Ref), "a reference fits the word");
        assert_eq!(a.borrow().get_boxed(i), v);
    }
}

#[test]
fn a_primitive_reaching_a_ref_array_is_boxed_transparently() {
    let h = heap();
    let _g = HeapGuard::enter(&h);
    let a = arr(&h, ArrayObj::filled(&h, ElemType::intern("object"), ElemKind::Ref, 2, Value::Null));
    let w = a.borrow_mut().set_boxed(0, Value::I64(42));
    let FieldWrite::Boxed(b) = w else { panic!("a primitive needs the kind-7 box, got {w:?}") };
    assert_eq!(a.borrow().get_boxed(0), Value::I64(42), "reading unboxes");
    // The barrier / GC see the box itself.
    let (slots, per) = a.borrow().elem_ref_slots(0, 2);
    assert_eq!(per, 1);
    assert_eq!(slots[0], Value::Array(b));
    assert_eq!(slots[1], Value::Null);
}

#[test]
fn mixed_array_claims_a_primitive_kind_and_keeps_references_in_the_tag_word() {
    let h = heap();
    let _g = HeapGuard::enter(&h);
    // `new T[3]` with T = int: the zero fill writes nothing — a never-written cell reads as the
    // array's zero kind — and the first store claims the cell.
    let ints = arr(&h, ArrayObj::filled(&h, ElemType::intern("T"), ElemKind::Mixed, 3, Value::I64(0)));
    assert!(matches!(ints.borrow().backing, ArrayBacking::Cells { zero, .. } if zero != 0));
    assert!(matches!(ints.borrow().ref_elem_data(), Some((_, 16, 1))), "the JIT learns the zero kind (I64)");
    assert_eq!(ints.borrow().elem_storage_bytes(), 3 * 16);
    assert_eq!(ints.borrow().get_boxed(1), Value::I64(0));
    assert_eq!(ints.borrow().get_boxed(2), Value::I64(0));
    assert!(matches!(ints.borrow_mut().set_boxed(1, Value::I64(-7)), FieldWrite::NotRef));
    assert_eq!(ints.borrow().get_boxed(1), Value::I64(-7));
    assert!(matches!(ints.borrow_mut().set_boxed(1, Value::Null), FieldWrite::NotRef), "null keeps the claim");
    assert_eq!(ints.borrow().get_boxed(1), Value::Null);
    // Another primitive kind into a never-written cell of a zero-kind array: the cell counts as
    // claimed for the array's kind, so the value is boxed (no claim CAS on the hot path).
    let mut zs = ArrayObj::filled(&h, ElemType::intern("T"), ElemKind::Mixed, 1, Value::I64(0));
    assert!(matches!(zs.set_boxed(0, Value::F64(2.5)), FieldWrite::Boxed(_)));
    assert_eq!(zs.get_boxed(0), Value::F64(2.5));
    // Another primitive kind into a claimed cell: boxed, read back unchanged.
    ints.borrow_mut().set_boxed(2, Value::I64(9));
    assert!(matches!(ints.borrow_mut().set_boxed(2, Value::F64(1.5)), FieldWrite::Boxed(_)));
    assert_eq!(ints.borrow().get_boxed(2), Value::F64(1.5));

    // `new T[2]` with T = string: references live in the tag word, never boxed.
    let strs = arr(&h, ArrayObj::filled(&h, ElemType::intern("T"), ElemKind::Mixed, 2, Value::Null));
    let s = Value::Str(h.alloc_str("x"));
    assert!(matches!(strs.borrow_mut().set_boxed(0, s), FieldWrite::Ref));
    assert_eq!(strs.borrow().get_boxed(0), s);
    let mut seen = 0;
    strs.borrow().visit_refs(&mut |v| { assert_eq!(*v, s); seen += 1; });
    assert_eq!(seen, 1, "only the cell holding a reference is an edge");
}

#[test]
fn prim_arrays_use_same_width_cells() {
    let h = heap();
    let a = arr(&h, ArrayObj::typed(&h, "int", vec![Value::I64(1), Value::I64(-2)]));
    assert_eq!(a.borrow().elem_storage_bytes(), 8);
    a.borrow_mut().set_boxed(1, Value::I64(i32::MIN as i64));
    assert_eq!(a.borrow().get_boxed(1), Value::I64(i32::MIN as i64));
    let c = arr(&h, ArrayObj::typed(&h, "char", vec![Value::Char('é')]));
    assert_eq!(c.borrow().get_boxed(0), Value::Char('é'));
    let b = arr(&h, ArrayObj::typed(&h, "bool", vec![Value::Bool(false), Value::Bool(true)]));
    assert_eq!(b.borrow().get_boxed(1), Value::Bool(true));
}

#[test]
fn bulk_copies_keep_every_mode_intact() {
    let h = heap();
    let _g = HeapGuard::enter(&h);
    let (o1, o2) = (obj(&h), obj(&h));
    let src = arr(&h, ArrayObj::from_iter_kind(&h, ElemType::intern("object"), ElemKind::Ref, 3,
        [o1, Value::Null, o2].into_iter()));
    let dst = arr(&h, ArrayObj::filled(&h, ElemType::intern("object"), ElemKind::Ref, 4, Value::Null));
    dst.borrow_mut().copy_elems_from(&src.borrow(), 0, 1, 3);
    assert_eq!(dst.borrow().to_boxed_vec(), vec![Value::Null, o1, Value::Null, o2]);
    // memmove within: shift right by one.
    dst.borrow_mut().copy_elems_within(1, 0, 3);
    assert_eq!(dst.borrow().to_boxed_vec(), vec![o1, Value::Null, o2, o2]);
    // MIXED → REF: element-wise.
    let mixed = arr(&h, ArrayObj::new(&h, vec![o2, Value::I64(5)]));
    dst.borrow_mut().copy_elems_from(&mixed.borrow(), 0, 0, 2);
    assert_eq!(dst.borrow().get_boxed(0), o2);
    assert_eq!(dst.borrow().get_boxed(1), Value::I64(5));
    // deep copy shares the referents.
    let copy = dst.borrow().deep_copy(&h);
    assert_eq!(copy.to_boxed_vec(), dst.borrow().to_boxed_vec());
}

#[test]
fn struct_array_copies_blob_by_blob() {
    let h = heap();
    let layout = std::sync::Arc::new(StructTypeLayout {
        size: 16, ref_offsets: Box::new([8]), ref_kinds: Box::new([STRUCT_REF_GCREF]), fields: Box::new([]),
    });
    let o = obj(&h);
    let mut src = ArrayObj::struct_backed(&h, ElemType::intern("P"), 2, layout.clone());
    let mut b = [0u8; 16];
    b[0..4].copy_from_slice(&7i32.to_le_bytes());
    src.write_struct_elem(1, &b, &[o], &mut |_| {}).unwrap();
    let mut dst = ArrayObj::struct_backed(&h, ElemType::intern("P"), 3, layout);
    dst.copy_elems_from(&src, 0, 1, 2);
    let (bytes, refs) = dst.struct_elem(2).unwrap();
    assert_eq!(&bytes[0..4], &7i32.to_le_bytes());
    assert_eq!(refs, vec![o]);
    assert_eq!(dst.struct_elem(1).unwrap().1, vec![Value::Null]);
    let (slots, per) = dst.elem_ref_slots(0, 3);
    assert_eq!((per, slots), (1, vec![Value::Null, Value::Null, o]));
}

#[test]
fn frozen_arrays_hold_any_value_raw() {
    let h = heap();
    let a = arr(&h, ArrayObj::frozen(&h, vec![Value::StructRef { idx: 3, frame_id: 9 }, Value::I64(1)]));
    assert!(matches!(a.borrow().get_boxed(0), Value::StructRef { idx: 3, frame_id: 9 }),
        "a box can carry a stack handle no cell encodes");
    assert_eq!(a.borrow().elem_storage_bytes(), 2 * std::mem::size_of::<Value>());
}

#[test]
fn a_fresh_mixed_cell_encodes_like_one_store() {
    // `init_words` must leave exactly what the CAS protocol would after one write into a zeroed cell.
    let h = heap();
    let _g = HeapGuard::enter(&h);
    let o = obj(&h);
    let built = arr(&h, ArrayObj::new(&h, vec![Value::I64(3), o, Value::Null, Value::Char('z')]));
    let stored = arr(&h, ArrayObj::filled(&h, ElemType::empty(), ElemKind::Mixed, 4, Value::Null));
    for (i, v) in built.borrow().to_boxed_vec().into_iter().enumerate() {
        stored.borrow_mut().set_boxed(i, v);
    }
    let words = |a: &crate::gc::GcRef<ArrayObj>| {
        let g = a.borrow();
        let ArrayBacking::Cells { block, len, .. } = &g.backing else { panic!() };
        let c = ArrayObj::cells_of(block, len * CELL_STRIDE);
        (0..len * 2).map(|k| c.load_word(k * 8, Ordering::Relaxed)).collect::<Vec<_>>()
    };
    assert_eq!(words(&built), words(&stored));
}
