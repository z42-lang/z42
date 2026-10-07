//! 型参字段的 16 B 单元（R2）：每种实例化定下的标签、各方向的不符写入（慢路）、负载字只装一种基元、
//! GC 经标签字追踪。SATB 的那一对测试在 `gc/arc_heap_tests/incremental.rs`。

use super::*;
use crate::gc::arc_heap::ArcMagrGC;
use crate::gc::heap::MagrGC;
use crate::gc::GcMode;
use crate::metadata::bytecode::ObjectLayoutDesc;
use crate::metadata::types::{
    compose_object_layout, generic_field_zero_overrides, FieldCell, FieldSlot, NativeData, ScriptObject,
    TypeDesc, TypeDescCold, STRUCT_LEAF_GCREF_CLOSURE, STRUCT_LEAF_PRIM,
};
use std::cell::Cell;
use std::sync::Arc;

thread_local! {
    static BOXES: Cell<usize> = const { Cell::new(0) };
}

/// Called by `box_value` (test builds): the slow path's allocation count on this thread.
pub(crate) fn count_box() {
    BOXES.with(|b| b.set(b.get() + 1));
}

fn boxes() -> usize {
    BOXES.with(|b| b.get())
}

/// `class Box<T> { int N; T V0; … T V{n-1}; }` — an `int` at 0, then `n` type-parameter
/// fields at 8, 16, … as the compiler lays them out, composed by the real
/// `compose_object_layout`.
pub(crate) fn tparam_td(n: usize) -> Arc<TypeDesc> {
    let offs: Vec<u32> = std::iter::once(0).chain((0..n as u32).map(|i| 8 + 8 * i)).collect();
    let own = ObjectLayoutDesc {
        size: 8 + 8 * n as u32,
        field_offsets: offs.clone().into(),
        field_sizes: std::iter::once(4).chain(std::iter::repeat(8).take(n)).collect(),
        field_kinds: std::iter::once(STRUCT_LEAF_PRIM)
            .chain(std::iter::repeat(STRUCT_LEAF_GCREF_CLOSURE).take(n)).collect(),
        ref_offsets: offs[1..].into(),
        ref_kinds: vec![2; n].into(),
    };
    let fields: Vec<FieldSlot> = std::iter::once(FieldSlot { name: "N".into(), type_tag: "int".into(), visibility: 0 })
        .chain((0..n).map(|i| FieldSlot { name: format!("V{i}").into(), type_tag: "T".into(), visibility: 0 }))
        .collect();
    let type_params: Box<[String]> = Box::new(["T".to_string()]);
    let layout = Arc::new(compose_object_layout(None, &own, &fields, &type_params));
    let field_index = fields.iter().enumerate().map(|(i, f)| (f.name.to_string(), i)).collect();
    Arc::new(TypeDesc {
        class_flags: 0,
        visibility: 0,
        name: "Demo.Box".to_string(),
        base_name: None,
        fields,
        field_index,
        vtable: Vec::new(),
        vtable_index: crate::metadata::NameIndex::new(),
        cold: Some(Box::new(TypeDescCold {
            composed_object_layout: Some(layout),
            type_params,
            ..Default::default()
        })),
        id: crate::metadata::tokens::TypeId::UNRESOLVED,
    })
}

/// A fresh `Box<arg>` the way `ObjNew` makes it: zeroed storage, then the instantiation's
/// zero values written through `set_field_value` (`generic_field_zero_overrides`).
fn new_box(td: &Arc<TypeDesc>, arg: Option<&str>) -> ScriptObject {
    let mut o = ScriptObject::new(td.clone(), td.object_storage());
    if let Some(a) = arg {
        for (slot, zero) in generic_field_zero_overrides(td, &[a.to_string()]) {
            o.set_field_value(slot, &zero);
        }
    }
    o
}

fn cell(o: &ScriptObject, slot: usize) -> (u32, u32) {
    let fa = o.type_desc.composed_object_layout_ref().unwrap().field_access[slot];
    assert_eq!(fa.cell(), FieldCell::TypeParam);
    (fa.offset, fa.aux)
}

fn tag_word(o: &ScriptObject, slot: usize) -> u64 {
    o.storage.load_word(cell(o, slot).0 as usize, Ordering::Relaxed)
}

fn payload_word(o: &ScriptObject, slot: usize) -> u64 {
    o.storage.load_word(cell(o, slot).1 as usize, Ordering::Relaxed)
}

