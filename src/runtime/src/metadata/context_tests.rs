//! Unit tests for context lifecycle + GC-driven reclamation
//! (add-lazy-context-unload). Covers the state machine (`unload`), the
//! `td_to_ctx` reverse map, and the `reclaim` decision — all pure registry
//! logic (no real GC needed). z42-level end-to-end (Unload + ForceCollect) is
//! covered by `src/tests/reflection/load_context_unload/`.

use super::{ContextId, ContextRegistry, ContextState, UnloadOutcome};
use crate::metadata::bytecode::Module;
use crate::metadata::tokens::TypeId;
use crate::metadata::types::{NativeData, TypeDesc};
use crate::metadata::NameIndex;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
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
fn unload_root_is_rejected() {
    let mut r = ContextRegistry::new();
    assert_eq!(r.unload(ContextId::ROOT), UnloadOutcome::RootRejected);
    assert_eq!(r.context_state(ContextId::ROOT), Some(ContextState::Active));
}

#[test]
fn unload_collectible_marks_unloading_and_is_idempotent() {
    let mut r = ContextRegistry::new();
    let flag = r.unloading_flag();
    let c = r.create_collectible("x");
    assert_eq!(r.context_state(c), Some(ContextState::Active));
    assert_eq!(flag.load(Ordering::Relaxed), 0);

    assert_eq!(r.unload(c), UnloadOutcome::Marked);
    assert_eq!(r.context_state(c), Some(ContextState::Unloading));
    assert_eq!(flag.load(Ordering::Relaxed), 1);

    // idempotent — second unload is a no-op
    assert_eq!(r.unload(c), UnloadOutcome::AlreadyUnloading);
    assert_eq!(flag.load(Ordering::Relaxed), 1);
}

#[test]
fn unload_unknown_context_not_found() {
    let mut r = ContextRegistry::new();
    assert_eq!(r.unload(ContextId(999)), UnloadOutcome::NotFound);
}

#[test]
fn load_into_registers_collectible_types_not_root() {
    let mut r = ContextRegistry::new();
    // root load → NOT in td_to_ctx (snapshot has empty td map)
    let _ra = r.load_into(ContextId::ROOT, "extra", mk_module(&["R.T"]));
    // collectible load → registered
    let c = r.create_collectible("x");
    let aid = r.load_into(c, "dep", mk_module(&["Demo.A"]));
    let td_ptr = Arc::as_ptr(&r.assembly_types(aid)[0]) as usize;

    let snap = r.liveness_snapshot();
    assert_eq!(snap.retained_context(td_ptr, &NativeData::None), Some(c));
    // an unrelated ptr resolves to None
    assert_eq!(snap.retained_context(0xDEAD, &NativeData::None), None);
}

#[test]
fn reclaim_frees_unreferenced_unloading_context() {
    let mut r = ContextRegistry::new();
    let flag = r.unloading_flag();
    let c = r.create_collectible("x");
    let aid = r.load_into(c, "dep", mk_module(&["Demo.A", "Demo.B"]));
    assert_eq!(r.assembly_types(aid).len(), 2);

    r.unload(c);
    assert_eq!(flag.load(Ordering::Relaxed), 1);

    // no live references → reclaimed
    let reclaimed = r.reclaim(&HashSet::new());
    assert_eq!(reclaimed, 1);
    assert_eq!(r.context_state(c), Some(ContextState::Reclaimed));
    assert!(r.assembly_types(aid).is_empty()); // arena (module) freed
    assert_eq!(flag.load(Ordering::Relaxed), 0);

    // snapshot no longer maps the freed types
    assert!(r.liveness_snapshot().is_empty());
}

#[test]
fn reclaim_keeps_referenced_unloading_context() {
    let mut r = ContextRegistry::new();
    let c = r.create_collectible("x");
    let aid = r.load_into(c, "dep", mk_module(&["Demo.A"]));
    r.unload(c);

    // c is live this cycle → NOT reclaimed (Erlang: wait for it to die)
    let mut live = HashSet::new();
    live.insert(c);
    let reclaimed = r.reclaim(&live);
    assert_eq!(reclaimed, 0);
    assert_eq!(r.context_state(c), Some(ContextState::Unloading));
    assert_eq!(r.assembly_types(aid).len(), 1); // arena still alive

    // next cycle, no longer referenced → reclaimed
    assert_eq!(r.reclaim(&HashSet::new()), 1);
    assert_eq!(r.context_state(c), Some(ContextState::Reclaimed));
}

#[test]
fn reflection_handles_are_retention_edges() {
    let mut r = ContextRegistry::new();
    let c = r.create_collectible("x");
    let aid = r.load_into(c, "dep", mk_module(&["Demo.A"]));
    let snap = r.liveness_snapshot();

    // AssemblyHandle → its context
    assert_eq!(snap.retained_context(0, &NativeData::AssemblyHandle(aid)), Some(c));
    // LoadContextHandle → the context itself
    assert_eq!(snap.retained_context(0, &NativeData::LoadContextHandle(c)), Some(c));
    // root assembly / root context handles → None (not collectible)
    assert_eq!(
        snap.retained_context(0, &NativeData::LoadContextHandle(ContextId::ROOT)),
        None
    );
}
