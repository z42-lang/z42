//! P1-2 PR 4: the `VCall` PIC payload is the callee's `FnId`, so a virtual call whose
//! target lives in a lazily loaded package is cached and dispatched like a module-local one.

use super::*;
use crate::metadata::bytecode::{BasicBlock, Instruction, Terminator};
use crate::metadata::name_index::NameIndex;
use crate::metadata::tokens::{alloc_type_id_block, FnId, TypeId};
use crate::metadata::types::{ExecMode, NativeData, TypeDesc};
use crate::metadata::Function;
use crate::interp::Frame;
use std::sync::Arc;

/// An instance method returning `val` (receiver in reg 0).
fn method(name: &str, val: i32) -> Function {
    Function {
        name: name.to_string(),
        param_count: 1,
        ret_type: "int".to_string(),
        exec_mode: ExecMode::Interp,
        blocks: vec![BasicBlock {
            label: "entry".to_string(),
            instructions: vec![Instruction::ConstI32 { dst: 1, val }],
            terminator: Terminator::Ret { reg: Some(1) },
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

fn module(name: &str, functions: Vec<Function>) -> Module {
    let func_index = functions.iter().enumerate().map(|(i, f)| (f.name.clone(), i)).collect();
    Module {
        name: name.to_string(),
        string_pool: vec![],
        classes: vec![],
        functions,
        type_registry: rustc_hash::FxHashMap::default(),
        func_index,
    }
}

fn register_lazy(ctx: &VmContext, f: Function) -> Arc<Function> {
    let f = Arc::new(f);
    let mut state = ctx.core.lazy_loader.write();
    assert!(state.as_mut().unwrap().insert_function(f.name.clone(), Arc::clone(&f)));
    f
}

/// An instance of class `class` whose vtable binds `slot_name` → `target`.
fn object(ctx: &VmContext, class: &str, slot_name: &str, target: &str) -> Value {
    let mut vtable_index = NameIndex::new();
    vtable_index.insert(slot_name.to_string(), 0);
    let td = Arc::new(TypeDesc {
        class_flags: 0,
        visibility: 0,
        name: class.to_string(),
        base_name: None,
        fields: Vec::new(),
        field_index: NameIndex::new(),
        vtable: vec![(slot_name.to_string(), target.to_string())],
        vtable_index,
        cold: None,
        id: TypeId(alloc_type_id_block(1)),
    });
    ctx.heap().alloc_object(td, Vec::new(), NativeData::None)
}

fn local(target: &VCallTarget<'_>) -> (*const Function, Option<usize>) {
    match target {
        VCallTarget::Local { func, id } => (*func as *const Function, *id),
        _ => panic!("expected a function target"),
    }
}

/// Resolution hands back the lazily loaded callee with its `FnId` and installs that id
/// in the PIC; the hit reads the same function back through the `FuncTable`.
#[test]
fn lazy_target_is_installed_in_the_pic_by_fn_id() {
    let ctx = VmContext::with_module(module("Entry", vec![method("Entry.Other.Run$0", 1)]));
    ctx.install_lazy_loader(None, 0);
    let run = register_lazy(&ctx, method("Pkg.Foo.Run$0", 7));
    let id = run.id.get().expect("registered").0 as usize;
    assert_eq!(id, 1, "lazy ids follow the entry module's");
    let entry = Arc::clone(ctx.module().unwrap());
    let obj = object(&ctx, "Pkg.Foo", "Run$0", "Pkg.Foo.Run$0");
    let ic = VCallIC::default();

    let r = resolve_vcall(&ctx, &entry, &obj, "Run$0", 0, Some(&ic)).unwrap();
    assert_eq!(local(&r.target), (Arc::as_ptr(&run), Some(id)));
    assert_eq!(vcall_ic_hit(Some(&ic), &obj), Some(id), "the lazy FnId entered the PIC");
    let back = super::super::exec_call::fn_by_id(&ctx, &entry, id as u32).unwrap();
    assert!(std::ptr::eq(back, &*run));
}

/// The interpreter's `VCall` runs the lazy target and, from the second call on, takes it
/// straight off the PIC — no name lookup: after the loader forgets the name the call
/// still dispatches (slots and ids outlive a loader's name space).
#[test]
fn interp_vcall_hits_the_pic_for_a_lazy_target() {
    let ctx = VmContext::with_module(module("Entry", vec![method("Entry.Other.Run$0", 1)]));
    ctx.install_lazy_loader(None, 0);
    register_lazy(&ctx, method("Pkg.Foo.Run$0", 7));
    let entry = Arc::clone(ctx.module().unwrap());
    let obj = object(&ctx, "Pkg.Foo", "Run$0", "Pkg.Foo.Run$0");
    let ic = VCallIC::default();

    let mut frame = Frame::new(&ctx, &[], 4);
    frame.set(0, obj.clone());
    let thrown = super::super::exec_vcall::vcall(&ctx, &entry, &mut frame, 1, 0, "Run$0", &[], Some(&ic), &[]).unwrap();
    assert!(thrown.is_none());
    assert!(matches!(frame.get(1).unwrap(), Value::I64(7)));

    // Fresh name space, seen from a context without a per-ctx lookup cache (shares the
    // core, so the same `FuncTable`): `Pkg.Foo.Run$0` no longer resolves by name.
    ctx.install_lazy_loader(None, 0);
    let ctx2 = VmContext::new_with_core(Arc::clone(&ctx.core));
    assert!(ctx2.try_lookup_function("Pkg.Foo.Run$0").is_none());
    let mut frame = Frame::new(&ctx2, &[], 4);
    frame.set(0, obj);
    let thrown = super::super::exec_vcall::vcall(&ctx2, &entry, &mut frame, 1, 0, "Run$0", &[], Some(&ic), &[]).unwrap();
    assert!(thrown.is_none());
    assert!(matches!(frame.get(1).unwrap(), Value::I64(7)), "dispatched off the PIC by FnId");
}

/// An entry-module target still caches its `module.functions` index (= its `FnId`).
#[test]
fn entry_target_payload_is_its_index() {
    let ctx = VmContext::with_module(module("Entry", vec![
        method("Entry.Other.Run$0", 1), method("Entry.Foo.Run$0", 5),
    ]));
    let entry = Arc::clone(ctx.module().unwrap());
    let obj = object(&ctx, "Entry.Foo", "Run$0", "Entry.Foo.Run$0");
    let ic = VCallIC::default();
    let r = resolve_vcall(&ctx, &entry, &obj, "Run$0", 0, Some(&ic)).unwrap();
    assert_eq!(local(&r.target), (&entry.functions[1] as *const Function, Some(1)));
    assert_eq!(vcall_ic_hit(Some(&ic), &obj), Some(1));
    assert_eq!(entry.functions[1].id.get(), Some(FnId(1)));
}

/// A target whose signature cannot take the call is resolved (so the arity check throws)
/// but never cached — lazy targets included.
#[test]
fn arity_mismatch_is_not_cached() {
    let ctx = VmContext::with_module(module("Entry", vec![method("Entry.Other.Run$0", 1)]));
    ctx.install_lazy_loader(None, 0);
    register_lazy(&ctx, method("Pkg.Foo.Run$0", 7));   // takes the receiver only
    let entry = Arc::clone(ctx.module().unwrap());
    let obj = object(&ctx, "Pkg.Foo", "Run$0", "Pkg.Foo.Run$0");
    let ic = VCallIC::default();
    let r = resolve_vcall(&ctx, &entry, &obj, "Run$0", 2, Some(&ic)).unwrap();
    assert!(matches!(r.target, VCallTarget::Thrown(_)));
    assert_eq!(vcall_ic_hit(Some(&ic), &obj), None);
}

/// Against a module that is not the VM's entry module (bare `VmContext::new()`), a lazy
/// target has no `FnId` under that module (its id could equal one of the module's
/// indices): it runs, but is never cached.
#[test]
fn non_entry_module_never_caches_a_lazy_target() {
    let ctx = VmContext::new();
    ctx.install_lazy_loader(None, 0);
    let run = register_lazy(&ctx, method("Pkg.Foo.Run$0", 7));
    assert_eq!(run.id.get(), Some(FnId(0)), "same number as the module's first function");
    let m = module("M", vec![method("M.Other.Run$0", 1)]);
    let obj = object(&ctx, "Pkg.Foo", "Run$0", "Pkg.Foo.Run$0");
    let ic = VCallIC::default();

    let r = resolve_vcall(&ctx, &m, &obj, "Run$0", 0, Some(&ic)).unwrap();
    assert_eq!(local(&r.target), (Arc::as_ptr(&run), None));
    assert_eq!(vcall_ic_hit(Some(&ic), &obj), None, "not cached");

    let mut frame = Frame::new(&ctx, &[], 4);
    frame.set(0, obj);
    super::super::exec_vcall::vcall(&ctx, &m, &mut frame, 1, 0, "Run$0", &[], Some(&ic), &[]).unwrap();
    assert!(matches!(frame.get(1).unwrap(), Value::I64(7)));
    assert_eq!(vcall_ic_hit(Some(&ic), &frame.get(0).unwrap().clone()), None);
}