fn heap() -> ArcMagrGC {
    let h = ArcMagrGC::new();
    h.set_mode(GcMode::StwMarkSweep);
    h
}

#[test]
fn the_cell_is_the_fields_own_slot_plus_a_payload_word_past_the_compiler_layout() {
    let td = tparam_td(2);
    let l = td.composed_object_layout_ref().unwrap();
    assert_eq!(l.size, 24, "the compiler's size is kept (a subclass's region starts there)");
    assert_eq!(&*l.tparam_cells, &[8, 16]);
    assert!(l.ref_offsets.is_empty() && l.ref_cells.is_empty(), "no side table, no plain reference words");
    assert_eq!((l.field_access[1].offset, l.field_access[1].aux), (8, 24));
    assert_eq!((l.field_access[2].offset, l.field_access[2].aux), (16, 32));
    assert_eq!(l.bytes_len(), 40);
    assert_eq!(td.object_region_sizes(), (40, 0), "16 B per type-parameter field, no side-table slot");
}

/// The instantiation's zero value fixes the cell's variant at allocation; a reference or unknown
/// argument leaves it open (`0` = null).
#[test]
fn a_known_primitive_argument_fixes_the_variant_at_allocation() {
    let td = tparam_td(1);
    for (arg, word, zero) in [
        ("int", prim_word(K_I64), Value::I64(0)),
        ("long", prim_word(K_I64), Value::I64(0)),
        ("double", prim_word(K_F64), Value::F64(0.0)),
        ("bool", prim_word(K_BOOL), Value::Bool(false)),
        ("char", prim_word(K_CHAR), Value::Char('\0')),
    ] {
        let o = new_box(&td, Some(arg));
        assert_eq!(tag_word(&o, 1), word, "Box<{arg}>");
        assert_eq!(o.field_value(1), zero, "Box<{arg}>.V reads the instantiation's zero");
    }
    for arg in [Some("string"), Some("Demo.Node"), Some("T"), None] {
        let o = new_box(&td, arg);
        assert_eq!(tag_word(&o, 1), 0, "Box<{arg:?}>: variant still open");
        assert_eq!(o.field_value(1), Value::Null);
    }
}

#[test]
fn primitives_of_the_fixed_variant_round_trip_through_the_payload_word() {
    let td = tparam_td(1);
    let before = boxes();
    let mut o = new_box(&td, Some("double"));
    for x in [1.5, -0.0, f64::MAX, f64::NAN] {
        assert!(matches!(o.try_set_field_value(1, &Value::F64(x)), Ok(FieldWrite::NotRef)));
        let Value::F64(back) = o.field_value(1) else { panic!() };
        assert_eq!(back.to_bits(), x.to_bits());
        assert_eq!(tag_word(&o, 1), prim_word(K_F64), "only the payload changes");
    }
    let mut o = new_box(&td, Some("char"));
    o.set_field_value(1, &Value::Char('é'));
    assert_eq!(o.field_value(1), Value::Char('é'));
    let mut o = new_box(&td, Some("long"));
    o.set_field_value(1, &Value::I64(i64::MIN));
    assert_eq!(o.field_value(1), Value::I64(i64::MIN));
    assert_eq!(boxes(), before, "no allocation on the fast path");
}

/// An instance allocated without usable type arguments (erased `new Node<T>()`) fixes the
/// variant on its first primitive store and stays on the fast path from then on.
#[test]
fn the_first_primitive_store_claims_the_variant_when_allocation_did_not_know_it() {
    let td = tparam_td(1);
    let before = boxes();
    let mut o = new_box(&td, Some("T"));
    o.set_field_value(1, &Value::I64(41));
    assert_eq!(tag_word(&o, 1), prim_word(K_I64));
    o.set_field_value(1, &Value::I64(42));
    assert_eq!(o.field_value(1), Value::I64(42));
    assert_eq!(boxes(), before);
}

/// `T = int` receiving `null` (and back): no allocation, the variant stays fixed.
#[test]
fn null_into_a_primitive_cell_keeps_the_variant() {
    let td = tparam_td(1);
    let before = boxes();
    let mut o = new_box(&td, Some("int"));
    o.set_field_value(1, &Value::I64(7));
    assert!(matches!(o.try_set_field_value(1, &Value::Null), Ok(FieldWrite::NotRef)));
    assert_eq!(tag_word(&o, 1), null_word(K_I64));
    assert_eq!(o.field_value(1), Value::Null);
    o.set_field_value(1, &Value::I64(8));
    assert_eq!(tag_word(&o, 1), prim_word(K_I64));
    assert_eq!(o.field_value(1), Value::I64(8));
    assert_eq!(boxes(), before);
}

