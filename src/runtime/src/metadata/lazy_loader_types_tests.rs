//! P1-2 PR 6: the loader publishes its type registry to the VM's `TypeTable` — new
//! descriptors on package load, fixed-up versions after the inheritance fixup.

use super::*;
use crate::metadata::bytecode::{ClassDesc, FieldDesc, Module};
use crate::metadata::type_table::TypeTable;

fn class(name: &str, base: Option<&str>, fields: &[&str]) -> ClassDesc {
    ClassDesc {
        static_fields: vec![].into(),
        interfaces: vec![].into(),
        enum_members: vec![].into(),
        iface_methods: vec![].into(),
        struct_layout: None,
        inline_layout: None,
        object_layout: None,
        class_flags: 0,
        visibility: 0,
        class_flags2: 0,
        struct_field_table: Box::new([]),
        name: name.to_owned(),
        base_class: base.map(str::to_owned),
        fields: fields.iter().map(|f| FieldDesc {
            name: (*f).to_owned(), type_tag: "long".to_owned(), attributes: Box::new([]), visibility: 0,
        }).collect(),
        type_params: Box::new([]),
        type_param_constraints: Box::new([]),
        attributes: Box::new([]),
    }
}

fn artifact(module_name: &str, classes: Vec<ClassDesc>) -> crate::metadata::loader::LoadedArtifact {
    let mut module = Module {
        name: module_name.to_owned(),
        string_pool: vec![],
        classes,
        functions: vec![],
        type_registry: FxHashMap::default(),
        func_index: FxHashMap::default(),
    };
    crate::metadata::loader::build_type_registry(&mut module);
    crate::metadata::loader::LoadedArtifact {
        module,
        entry_hint: None,
        dependencies: vec![],
        import_namespaces: vec![],
        test_index: vec![],
        impl_pairs: vec![],
        package_name: Some(module_name.to_lowercase()),
    }
}

fn loader_with(table: &Arc<TypeTable>) -> LazyLoader {
    let mut loader = LazyLoader::new(Vec::new(), 0, Vec::new(), Vec::new());
    loader.set_type_table(Arc::clone(table));
    loader
}

fn slot_is_registry_entry(table: &TypeTable, loader: &LazyLoader, name: &str) -> bool {
    let td = loader.loaded_type(name).expect("registered");
    table.get(td.id).is_some_and(|t| std::ptr::eq(t, Arc::as_ptr(&td)))
}

#[test]
fn package_types_are_published_with_their_ids() {
    let table = Arc::new(TypeTable::default());
    let mut loader = loader_with(&table);
    loader.register_loaded_artifact(artifact("P1", vec![class("P1.A", None, &["x"]), class("P1.B", Some("P1.A"), &[])]))
        .expect("register p1");
    assert_eq!(table.len(), 2);
    assert!(slot_is_registry_entry(&table, &loader, "P1.A"));
    assert!(slot_is_registry_entry(&table, &loader, "P1.B"));
}

/// A later package with a type of the same name is not registered (the loader keeps the
/// first), so its id stays empty in the table and the first one's slot is untouched.
#[test]
fn a_duplicate_type_name_keeps_the_first_id() {
    let table = Arc::new(TypeTable::default());
    let mut loader = loader_with(&table);
    loader.register_loaded_artifact(artifact("P1", vec![class("Ns.T", None, &[])])).expect("p1");
    let first = loader.loaded_type("Ns.T").unwrap();
    let dup = artifact("P2", vec![class("Ns.T", None, &["y"])]);
    let dup_id = dup.module.type_registry["Ns.T"].id;
    loader.register_loaded_artifact(dup).expect("p2");
    assert!(std::ptr::eq(loader.loaded_type("Ns.T").unwrap().as_ref(), first.as_ref()));
    assert!(table.get(dup_id).is_none(), "the losing duplicate is never published");
    assert!(std::ptr::eq(table.get(first.id).unwrap(), Arc::as_ptr(&first)));
    assert_eq!(table.len(), 1);
}

/// A subclass loaded before its cross-package base is published own-only; when the base
/// arrives, the fixup replaces it (same id) and the slot follows — ids never move.
#[test]
fn the_fixed_up_version_replaces_the_own_only_one() {
    let table = Arc::new(TypeTable::default());
    let mut loader = loader_with(&table);
    loader.register_loaded_artifact(artifact("Sub", vec![class("Sub.D", Some("Base.B"), &["own"])]))
        .expect("sub");
    let own_only = loader.loaded_type("Sub.D").unwrap();
    assert_eq!(own_only.fields.len(), 1);
    let id = own_only.id;
    assert!(std::ptr::eq(table.get(id).unwrap(), Arc::as_ptr(&own_only)));

    loader.register_loaded_artifact(artifact("Base", vec![class("Base.B", None, &["inherited"])]))
        .expect("base");
    let merged = loader.loaded_type("Sub.D").unwrap();
    assert_eq!(merged.id, id, "the fixup keeps the id");
    assert_eq!(merged.fields.len(), 2, "base field merged in");
    assert!(std::ptr::eq(table.get(id).unwrap(), Arc::as_ptr(&merged)), "slot repointed at the merged copy");
    assert_eq!(table.len(), 2);
}
