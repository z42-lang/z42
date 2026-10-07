//! P1-2 PR 5: the `ObjNew` site cache holds the class descriptor and the ctor's `FnId`, so a
//! `new` of a class from a lazily loaded package hashes no names after its first execution.

use super::*;
use crate::interp::Frame;
use crate::metadata::bytecode::{BasicBlock, Instruction, ObjNewInsn, Terminator};
use crate::metadata::name_index::NameIndex;
use crate::metadata::tokens::{alloc_type_id_block, FnId, TypeId};
use crate::metadata::types::{ExecMode, TypeDescCold};

/// A ctor (receiver in reg 0) with `params` physical parameters, body `Ret`.
fn ctor(name: &str, params: usize) -> Function {
    func(name, params, vec![])
}

fn func(name: &str, params: usize, instructions: Vec<Instruction>) -> Function {
    Function {
        name: name.to_string(),
        param_count: params,
        ret_type: "void".to_string(),
        exec_mode: ExecMode::Interp,
        blocks: vec![BasicBlock {
            label: "entry".to_string(),
            instructions,
            terminator: Terminator::Ret { reg: None },
        }],
        is_static: false,
        visibility: 0,
        method_flags: 0, min_arg: 0, params_from: 0xFF,
        max_reg: 4,
        cold: None,
        reg_types: Box::new([]),
        block_index: std::collections::HashMap::new(),
        branch_targets: Vec::new(),
        fused_tails: Vec::new(),
        frame_meta: None,
        resolved: std::sync::OnceLock::new(),
        owner_init: Default::default(),
        id: Default::default(),
    }
}

fn class(name: &str, base_unmerged: bool) -> Arc<TypeDesc> {
    Arc::new(TypeDesc {
        class_flags: 0,
        visibility: 0,
        name: name.to_string(),
        base_name: base_unmerged.then(|| "Gone.Base".to_string()),
        fields: Vec::new(),
        field_index: NameIndex::new(),
        vtable: Vec::new(),
        vtable_index: NameIndex::new(),
        cold: base_unmerged.then(|| Box::new(TypeDescCold { base_unmerged: true, ..Default::default() })),
        id: TypeId(alloc_type_id_block(1)),
    })
}

fn module(name: &str, functions: Vec<Function>, types: Vec<Arc<TypeDesc>>) -> Module {
    let func_index = functions.iter().enumerate().map(|(i, f)| (f.name.clone(), i)).collect();
    Module {
        name: name.to_string(),
        string_pool: vec![],
        classes: vec![],
        functions,
        type_registry: types.into_iter().map(|t| (t.name.clone(), t)).collect(),
        func_index,
    }
}

fn register_lazy_fn(ctx: &VmContext, f: Function) -> Arc<Function> {
    let f = Arc::new(f);
    let mut state = ctx.core.lazy_loader.write();
    assert!(state.as_mut().unwrap().insert_function(f.name.clone(), Arc::clone(&f)));
    f
}

fn register_lazy_type(ctx: &VmContext, td: &Arc<TypeDesc>) {
    let mut state = ctx.core.lazy_loader.write();
    state.as_mut().unwrap().insert_type(td.name.clone(), Arc::clone(td));
}

/// `new Pkg.Foo()` with both the class and its ctor in a lazily loaded package.
fn lazy_pkg_ctx() -> (std::pin::Pin<Box<VmContext>>, Arc<TypeDesc>, Arc<Function>) {
    let ctx = VmContext::with_module(module("Entry", vec![ctor("Entry.Other..ctor$0", 1)], vec![]));
    ctx.install_lazy_loader(None, 0);
    let td = class("Pkg.Foo", false);
    register_lazy_type(&ctx, &td);
    let f = register_lazy_fn(&ctx, ctor("Pkg.Foo..ctor$0", 1));
    (ctx, td, f)
}

fn ctor_of<'a>(r: &Result<Option<(&'a Function, Option<usize>)>, Value>) -> Option<(*const Function, Option<usize>)> {
    match r {
        Ok(c) => c.map(|(f, id)| (f as *const Function, id)),
        Err(_) => panic!("unexpected exception"),
    }
}