/// `T = string` (or unknown) holding references: the reference is the tag word itself.
#[test]
fn references_live_in_the_tag_word_without_a_box() {
    let heap = heap();
    let _g = crate::gc::ambient::HeapGuard::enter(&heap);
    let td = tparam_td(1);
    let before = boxes();
    let mut o = new_box(&td, Some("string"));
    let s = Value::Str(heap.alloc_str("x"));
    assert!(matches!(o.try_set_field_value(1, &s), Ok(FieldWrite::Ref)));
    assert_eq!(tag_word(&o, 1), ref_word::encode(&s).unwrap());
    assert_eq!(o.field_value(1), s);
    assert!(matches!(o.try_set_field_value(1, &Value::Null), Ok(FieldWrite::NotRef)));
    assert_eq!(tag_word(&o, 1), 0, "a cell that never held a primitive goes back to plain null");
    // ... so its variant is still open.
    o.set_field_value(1, &Value::Bool(true));
    assert_eq!(o.field_value(1), Value::Bool(true));
    assert_eq!(boxes(), before);
}

/// Mismatched writes in each direction read back unchanged through the boxed slow path.
#[test]
fn mismatched_writes_take_the_boxed_slow_path_and_read_back_unchanged() {
    let heap = heap();
    let _g = crate::gc::ambient::HeapGuard::enter(&heap);
    let td = tparam_td(1);
    let s = Value::Str(heap.alloc_str("s"));

    // Fixed `int`, then a reference, another primitive variant, the fixed variant again, null.
    let mut o = new_box(&td, Some("int"));
    let before = boxes();
    assert!(matches!(o.try_set_field_value(1, &s), Ok(FieldWrite::Boxed(_))));
    assert_eq!(o.field_value(1), s);
    assert!(matches!(o.try_set_field_value(1, &Value::F64(2.5)), Ok(FieldWrite::Boxed(_))));
    assert_eq!(o.field_value(1), Value::F64(2.5));
    o.set_field_value(1, &Value::I64(3));
    assert_eq!(o.field_value(1), Value::I64(3));
    assert!(matches!(o.try_set_field_value(1, &Value::Null), Ok(FieldWrite::NotRef)));
    assert_eq!(tag_word(&o, 1), BOXED_NULL, "null after a box: no allocation");
    assert_eq!(o.field_value(1), Value::Null);
    assert_eq!(boxes() - before, 3, "string, double, then the int (the variant is forgotten once boxed)");

    // Never held a primitive: a reference, then a primitive (claiming would hide the reference
    // from a racing reader).
    let mut o = new_box(&td, None);
    o.set_field_value(1, &s);
    let before = boxes();
    assert!(matches!(o.try_set_field_value(1, &Value::I64(9)), Ok(FieldWrite::Boxed(_))));
    assert_eq!(o.field_value(1), Value::I64(9));
    assert_eq!(boxes() - before, 1);

    // A value no 8 B word encodes (a stack handle) is boxed too.
    let mut o = new_box(&td, None);
    let h = Value::StackObject { idx: 3, frame_id: 4 };
    assert!(matches!(o.try_set_field_value(1, &h), Ok(FieldWrite::Boxed(_))));
    assert_eq!(o.field_value(1), h);
}

/// The invariant that makes the cell tear-free: once claimed, the payload word only ever holds
/// bits of that variant — every other value goes to the tag word.
#[test]
fn the_payload_word_only_ever_holds_the_claimed_variant() {
    let heap = heap();
    let _g = crate::gc::ambient::HeapGuard::enter(&heap);
    let td = tparam_td(1);
    let mut o = new_box(&td, None);
    o.set_field_value(1, &Value::I64(5));
    let s = Value::Str(heap.alloc_str("s"));
    for v in [s, Value::F64(1.0), Value::Bool(true), Value::Null, Value::Char('c'), s] {
        o.set_field_value(1, &v);
        assert_eq!(payload_word(&o, 1), 5, "{v:?} must not touch the payload word");
        assert_eq!(o.field_value(1), v);
    }
    // Once boxed the cell no longer knows its variant, so even an `int` stays in the tag word.
    o.set_field_value(1, &Value::I64(6));
    assert_eq!(payload_word(&o, 1), 5);
    assert_eq!(o.field_value(1), Value::I64(6));
    // Through `null` the variant is still known: the payload word takes the next `int`.
    let mut o = new_box(&td, Some("int"));
    o.set_field_value(1, &Value::Null);
    o.set_field_value(1, &Value::I64(6));
    assert_eq!(payload_word(&o, 1), 6);
}

