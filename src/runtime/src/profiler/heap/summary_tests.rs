use super::*;
use crate::object::{FieldSlot, NativeData, TypeDesc, Value};
use std::sync::Arc;

/// Four reference fields provide real slots for nested-object reachability.
fn dummy_type_desc(name: &str) -> Arc<TypeDesc> {
    let fields: Vec<FieldSlot> = (0..4)
        .map(|i| FieldSlot {
            name: format!("f{i}").into(),
            type_tag: "object".into(),
            visibility: 0,
        })
        .collect();
    let field_index: crate::metadata::NameIndex = fields
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.to_string(), i))
        .collect();
    Arc::new(TypeDesc {
        class_flags: 0,
        visibility: 0,
        name: name.to_string(),
        base_name: None,
        fields,
        field_index,
        vtable: Vec::new(),
        vtable_index: crate::metadata::NameIndex::new(),
        cold: None,
        id: crate::vm::runtime::ids::tokens::TypeId::UNRESOLVED,
    })
}

#[test]
fn snapshot_coverage_is_full_after_phase_3b_registry() {
    // Phase 3b: heap registry 升级 coverage 到 Full（不依赖 host pin）。
    let heap = Heap::default();
    let snap = take_snapshot(&heap);
    assert_eq!(snap.coverage, SnapshotCoverage::Full);
}

#[test]
fn snapshot_includes_unpinned_alive_object() {
    // Phase 3b: 没 pin 也能在 snapshot 里看到（只要还存活）。
    let heap = Heap::default();
    let _alive = heap.alloc_object(dummy_type_desc("UnpinnedFoo"), vec![], NativeData::None);
    let snap = take_snapshot(&heap);
    assert!(snap.objects_by_type.contains_key("UnpinnedFoo"));
    assert!(snap.total_objects >= 1);
}

#[test]
fn snapshot_includes_pinned_root_object() {
    let heap = Heap::default();
    let v = heap.alloc_object(dummy_type_desc("Foo"), vec![], NativeData::None);
    let _h = heap.pin_root(v);
    let snap = take_snapshot(&heap);
    assert!(snap.total_objects >= 1);
    assert!(snap.objects_by_type.contains_key("Foo"));
}

#[test]
fn snapshot_traverses_nested_objects() {
    let heap = Heap::default();
    let inner1 = heap.alloc_object(dummy_type_desc("Inner"), vec![], NativeData::None);
    let inner2 = heap.alloc_object(dummy_type_desc("Inner"), vec![], NativeData::None);
    let outer = heap.alloc_object(
        dummy_type_desc("Outer"),
        vec![inner1, inner2],
        NativeData::None,
    );
    let _h = heap.pin_root(outer);
    let snap = take_snapshot(&heap);
    assert!(snap.total_objects >= 3);
    assert_eq!(snap.objects_by_type.get("Outer").map(|s| s.count), Some(1));
    assert_eq!(snap.objects_by_type.get("Inner").map(|s| s.count), Some(2));
}