/// Class and ctor from a lazily loaded package enter the site cache as the loader's
/// descriptor and the ctor's `FnId`; the hit reads them back without any name lookup —
/// after the loader forgets both names, the site still resolves.
#[test]
fn lazy_class_and_ctor_are_cached_by_identity() {
    let (ctx, td, f) = lazy_pkg_ctx();
    let entry = Arc::clone(ctx.module().unwrap());
    let site = ObjNewSite::default();
    let s = site_for(&ctx, &entry, Some(&site));
    assert!(s.is_some(), "entry module uses the cache");

    let got = resolve_class(&ctx, &entry, "Pkg.Foo", s).unwrap();
    assert!(Arc::ptr_eq(&got, &td));
    assert!(Arc::ptr_eq(site.class.get().expect("class cached"), &td));
    let mut holder = None;
    let r = resolve_ctor(&ctx, &entry, "Pkg.Foo", "Pkg.Foo..ctor$0", 1, true, s, &mut holder);
    let id = f.id.get().expect("registered").0 as usize;
    assert_eq!(ctor_of(&r), Some((Arc::as_ptr(&f), Some(id))));
    assert_eq!(site.ctor.load(Ordering::Relaxed), id as u32, "the lazy FnId is cached");

    // Fresh loader name space, seen from a context without a per-ctx lookup cache.
    ctx.install_lazy_loader(None, 0);
    let ctx2 = VmContext::new_with_core(Arc::clone(&ctx.core));
    assert!(ctx2.try_lookup_function("Pkg.Foo..ctor$0").is_none());
    assert!(ctx2.try_lookup_type("Pkg.Foo").is_none());
    let s = site_for(&ctx2, &entry, Some(&site));
    assert!(Arc::ptr_eq(&resolve_class(&ctx2, &entry, "Pkg.Foo", s).unwrap(), &td));
    let mut holder = None;
    let r = resolve_ctor(&ctx2, &entry, "Pkg.Foo", "Pkg.Foo..ctor$0", 1, true, s, &mut holder);
    assert_eq!(ctor_of(&r), Some((Arc::as_ptr(&f), Some(id))), "hit by FnId");
}

/// The interpreter's `ObjNew` end to end: allocates the cached class and runs the cached
/// ctor — also once neither name resolves any more.
#[test]
fn interp_obj_new_hits_the_site_cache() {
    let (ctx, td, _f) = lazy_pkg_ctx();
    let entry = Arc::clone(ctx.module().unwrap());
    let site = ObjNewSite::default();
    let run = |ctx: &VmContext| {
        let mut frame = Frame::new(ctx, &[], 4);
        let thrown = super::super::exec_object::obj_new(
            ctx, &entry, &mut frame, 1, "Pkg.Foo", "Pkg.Foo..ctor$0", &[], &[], Some(&site), false, true,
        ).unwrap();
        assert!(thrown.is_none());
        match frame.get(1).unwrap() {
            Value::Object(rc) => assert!(Arc::ptr_eq(&rc.borrow().type_desc, &td)),
            other => panic!("expected an object, got {other:?}"),
        }
    };
    run(&ctx);
    ctx.install_lazy_loader(None, 0);
    let ctx2 = VmContext::new_with_core(Arc::clone(&ctx.core));
    run(&ctx2);
}

/// An entry-module ctor caches its `module.functions` index (= its `FnId`).
#[test]
fn entry_ctor_payload_is_its_index() {
    let td = class("Entry.Foo", false);
    let ctx = VmContext::with_module(module(
        "Entry", vec![ctor("Entry.Other..ctor$0", 1), ctor("Entry.Foo..ctor$0", 1)], vec![Arc::clone(&td)],
    ));
    let entry = Arc::clone(ctx.module().unwrap());
    let site = ObjNewSite::default();
    let s = site_for(&ctx, &entry, Some(&site));
    assert!(Arc::ptr_eq(&resolve_class(&ctx, &entry, "Entry.Foo", s).unwrap(), &td));
    let mut holder = None;
    let r = resolve_ctor(&ctx, &entry, "Entry.Foo", "Entry.Foo..ctor$0", 1, true, s, &mut holder);
    assert_eq!(ctor_of(&r), Some((&entry.functions[1] as *const Function, Some(1))));
    assert_eq!(site.ctor.load(Ordering::Relaxed), 1);
    assert_eq!(entry.functions[1].id.get(), Some(FnId(1)));
}

/// A ctor whose signature cannot take the call throws and is never cached.
#[test]
fn arity_mismatch_is_not_cached() {
    let (ctx, _td, _f) = lazy_pkg_ctx();
    let entry = Arc::clone(ctx.module().unwrap());
    let site = ObjNewSite::default();
    let s = site_for(&ctx, &entry, Some(&site));
    let mut holder = None;
    let r = resolve_ctor(&ctx, &entry, "Pkg.Foo", "Pkg.Foo..ctor$0", 2, true, s, &mut holder);
    assert!(r.is_err(), "wrong arity throws");
    assert_eq!(site.ctor.load(Ordering::Relaxed), UNRESOLVED);
}

