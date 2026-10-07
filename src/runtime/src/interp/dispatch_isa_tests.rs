//! P1-2 PR 6: type tests (`is` / `as` / typed `catch`) are cached by `(receiver TypeId,
//! target key)` — never by descriptor or string address — and that holds for lazily
//! loaded and generic-instantiation types too.

use super::*;
use crate::metadata::bytecode::{ClassDesc, Module};
use crate::metadata::tokens::{TypeId, TypeKeyCell};
use std::sync::Arc;

fn class(name: &str, base: Option<&str>, interfaces: &[&str]) -> ClassDesc {
    ClassDesc {
        static_fields: vec![].into(),
        interfaces: interfaces.iter().map(|s| s.to_string()).collect(),
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
        fields: Box::new([]),
        type_params: Box::new([]),
        type_param_constraints: Box::new([]),
        attributes: Box::new([]),
    }
}

/// A module whose `type_registry` is built the way the loader builds it (global TypeIds).
fn module(name: &str, classes: Vec<ClassDesc>) -> Module {
    let mut m = Module {
        name: name.to_owned(),
        string_pool: vec![],
        classes,
        functions: vec![],
        type_registry: rustc_hash::FxHashMap::default(),
        func_index: rustc_hash::FxHashMap::default(),
    };
    crate::metadata::loader::build_type_registry(&mut m);
    m
}

/// Register `pkg`'s types with the context's lazy loader, as a package load does.
fn load_lazily(ctx: &VmContext, pkg: Module) {
    let mut state = ctx.core.lazy_loader.write();
    let loader = state.as_mut().expect("loader installed");
    for (name, td) in pkg.type_registry {
        loader.insert_type(name, td);
    }
    loader.publish_types();
}

/// Entry `App.Thing : Pkg.Base`; lazily loaded `Pkg.Base : IShape`, `Pkg.Leaf : Pkg.Base`,
/// and the instantiation `Pkg.Box<int>`.
fn setup() -> std::pin::Pin<Box<VmContext>> {
    let ctx = VmContext::with_module(module("App", vec![class("App.Thing", Some("Pkg.Base"), &[])]));
    ctx.install_lazy_loader(None, 0);
    load_lazily(&ctx, module("Pkg", vec![
        class("Pkg.IShape", None, &[]),
        class("Pkg.Base", None, &["Pkg.IShape"]),
        class("Pkg.Leaf", Some("Pkg.Base"), &[]),
        class("Pkg.Box<int>", None, &[]),
    ]));
    ctx
}

fn lazy(ctx: &VmContext, name: &str) -> Arc<TypeDesc> {
    ctx.try_lookup_type(name).unwrap_or_else(|| panic!("{name} registered"))
}

fn isa(ctx: &VmContext, td: &TypeDesc, target: &str, key: &TypeKeyCell) -> bool {
    let entry = Arc::clone(ctx.module().unwrap());
    isa_td(ctx, &entry.type_registry, td, target, key)
}

#[test]
fn lazy_types_are_registered_in_the_type_table() {
    let ctx = setup();
    for n in ["Pkg.IShape", "Pkg.Base", "Pkg.Leaf", "Pkg.Box<int>"] {
        let td = lazy(&ctx, n);
        assert!(td.id.is_resolved());
        assert!(std::ptr::eq(ctx.types().get(td.id).unwrap(), Arc::as_ptr(&td)), "{n}");
    }
    let thing = Arc::clone(&ctx.module().unwrap().type_registry["App.Thing"]);
    assert!(std::ptr::eq(ctx.types().get(thing.id).unwrap(), Arc::as_ptr(&thing)));
}

/// The key is the target's TypeId when it names a registered type; the verdict lands in
/// `isa_cache` under `(receiver id, key)`.
#[test]
fn lazy_receiver_and_target_are_keyed_by_type_id() {
    let ctx = setup();
    let leaf = lazy(&ctx, "Pkg.Leaf");
    let base = lazy(&ctx, "Pkg.Base");
    let key = TypeKeyCell::new();
    assert!(isa(&ctx, &leaf, "Pkg.Base", &key));
    assert_eq!(key.get(), Some(base.id.0), "registered target ⇒ its TypeId");
    assert_eq!(ctx.isa_cache.get(leaf.id.0, base.id.0), Some(true));
    // Interface reached through the base, and a miss.
    assert!(isa(&ctx, &leaf, "Pkg.IShape", &TypeKeyCell::new()));
    let thing_key = TypeKeyCell::new();
    assert!(!isa(&ctx, &leaf, "App.Thing", &thing_key));
    let thing_id = ctx.module().unwrap().type_registry["App.Thing"].id.0;
    assert_eq!(ctx.isa_cache.get(leaf.id.0, thing_id), Some(false));
    // An entry-module receiver whose base lives in the lazy package.
    let thing = Arc::clone(&ctx.module().unwrap().type_registry["App.Thing"]);
    assert!(isa(&ctx, &thing, "Pkg.IShape", &TypeKeyCell::new()));
}

