//! 8 B 自描述引用字：每个种类的编解码往返、null、种类表与 `Value` 判别值一致、基元装箱逃生口，
//! 以及 GC 经引用字追踪 / SATB 经引用字记录（R1）。

use super::*;
use crate::gc::arc_heap::ArcMagrGC;
use crate::gc::heap::MagrGC;
use crate::gc::var_region::BlockType;
use crate::gc::GcMode;
use crate::metadata::types::{
    FieldAccess, FieldSlot, FieldWrite, NativeData, ObjStorage, ObjectLayout, TypeDesc, TypeDescCold, TAG_OBJECT,
    STRUCT_LEAF_GCREF_CLOSURE,
};
use std::sync::Arc;

fn plain_td(name: &str) -> Arc<TypeDesc> {
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

/// `class Holder { object f0; object f1; … }` — `n` direct reference fields, each an 8 B
/// reference word at `8 * i`, as `compose_object_layout` lays them out.
pub(crate) fn ref_cell_td(n: usize) -> Arc<TypeDesc> {
    let offs: Vec<u32> = (0..n as u32).map(|i| i * 8).collect();
    let layout = Arc::new(ObjectLayout {
        size: 8 * n,
        field_offsets: offs.clone().into(),
        field_sizes: vec![8; n].into(),
        field_kinds: vec![STRUCT_LEAF_GCREF_CLOSURE; n].into(),
        ref_offsets: Box::new([]),
        ref_kinds: Box::new([]),
        ref_cells: offs.clone().into(),
        tparam_cells: Box::new([]),
        field_access: offs.iter().map(|&o| FieldAccess::ref_word(o, TAG_OBJECT)).collect(),
    });
    let fields: Vec<FieldSlot> = (0..n)
        .map(|i| FieldSlot { name: format!("f{i}").into(), type_tag: "object".into(), visibility: 0 })
        .collect();
    let field_index = fields.iter().enumerate().map(|(i, f)| (f.name.to_string(), i)).collect();
    Arc::new(TypeDesc {
        class_flags: 0,
        visibility: 0,
        name: "Holder".to_string(),
        base_name: None,
        fields,
        field_index,
        vtable: Vec::new(),
        vtable_index: crate::metadata::NameIndex::new(),
        cold: Some(Box::new(TypeDescCold { composed_object_layout: Some(layout), ..Default::default() })),
        id: crate::metadata::tokens::TypeId::UNRESOLVED,
    })
}

fn discriminant(v: &Value) -> u8 {
    // SAFETY: `Value` is `#[repr(C, u8)]` — the first byte is the discriminant.
    unsafe { *(v as *const Value as *const u8) }
}

fn payload(v: &Value) -> u64 {
    // SAFETY: `#[repr(C, u8)]` with every payload ≤ 8 B: payload at offset 8.
    unsafe { *((v as *const Value as *const u8).add(8) as *const u64) }
}

/// One live handle of every encodable kind.
fn one_of_each(heap: &ArcMagrGC) -> Vec<(Value, u64)> {
    let obj = GcRef::new(ScriptObject::new(plain_td("O"), ObjStorage::new(0, 0)));
    let boxed = GcRef::new(ScriptObject::new(plain_td("S"), ObjStorage::new(0, 0)));
    let Value::Array(arr) = heap.alloc_array(vec![]) else { panic!() };
    let s = Str::new_leaked("text");
    let f = Str::new_leaked("Demo.F");
    let clo = VarGcRef::leak_block_for_test(16, BlockType::Closure);
    vec![
        (Value::Object(obj), RK_OBJECT),
        (Value::Array(arr), RK_ARRAY),
        (Value::Str(s), RK_STR),
        (Value::Closure(clo), RK_CLOSURE),
        (Value::FuncRef(f), RK_FUNC_REF),
        (Value::BoxedStruct(boxed), RK_BOXED_STRUCT),
    ]
}

#[test]
fn every_kind_round_trips_with_its_tag_in_the_low_bits() {
    let heap = ArcMagrGC::new();
    for (v, kind) in one_of_each(&heap) {
        let w = encode(&v).expect("a heap reference encodes");
        assert_eq!(w & KIND_MASK, kind, "{v:?}");
        assert_eq!(handle_bits(w), payload(&v), "{v:?}: the handle bits are the register payload");
        let back = unsafe { decode(w) };
        assert_eq!(discriminant(&back), discriminant(&v), "{v:?}");
        assert_eq!(payload(&back), payload(&v), "{v:?}: same handle");
        let traced = unsafe { decode_for_trace(w) };
        assert_eq!(payload(&traced), payload(&v), "{v:?}: tracing sees the same handle");
    }
}

#[test]
fn null_is_the_zero_word() {
    assert_eq!(encode(&Value::Null), Some(0));
    assert!(matches!(unsafe { decode(0) }, Value::Null));
    assert!(matches!(unsafe { decode_for_trace(0) }, Value::Null));
}

/// The JIT decodes a word by looking its kind up in `KIND_TO_VALUE_TAG`; the table must name
/// exactly the discriminant `decode` produces, for every kind including `null`.
#[test]
fn the_jit_kind_table_matches_decode() {
    assert_eq!(KIND_TO_VALUE_TAG[0], discriminant(&Value::Null));
    let heap = ArcMagrGC::new();
    for (v, kind) in one_of_each(&heap) {
        let w = encode(&v).expect("encodes");
        assert_eq!(KIND_TO_VALUE_TAG[kind as usize], discriminant(&unsafe { decode(w) }), "{v:?}");
    }
    assert_eq!(KIND_TO_VALUE_TAG[RK_BOXED_VALUE as usize], SLOW_PATH_TAG, "kind 7 goes to the helper");
}

#[test]
fn values_that_do_not_fit_eight_bytes_need_a_box() {
    for v in [Value::I64(7), Value::F64(1.5), Value::Bool(true), Value::Char('z'),
              Value::StackObject { idx: 1, frame_id: 2 }, Value::StructRef { idx: 0, frame_id: 1 }] {
        assert_eq!(encode(&v), None, "{v:?}");
    }
}

/// A raw primitive reaching an `object` field (erased generics) is boxed on store and comes
/// back unchanged; the tracer sees the box.
#[test]
fn a_boxed_primitive_reads_back_unchanged_and_traces_as_its_box() {
    let heap = ArcMagrGC::new();
    let _g = crate::gc::ambient::HeapGuard::enter(&heap);
    for v in [Value::I64(-42), Value::F64(2.25), Value::Bool(true), Value::Char('é')] {
        let b = alloc_box_ambient(&v).expect("boxes");
        let w = encode_boxed(&b);
        assert_eq!(w & KIND_MASK, RK_BOXED_VALUE);
        assert_eq!(unsafe { decode(w) }, v, "user code sees the original value");
        assert!(matches!(unsafe { decode_for_trace(w) }, Value::Array(_)), "the tracer sees the box");
    }
}

#[test]
fn boxing_without_an_active_heap_is_an_error_not_a_crash() {
    assert!(alloc_box_ambient(&Value::I64(1)).is_err());
}

/// End to end through `ScriptObject`: a string, an object and a boxed int held only through
/// reference words survive a full collection, read back intact, and die once the fields drop.
#[test]
fn gc_traces_through_reference_words() {
    let heap = ArcMagrGC::new();
    heap.set_mode(GcMode::StwMarkSweep);
    let _g = crate::gc::ambient::HeapGuard::enter(&heap);

    let holder = heap.alloc_object(ref_cell_td(3), vec![], NativeData::None);
    let Value::Object(h) = &holder else { panic!() };
    let _root = heap.pin_root(holder.clone());
    {
        let child = heap.alloc_object(plain_td("Child"), vec![], NativeData::None);
        let s = Value::Str(heap.alloc_str("kept by a reference word"));
        let mut o = h.borrow_mut();
        assert!(matches!(o.try_set_field_value(0, &s), Ok(FieldWrite::Ref)));
        assert!(matches!(o.try_set_field_value(1, &child), Ok(FieldWrite::Ref)));
        assert!(matches!(o.try_set_field_value(2, &Value::I64(99)), Ok(FieldWrite::Boxed(_))),
            "a primitive into a reference word reports its box for the barrier");
    }
    let weak_child = heap.make_weak(&h.borrow().field_value(1)).expect("object");
    let mut edges = 0;
    h.borrow().visit_refs(&mut |_| edges += 1);
    assert_eq!(edges, 3, "visit_refs yields every non-null reference word");

    heap.force_collect();
    let o = h.borrow();
    match o.field_value(0) {
        Value::Str(s) => assert_eq!(s.as_str(), "kept by a reference word"),
        other => panic!("string field lost: {other:?}"),
    }
    assert!(heap.upgrade_weak(&weak_child).is_some(), "object reachable only through a word survives");
    assert_eq!(o.field_value(2), Value::I64(99), "boxed primitive survives");
    drop(o);

    {
        let mut o = h.borrow_mut();
        for i in 0..3 {
            assert!(matches!(o.try_set_field_value(i, &Value::Null), Ok(FieldWrite::Ref)));
        }
    }
    heap.force_collect();
    assert!(heap.upgrade_weak(&weak_child).is_none(), "cleared word drops the edge");
    assert!(matches!(h.borrow().field_value(1), Value::Null));
}

/// `clear_refs_for_sweep` zeroes every word (the sweep breaking a dead object's edges).
#[test]
fn clear_refs_for_sweep_zeroes_every_word() {
    let leaf = Value::Object(GcRef::new(ScriptObject::new(plain_td("Leaf"), ObjStorage::new(0, 0))));
    let holder = GcRef::new(ScriptObject::new(ref_cell_td(2), ObjStorage::new(16, 0)));
    holder.borrow_mut().set_field_value(0, &leaf);
    holder.borrow_mut().set_field_value(1, &leaf);
    holder.borrow_mut().clear_refs_for_sweep();
    let mut edges = 0;
    holder.borrow().visit_refs(&mut |_| edges += 1);
    assert_eq!(edges, 0);
    assert!(holder.borrow().bytes().iter().all(|&b| b == 0));
}
