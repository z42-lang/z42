//! Unit tests for the load-context registry (add-load-context-model).
//!
//! Covers the core semantics backing the z42-facing API: root vs collectible
//! contexts, `load_into` assembly registration, collectibility propagation, and
//! the (sorted) `assembly_types` reflection data path. The z42-level API
//! (`Default()` / `CreateCollectible` / `Name` / `IsCollectible` / `Unload`
//! throws) is exercised end-to-end by the golden test
//! `src/tests/reflection/load_context/`.

use crate::metadata::bytecode::Module;
use crate::metadata::context::{AssemblyId, ContextId, ContextRegistry};
use crate::metadata::tokens::TypeId;
use crate::metadata::types::TypeDesc;
use crate::metadata::NameIndex;
use std::collections::HashMap;
use std::sync::Arc;

fn dummy_type_desc(name: &str) -> Arc<TypeDesc> {
    Arc::new(TypeDesc {
        class_flags: 0,
        name: name.to_string(),
        base_name: None,
        fields: Vec::new(),
        field_index: NameIndex::new(),
        vtable: Vec::new(),
        vtable_index: NameIndex::new(),
        cold: None,
        id: TypeId::UNRESOLVED,
    })
}

fn mk_module(type_names: &[&str]) -> Module {
    let mut type_registry = HashMap::new();
    for n in type_names {
        type_registry.insert(n.to_string(), dummy_type_desc(n));
    }
    Module {
        name: "dep".to_string(),
        string_pool: Vec::new(),
        classes: Vec::new(),
        functions: Vec::new(),
        type_registry,
        type_registry_vec: Vec::new(),
        func_index: HashMap::new(),
        func_ref_cache_slots: 0,
        interned_strings: Vec::new(),
    }
}

#[test]
fn root_context_and_assembly_are_not_collectible() {
    let r = ContextRegistry::new();
    assert!(r.context_exists(ContextId::ROOT));
    assert_eq!(r.context_name(ContextId::ROOT).as_deref(), Some("root"));
    assert_eq!(r.context_is_collectible(ContextId::ROOT), Some(false));
    assert_eq!(r.assembly_name(AssemblyId::ROOT).as_deref(), Some("root"));
    assert!(!r.assembly_is_collectible(AssemblyId::ROOT));
    assert!(r.assembly_types(AssemblyId::ROOT).is_empty());
}

#[test]
fn create_collectible_is_collectible_and_distinct() {
    let mut r = ContextRegistry::new();
    let cid = r.create_collectible("plugin-a");
    assert_ne!(cid, ContextId::ROOT);
    assert_eq!(r.context_name(cid).as_deref(), Some("plugin-a"));
    assert_eq!(r.context_is_collectible(cid), Some(true));
    assert!(r.context_assemblies(cid).is_empty());
}

#[test]
fn load_into_collectible_registers_sorted_reflectable_assembly() {
    let mut r = ContextRegistry::new();
    let cid = r.create_collectible("plugin-a");
    let aid = r.load_into(cid, "dep", mk_module(&["Demo.Zebra", "Demo.Apple"]));

    assert_ne!(aid, AssemblyId::ROOT);
    assert_eq!(r.assembly_name(aid).as_deref(), Some("dep"));
    assert_eq!(r.assembly_context(aid), Some(cid));
    // collectibility propagates from the owning context
    assert!(r.assembly_is_collectible(aid));
    assert_eq!(r.context_assemblies(cid), vec![aid]);

    // GetTypes order is deterministic (sorted by FQ name), not HashMap order
    let names: Vec<String> = r.assembly_types(aid).iter().map(|t| t.name.clone()).collect();
    assert_eq!(names, vec!["Demo.Apple".to_string(), "Demo.Zebra".to_string()]);
}

#[test]
fn load_into_root_yields_non_collectible_assembly() {
    let mut r = ContextRegistry::new();
    let aid = r.load_into(ContextId::ROOT, "extra", mk_module(&[]));
    assert!(!r.assembly_is_collectible(aid)); // owning context = root
    // root context now lists the pre-populated root assembly + this one
    assert_eq!(r.context_assemblies(ContextId::ROOT).len(), 2);
}

#[test]
fn multiple_collectible_contexts_are_independent() {
    let mut r = ContextRegistry::new();
    let a = r.create_collectible("a");
    let b = r.create_collectible("b");
    assert_ne!(a, b);
    let aid = r.load_into(a, "dep-a", mk_module(&["A.T"]));
    assert_eq!(r.context_assemblies(a), vec![aid]);
    assert!(r.context_assemblies(b).is_empty());
}

#[test]
fn unknown_ids_degrade_safely() {
    let r = ContextRegistry::new();
    assert_eq!(r.context_name(ContextId(999)), None);
    assert_eq!(r.context_is_collectible(ContextId(999)), None);
    assert_eq!(r.assembly_context(AssemblyId(999)), None);
    assert!(!r.assembly_is_collectible(AssemblyId(999)));
    assert!(r.assembly_types(AssemblyId(999)).is_empty());
}