/// GC traces a type-parameter cell through its tag word: a direct reference and a box survive
/// a collection, a primitive is no edge, and clearing the fields drops the referents.
#[test]
fn gc_traces_through_type_parameter_cells() {
    let heap = heap();
    let _g = crate::gc::ambient::HeapGuard::enter(&heap);
    let holder = heap.alloc_object(tparam_td(3), vec![], NativeData::None);
    let Value::Object(h) = &holder else { panic!() };
    let _root = heap.pin_root(holder.clone());
    let child = heap.alloc_object(tparam_td(0), vec![], NativeData::None);
    let weak_child = heap.make_weak(&child).expect("object");
    {
        let s = Value::Str(heap.alloc_str("kept by a tag word"));
        let mut o = h.borrow_mut();
        o.set_field_value(1, &s);
        o.set_field_value(2, &Value::I64(1));
        o.set_field_value(2, &child); // claimed int, then a reference: boxed
        o.set_field_value(3, &Value::I64(77));
    }
    drop(child);
    let mut edges = 0;
    h.borrow().visit_refs(&mut |_| edges += 1);
    assert_eq!(edges, 2, "the string and the box; the int is no edge");

    heap.force_collect();
    {
        let o = h.borrow();
        match o.field_value(1) {
            Value::Str(s) => assert_eq!(s.as_str(), "kept by a tag word"),
            other => panic!("string lost: {other:?}"),
        }
        assert!(heap.upgrade_weak(&weak_child).is_some(), "reachable only through a box in a tag word");
        assert_eq!(o.field_value(3), Value::I64(77));
    }
    {
        let mut o = h.borrow_mut();
        for i in 1..=3 {
            o.set_field_value(i, &Value::Null);
        }
    }
    heap.force_collect();
    assert!(heap.upgrade_weak(&weak_child).is_none(), "cleared cell drops the edge");
}

#[test]
fn clear_refs_for_sweep_breaks_type_parameter_edges() {
    let heap = heap();
    let _g = crate::gc::ambient::HeapGuard::enter(&heap);
    let leaf = heap.alloc_object(tparam_td(0), vec![], NativeData::None);
    let mut o = new_box(&tparam_td(2), None);
    o.set_field_value(1, &leaf);
    o.set_field_value(2, &Value::I64(4));
    o.clear_refs_for_sweep();
    let mut edges = 0;
    o.visit_refs(&mut |_| edges += 1);
    assert_eq!(edges, 0);
    assert_eq!(o.field_value(2), Value::I64(4), "a primitive is not an edge and stays");
}

/// A subclass re-places the inherited cells' payload words past its own (larger) layout.
#[test]
fn a_subclass_moves_inherited_payload_words_past_its_own_fields() {
    let base_td = tparam_td(1);
    let base = base_td.composed_object_layout_ref().unwrap();
    let own = ObjectLayoutDesc {
        size: 8,
        field_offsets: Box::new([0]),
        field_sizes: Box::new([8]),
        field_kinds: Box::new([STRUCT_LEAF_GCREF_CLOSURE]),
        ref_offsets: Box::new([0]),
        ref_kinds: Box::new([2]),
    };
    let mut fields = base_td.fields.clone();
    fields.push(FieldSlot { name: "W".into(), type_tag: "U".into(), visibility: 0 });
    let sub = compose_object_layout(Some(base), &own, &fields, &["U".to_string()]);
    assert_eq!(sub.size, 24, "compiler size: base 16 + own 8");
    assert_eq!(&*sub.tparam_cells, &[8, 16]);
    assert_eq!(sub.field_access[1].aux, 24, "inherited cell's payload past the subclass layout");
    assert_eq!(sub.field_access[2].aux, 32);
    assert_eq!(sub.bytes_len(), 40);
}
