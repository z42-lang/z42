use super::*;
use crate::metadata::name_index::NameIndex;
use crate::metadata::tokens::UNRESOLVED;

fn desc(name: &str, id: TypeId) -> Arc<TypeDesc> {
    Arc::new(TypeDesc {
        name: name.to_string(),
        id,
        base_name: None,
        fields: Vec::new(),
        field_index: NameIndex::new(),
        vtable: Vec::new(),
        vtable_index: NameIndex::new(),
        class_flags: 0,
        visibility: 0,
        cold: None,
    })
}

fn fresh_id() -> TypeId {
    TypeId(alloc_type_id_block(1))
}

fn entry_module(types: &[Arc<TypeDesc>]) -> Module {
    Module {
        name: "Entry".to_string(),
        string_pool: vec![],
        classes: vec![],
        functions: vec![],
        type_registry: types.iter().map(|t| (t.name.clone(), Arc::clone(t))).collect(),
        func_index: FxHashMap::default(),
    }
}

#[test]
fn entry_types_are_registered_at_construction() {
    let a = desc("App.A", fresh_id());
    let b = desc("App.B", fresh_id());
    let t = TypeTable::new(Some(&entry_module(&[Arc::clone(&a), Arc::clone(&b)])));
    assert_eq!(t.len(), 2);
    assert!(std::ptr::eq(t.get(a.id).unwrap(), Arc::as_ptr(&a)));
    assert!(std::ptr::eq(t.get(b.id).unwrap(), Arc::as_ptr(&b)));
    assert!(t.get(fresh_id()).is_none(), "an id this table never registered");
    assert!(t.get(TypeId(UNRESOLVED)).is_none());
}

#[test]
fn transient_descriptors_are_not_registered() {
    let t = TypeTable::default();
    assert!(!t.publish(&desc("Local", TypeId::UNRESOLVED)));
    assert!(t.is_empty());
}

#[test]
fn republishing_the_same_descriptor_is_a_no_op() {
    let t = TypeTable::default();
    let a = desc("Pkg.A", fresh_id());
    assert!(t.publish(&a));
    assert!(!t.publish(&a));
    assert_eq!(t.len(), 1);
}

/// The inheritance fixup replaces a registry entry with a merged copy (same id, same name):
/// the slot follows it, and a reference to the previous version stays valid.
#[test]
fn a_newer_version_of_the_same_type_repoints_the_slot() {
    let t = TypeTable::default();
    let id = fresh_id();
    let own_only = desc("Pkg.Derived", id);
    t.publish(&own_only);
    let before = t.get(id).unwrap();

    let mut merged = (*own_only).clone();
    merged.base_name = Some("Pkg.Base".to_string());
    let merged = Arc::new(merged);
    drop(own_only);
    assert!(t.publish(&merged));
    assert_eq!(t.len(), 1, "same id, still one registered type");
    assert!(std::ptr::eq(t.get(id).unwrap(), Arc::as_ptr(&merged)));
    assert_eq!(before.name, "Pkg.Derived", "the previous version is kept alive");
    assert_eq!(before.base_name, None);
}

#[test]
fn a_different_type_under_a_taken_id_is_refused_first_wins() {
    let t = TypeTable::default();
    let id = fresh_id();
    let first = desc("Pkg.First", id);
    t.publish(&first);
    assert!(!t.publish(&desc("Pkg.Second", id)));
    assert!(std::ptr::eq(t.get(id).unwrap(), Arc::as_ptr(&first)));
}

/// Ids are process-global, so one VM sees a sparse subset; far-apart ids cost one segment each.
#[test]
fn sparse_ids_far_apart() {
    let t = TypeTable::default();
    let lo = desc("Pkg.Lo", TypeId(3));
    let hi = desc("Pkg.Hi", TypeId(1 << 16));
    t.publish(&lo);
    t.publish(&hi);
    assert_eq!(t.get(TypeId(3)).unwrap().name, "Pkg.Lo");
    assert_eq!(t.get(TypeId(1 << 16)).unwrap().name, "Pkg.Hi");
    assert!(t.get(TypeId(4)).is_none());
}

#[test]
fn name_keys_are_stable_distinct_and_never_a_registered_id() {
    let t = TypeTable::default();
    let a = desc("Pkg.A", fresh_id());
    t.publish(&a);
    let k1 = t.name_key("Demo.GBox");
    assert_eq!(t.name_key("Demo.GBox"), k1, "stable per name");
    let k2 = t.name_key("Demo.GBox$1");
    assert_ne!(k1, k2, "distinct per name");
    assert_ne!(k1, a.id.0);
    assert!(t.get(TypeId(k1)).is_none(), "a reserved key is never given to a descriptor");
    // Another VM reserves its own key for the same name — both stay unique process-wide.
    let other = TypeTable::default();
    assert_ne!(other.name_key("Demo.GBox"), k1);
}

#[test]
fn concurrent_publish_and_read() {
    let t = Arc::new(TypeTable::default());
    let descs: Vec<Arc<TypeDesc>> = (0..256).map(|i| desc(&format!("P.T{i}"), fresh_id())).collect();
    std::thread::scope(|s| {
        for chunk in descs.chunks(64) {
            let t = Arc::clone(&t);
            s.spawn(move || for d in chunk { t.publish(d); });
        }
        let t = Arc::clone(&t);
        let descs = &descs;
        s.spawn(move || {
            for _ in 0..1000 {
                for d in descs {
                    if let Some(got) = t.get(d.id) { assert_eq!(got.name, d.name); }
                }
            }
        });
    });
    assert_eq!(t.len(), 256);
}