/// A class without a ctor: the site remembers it and stops resolving until a function is
/// registered — then the ctor that appeared is found and cached.
#[test]
fn ctorless_site_resolves_again_after_a_registration() {
    let (ctx, _td, _f) = lazy_pkg_ctx();
    let entry = Arc::clone(ctx.module().unwrap());
    let site = ObjNewSite::default();
    let s = site_for(&ctx, &entry, Some(&site));
    let mut holder = None;
    let r = resolve_ctor(&ctx, &entry, "Pkg.Bar", "Pkg.Bar..ctor$0", 1, false, s, &mut holder);
    assert_eq!(ctor_of(&r), None);
    // The registration mark is process-global (parallel tests bump it), so check that the
    // site recorded a proof rather than that it still hits.
    assert_ne!(site.ctorless.load(Ordering::Relaxed), 0, "proved ctorless");

    let bar = register_lazy_fn(&ctx, ctor("Pkg.Bar..ctor$0", 1));
    let mut holder = None;
    let r = resolve_ctor(&ctx, &entry, "Pkg.Bar", "Pkg.Bar..ctor$0", 1, false, s, &mut holder);
    let id = bar.id.get().unwrap().0 as usize;
    assert_eq!(ctor_of(&r), Some((Arc::as_ptr(&bar), Some(id))));
    assert_eq!(site.ctor.load(Ordering::Relaxed), id as u32);
}

/// Descriptors that may still change are not cached: a fallback descriptor (rebuilt per
/// allocation) and one whose base never merged (that one throws).
#[test]
fn fallback_and_unmerged_descriptors_are_not_cached() {
    let ctx = VmContext::with_module(module(
        "Entry", vec![ctor("Entry.Other..ctor$0", 1)], vec![class("Entry.Half", true)],
    ));
    ctx.install_lazy_loader(None, 0);
    let entry = Arc::clone(ctx.module().unwrap());

    let site = ObjNewSite::default();
    let s = site_for(&ctx, &entry, Some(&site));
    let td = resolve_class(&ctx, &entry, "Local", s).expect("fallback for a dotless local class");
    assert_eq!(td.name, "Local");
    assert!(site.class.get().is_none(), "fallback descriptor not cached");

    let site = ObjNewSite::default();
    let s = site_for(&ctx, &entry, Some(&site));
    assert!(resolve_class(&ctx, &entry, "Entry.Half", s).is_err(), "missing base throws");
    assert!(site.class.get().is_none());
}

/// Against a module that is not the VM's entry module the cache is bypassed: resolution
/// works by name and nothing is stored (a lazy ctor's `FnId` may equal a module index).
#[test]
fn non_entry_module_never_uses_the_cache() {
    let ctx = VmContext::new();
    ctx.install_lazy_loader(None, 0);
    let td = class("Pkg.Foo", false);
    register_lazy_type(&ctx, &td);
    let f = register_lazy_fn(&ctx, ctor("Pkg.Foo..ctor$0", 1));
    let m = module("M", vec![ctor("M.Other..ctor$0", 1)], vec![]);
    let site = ObjNewSite::default();
    let s = site_for(&ctx, &m, Some(&site));
    assert!(s.is_none());
    assert!(Arc::ptr_eq(&resolve_class(&ctx, &m, "Pkg.Foo", s).unwrap(), &td));
    let mut holder = None;
    let r = resolve_ctor(&ctx, &m, "Pkg.Foo", "Pkg.Foo..ctor$0", 1, true, s, &mut holder);
    assert_eq!(ctor_of(&r), Some((Arc::as_ptr(&f), None)));
    assert!(site.class.get().is_none());
    assert_eq!(site.ctor.load(Ordering::Relaxed), UNRESOLVED);
}

/// The resolver pre-fills an entry-module site from what is registered: the registry's
/// class and the ctor's `FnId` (lazily loaded ones included).
#[test]
fn resolver_prefills_registered_class_and_ctor() {
    let new_foo = Instruction::ObjNew(Box::new(ObjNewInsn {
        dst: 1, class_name: "Entry.Foo".to_string(), ctor_name: "Pkg.Foo..ctor$0".to_string(),
        args: Box::new([]), type_args: Box::new([]), stack_alloc: false, ctor_known: true,
    }));
    let td = class("Entry.Foo", false);
    let ctx = VmContext::with_module(module(
        "Entry", vec![func("Entry.Main", 0, vec![new_foo])], vec![Arc::clone(&td)],
    ));
    ctx.install_lazy_loader(None, 0);
    let f = register_lazy_fn(&ctx, ctor("Pkg.Foo..ctor$0", 1));
    let entry = Arc::clone(ctx.module().unwrap());
    crate::metadata::resolver::resolve_function_tokens(&entry.functions[0], &entry, &ctx);
    let r = entry.functions[0].resolved.get().unwrap();
    assert_eq!(r.obj_new.len(), 1);
    assert!(Arc::ptr_eq(r.obj_new[0].class.get().expect("prefilled"), &td));
    assert_eq!(r.obj_new[0].ctor.load(Ordering::Relaxed), f.id.get().unwrap().0);
}
