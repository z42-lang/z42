//! The object access contract works with a Heap, without a VM or execution frame.
use crate::gc::{Heap, heap::{BarrierEvent, BarrierObserver}};
use crate::object::*;
use std::sync::Arc;

fn fields() -> Arc<TypeDesc> {
    let fields = vec![
        FieldSlot { name: "child".into(), type_tag: "object".into(), visibility: 0 },
        FieldSlot { name: "number".into(), type_tag: "int".into(), visibility: 0 },
    ];
    let field_index = fields.iter().enumerate().map(|(i, f)| (f.name.to_string(), i)).collect();
    Arc::new(TypeDesc {
        class_flags: 0, visibility: 0, name: "AccessTest".into(), base_name: None,
        fields, field_index, vtable: Vec::new(), vtable_index: Default::default(),
        cold: None, id: crate::metadata::ids::tokens::TypeId::UNRESOLVED,
    })
}

#[test]
fn field_access_completes_barrier_and_preserves_rejected_storage_without_vm() {
    let heap = Heap::default();
    let observer = Arc::new(BarrierObserver::new());
    heap.install_barrier_observer(observer.clone());
    let Value::Object(owner) = heap.alloc_object(fields(), vec![], NativeData::None) else { panic!("object") };
    let child = heap.alloc_object(fields(), vec![], NativeData::None);
    store_field_slot(&heap, &owner, 0, &child).unwrap();
    store_field_slot(&heap, &owner, 1, &Value::I64(17)).unwrap();
    assert!(store_field_slot(&heap, &owner, 1, &child).is_err());
    store_field_slot(&heap, &owner, usize::MAX, &child).unwrap();
    assert!(matches!(owner.borrow().field_value(1), Value::I64(17)));
    let events = observer.events();
    assert_eq!(events.len(), 1, "only the successfully stored reference needs a barrier");
    assert!(matches!(events[0], BarrierEvent::Field { new_is_heap: true, .. }));
}

#[test]
fn array_access_keeps_typed_and_untyped_contracts_without_vm() {
    let heap = Heap::default();
    let observer = Arc::new(BarrierObserver::new());
    heap.install_barrier_observer(observer.clone());
    let Value::Array(array) = ArrayObj::alloc_filled(&heap, ElemType::intern("int"), ElemKind::I32, 1, Value::I64(9)) else { panic!("array") };
    assert!(try_store_array_element(&heap, &array, 0, &Value::Null).is_err());
    assert!(matches!(array.borrow().get_boxed(0), Value::I64(9)));
    store_array_element(&heap, &array, 0, &Value::I64(21));
    assert!(matches!(array.borrow().get_boxed(0), Value::I64(21)));
    assert!(observer.events().is_empty());
}

#[test]
fn struct_copy_tracks_leaves_and_snapshots_independent_storage_without_vm() {
    let heap = Heap::default();
    let observer = Arc::new(BarrierObserver::new());
    heap.install_barrier_observer(observer.clone());
    let layout = Arc::new(StructTypeLayout { float_field_ranges: Default::default(),
        size: 8, ref_offsets: Box::new([0]), ref_kinds: Box::new([STRUCT_REF_GCREF]), fields: Box::new([]),
    });
    let td = Arc::new(TypeDesc {
        class_flags: crate::zpkg::ir::CLASS_FLAG_STRUCT, visibility: 0, name: "AccessStruct".into(), base_name: None,
        fields: Vec::new(), field_index: Default::default(), vtable: Vec::new(), vtable_index: Default::default(),
        cold: Some(Box::new(TypeDescCold { struct_layout: Some(layout.clone()), ..Default::default() })),
        id: crate::metadata::ids::tokens::TypeId::UNRESOLVED,
    });
    let child = heap.alloc_object(fields(), vec![], NativeData::None);
    let boxed = box_struct(&heap, td, &[0; 8], &[child]).unwrap();
    let Value::BoxedStruct(object) = boxed else { panic!("box") };
    let Value::Array(array) = ArrayObj::alloc_struct(&heap, ElemType::intern("AccessStruct"), 1, layout) else { panic!("array") };
    try_store_array_element(&heap, &array, 0, &boxed).unwrap();
    let snapshot = snapshot_struct_element(&StructArrayElem { arr: array, index: 0 }).unwrap();
    store_struct_leaf(&heap, &object, 0, TAG_OBJECT, &Value::Null).unwrap();
    assert!(matches!(snapshot_struct_box(&object).2[0], Value::Null));
    let Value::Object(expected) = child else { panic!("child") };
    let Value::Object(actual) = snapshot.1[0] else { panic!("snapshot child") };
    assert!(ObjRef::ptr_eq(&expected, &actual));
    assert!(matches!(array.borrow().struct_elem_ref(0, 0).unwrap(), Value::Object(_)));
    assert_eq!(&*snapshot.3, "AccessStruct");
    let events = observer.events();
    assert_eq!(events.len(), 1, "fresh boxing and clearing a leaf need no insertion barrier");
    assert!(matches!(events[0], BarrierEvent::ArrayElem { idx: 0, new_is_heap: true, .. }));
}