/// A hit is answered from the cache by ids alone: a key cell already resolved for one
/// target name is never re-resolved.
#[test]
fn a_resolved_key_is_reused_without_looking_up_the_name() {
    let ctx = setup();
    let leaf = lazy(&ctx, "Pkg.Leaf");
    let key = TypeKeyCell::new();
    assert!(isa(&ctx, &leaf, "Pkg.Base", &key));
    let k = key.get().unwrap();
    assert!(isa(&ctx, &leaf, "Pkg.Base", &key));
    assert_eq!(key.get(), Some(k));
}

/// Generic instantiations: the erased name and the arity-mangled spelling name no
/// registered type, so they get reserved keys; another instantiation does not match.
#[test]
fn generic_instantiation_tests_use_reserved_keys_for_erased_names() {
    let ctx = setup();
    let boxed = lazy(&ctx, "Pkg.Box<int>");
    let erased = TypeKeyCell::new();
    assert!(isa(&ctx, &boxed, "Pkg.Box", &erased));
    let k = erased.get().unwrap();
    assert!(ctx.types().get(TypeId(k)).is_none(), "reserved for the name, not a descriptor");
    assert_eq!(ctx.isa_cache.get(boxed.id.0, k), Some(true));
    assert!(isa(&ctx, &boxed, "Pkg.Box$1", &TypeKeyCell::new()));
    assert!(!isa(&ctx, &boxed, "Pkg.Box<string>", &TypeKeyCell::new()));
    assert!(isa(&ctx, &boxed, "Pkg.Box<int>", &TypeKeyCell::new()));
}

/// A target that is not registered yet gets a reserved key; once its package loads, the
/// key still denotes the same name and the verdict is unchanged (keys and ids are stable
/// across lazy loads).
#[test]
fn keys_and_ids_are_stable_across_later_lazy_loads() {
    let ctx = setup();
    let leaf = lazy(&ctx, "Pkg.Leaf");
    let early = TypeKeyCell::new();
    assert!(!isa(&ctx, &leaf, "Late.Other", &early));
    let reserved = early.get().unwrap();

    load_lazily(&ctx, module("Late", vec![class("Late.Other", None, &[])]));
    let other = lazy(&ctx, "Late.Other");
    assert_ne!(other.id.0, reserved, "the reservation is not the type's id");
    assert_eq!(lazy(&ctx, "Pkg.Leaf").id, leaf.id, "earlier ids unchanged");
    assert!(std::ptr::eq(ctx.types().get(leaf.id).unwrap(), Arc::as_ptr(&leaf)));
    // Old reserved key and the new TypeId key both answer for the same name.
    assert!(!isa(&ctx, &leaf, "Late.Other", &early));
    let fresh = TypeKeyCell::new();
    assert!(isa(&ctx, &other, "Late.Other", &fresh));
    assert_eq!(fresh.get(), Some(other.id.0));
}

/// Name-only callers (reflection) share the same id-keyed memo.
#[test]
fn name_only_entry_memoises_by_ids() {
    let ctx = setup();
    let entry = Arc::clone(ctx.module().unwrap());
    assert!(is_subclass_or_eq_td(&ctx, &entry.type_registry, "Pkg.Leaf", "Pkg.IShape"));
    assert!(!is_subclass_or_eq_td(&ctx, &entry.type_registry, "Pkg.IShape", "Pkg.Leaf"));
    assert!(!is_subclass_or_eq_td(&ctx, &entry.type_registry, "No.Such", "Pkg.Leaf"));
    let leaf = lazy(&ctx, "Pkg.Leaf");
    let shape = lazy(&ctx, "Pkg.IShape");
    let pair = crate::vm_context::isa_cache::pair_key(leaf.id.0, shape.id.0);
    assert_eq!(ctx.subclass_memo.lock().get(&pair).copied(), Some(true));
}

/// Descriptors without an id (transient fallbacks) are answered by the walk, never cached.
#[test]
fn id_less_descriptors_are_answered_but_not_cached() {
    let ctx = setup();
    let entry = Arc::clone(ctx.module().unwrap());
    let fallback = make_fallback_type_desc(&entry, "Local");
    assert!(!fallback.id.is_resolved());
    let key = TypeKeyCell::new();
    assert!(isa(&ctx, &fallback, "Local", &key));
    assert!(!isa(&ctx, &fallback, "Pkg.Base", &key));
    assert_eq!(key.get(), None, "no key resolved for an uncached test");
    assert!(ctx.subclass_memo.lock().is_empty());
}

/// Explicit module (re)load clears both caches; ids stay.
#[test]
fn explicit_reload_clears_the_caches() {
    let ctx = setup();
    let leaf = lazy(&ctx, "Pkg.Leaf");
    let key = TypeKeyCell::new();
    assert!(isa(&ctx, &leaf, "Pkg.Base", &key));
    let _ = ctx.load_module_bytes_into_vm(&[]); // fails to parse, still clears first
    assert_eq!(ctx.isa_cache.get(leaf.id.0, key.get().unwrap()), None);
    assert!(ctx.subclass_memo.lock().is_empty());
}
