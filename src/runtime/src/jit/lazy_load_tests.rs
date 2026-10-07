//! Unit tests for JIT ids and slots over lazily loaded functions (P1-2).
//!
//! Distinct from `lazy_tests.rs` (which covers lazy *compilation* of entry-module
//! functions): these cover the id space the JIT shares with `VmCore.funcs` —
//! name → id, `FnId`-keyed slots for lazily loaded functions, the negative cache,
//! tiering, OSR — and graceful degradation when a name or id resolves nowhere.
//!
//! The end-to-end behaviours the tasks enumerate — JIT loading only the zpkgs it
//! touches, a lazily-loaded stdlib function running native, and dep-package
//! `__static_init__` running under `--mode jit` — require on-disk zpkg fixtures
//! and live in the golden suite (`xtask test e2e --mode jit`, byte-identical to
//! interp). CI is authoritative for those (see tasks.md 3.2 / 3.4). Here we lock
//! the in-process resolution invariants that back them.

use crate::jit::JitModule;
use crate::metadata::bytecode::{BasicBlock, Function, Module, Terminator};
use crate::metadata::types::ExecMode;
use crate::metadata::tokens::UNRESOLVED;
use std::sync::Arc;
use crate::vm_context::VmContext;
use std::sync::atomic::Ordering;

/// A JIT-translatable `return;` function (no interp-only opcode).
fn empty_fn(name: &str) -> Function {
    Function {
        name: name.to_string(),
        param_count: 0,
        ret_type: "void".to_string(),
        exec_mode: ExecMode::Jit,
        blocks: vec![BasicBlock {
            label: "entry".to_string(),
            instructions: Vec::new(),
            terminator: Terminator::Ret { reg: None },
        }],
        is_static: false,
        visibility: 0,
        method_flags: 0, min_arg: 0, params_from: 0xFF,
        max_reg: 0,
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

fn module_of(name: &str, functions: Vec<Function>) -> Module {
    let func_index = functions.iter().enumerate()
        .map(|(i, f)| (f.name.clone(), i))
        .collect();
    Module {
        name: name.to_string(),
        string_pool: Vec::new(),
        classes: Vec::new(),
        functions,
        type_registry: rustc_hash::FxHashMap::default(),
        func_index,
    }
}

fn compiled_count(vm: &VmContext) -> u64 {
    vm.counters().jit_methods_compiled.load(Ordering::Relaxed)
}

/// An untranslatable function (`LoadLocalAddr` is interp-only).
fn interp_only_fn(name: &str) -> Function {
    let mut f = empty_fn(name);
    f.blocks[0].instructions = vec![crate::metadata::bytecode::Instruction::LoadLocalAddr { dst: 0, slot: 0 }];
    f
}

/// A VM whose entry module is `module` (so `FuncTable::is_entry` holds for the JIT's
/// module), plus one lazily registered function per `lazy` — returns their ids.
fn vm_with_lazy(module: Module, lazy: Vec<Function>) -> (std::pin::Pin<Box<VmContext>>, Vec<usize>) {
    let vm = VmContext::with_module(module);
    let ids = lazy.into_iter()
        .map(|f| vm.funcs().register_lazy(&Arc::new(f)).expect("fresh name").0 as usize)
        .collect();
    (vm, ids)
}

fn jit_for(vm: &VmContext) -> JitModule {
    let mut jm = JitModule::setup(vm.module().expect("entry module")).expect("setup");
    jm.ctx.vm_ctx = (vm as *const VmContext) as *mut VmContext;
    jm
}

#[test]
fn id_by_name_maps_entry_function_to_its_index() {
    let module = module_of("M", vec![empty_fn("entry"), empty_fn("a"), empty_fn("b")]);
    let vm = VmContext::new();
    let mut jm = JitModule::setup(&module).expect("setup");
    jm.ctx.vm_ctx = (&*vm as *const VmContext) as *mut VmContext;
    unsafe {
        assert_eq!(jm.ctx.id_by_name("entry"), Some(0));
        assert_eq!(jm.ctx.id_by_name("a"), Some(1));
        assert_eq!(jm.ctx.id_by_name("b"), Some(2));
        let b = jm.ctx.fn_of(2).expect("id 2");
        assert_eq!(jm.ctx.id_of_func(b), Some(2), "id_of_func finds an entry function by address");
    }
}

#[test]
fn resolve_fn_by_name_routes_through_id_and_compiles() {
    let module = module_of("M", vec![empty_fn("a")]);
    let vm = VmContext::new();
    let mut jm = JitModule::setup(&module).expect("setup");
    jm.ctx.vm_ctx = (&*vm as *const VmContext) as *mut VmContext;
    let e = unsafe { jm.ctx.resolve_fn_by_name("a") };
    assert!(e.is_some(), "entry function resolves via name→id→entry");
    assert_eq!(compiled_count(&vm), 1);
}

#[test]
fn unregistered_id_resolves_to_none() {
    // An id nothing is registered under must return None gracefully — never an
    // out-of-bounds panic, never a compile.
    let module = module_of("M", vec![empty_fn("a")]);
    let vm = VmContext::new();
    let mut jm = JitModule::setup(&module).expect("setup");
    jm.ctx.vm_ctx = (&*vm as *const VmContext) as *mut VmContext;
    let r = unsafe { jm.ctx.resolve_fn_by_id(module.functions.len() + 7) };
    assert!(r.is_none(), "unregistered id → None, no panic");
    assert_eq!(compiled_count(&vm), 0);
    assert_eq!(jm.ctx.slot_probe(module.functions.len() + 7), (0, false), "no slot touched");
}

#[test]
fn id_by_name_unknown_without_loader_is_none() {
    // A name absent from the module, with no lazy loader installed, resolves to
    // None — jit_call's by-name tier then raises the missing-symbol exception.
    let module = module_of("M", vec![empty_fn("a")]);
    let vm = VmContext::new();
    let mut jm = JitModule::setup(&module).expect("setup");
    jm.ctx.vm_ctx = (&*vm as *const VmContext) as *mut VmContext;
    assert_eq!(unsafe { jm.ctx.id_by_name("nonexistent.Fn") }, None);
    assert_ne!(0u32, UNRESOLVED);
}

#[test]
fn lazy_function_resolves_by_its_fn_id() {
    // A lazily loaded function's JIT id is its `FnId` (after the entry block); it
    // compiles into its own slot, by id and by name alike.
    let (vm, ids) = vm_with_lazy(module_of("M", vec![empty_fn("entry")]), vec![empty_fn("Lib.f")]);
    let jm = jit_for(&vm);
    let id = ids[0];
    assert_eq!(id, 1, "first lazy FnId follows the entry module's functions");
    unsafe {
        assert_eq!(jm.ctx.id_by_name("Lib.f"), Some(id));
        let f = jm.ctx.fn_of(id).expect("lazy fn by id");
        assert_eq!(f.name, "Lib.f");
        assert_eq!(jm.ctx.id_of_func(f), Some(id));
        assert!(jm.ctx.peek_fn_by_id(id).is_none(), "peek never compiles");
        let e = jm.ctx.resolve_fn_by_id(id).expect("compiles");
        assert!(std::ptr::eq(e.func, f), "entry points at the FuncTable's function");
        assert!(jm.ctx.peek_fn_by_id(id).is_some());
        assert!(jm.ctx.resolve_fn_by_name("Lib.f").is_some());
    }
    assert_eq!(compiled_count(&vm), 1, "compiled once");
}

#[test]
fn lazy_function_without_entry_module_has_no_id() {
    // Under a bare `VmContext::new()` the JIT's module is not the FuncTable's entry
    // module, so lazy FnIds would alias module indices: they are not JIT ids.
    let module = module_of("M", vec![empty_fn("a")]);
    let vm = VmContext::new();
    let lazy = Arc::new(empty_fn("Lib.f"));
    let id = vm.funcs().register_lazy(&lazy).expect("registered").0 as usize;
    let mut jm = JitModule::setup(&module).expect("setup");
    jm.ctx.vm_ctx = (&*vm as *const VmContext) as *mut VmContext;
    unsafe {
        assert_eq!(jm.ctx.id_of_func(&lazy), None);
        assert_eq!(jm.ctx.id_by_name("Lib.f"), None);
        if id >= module.functions.len() {
            assert!(jm.ctx.fn_of(id).is_none());
        }
    }
}

#[test]
fn tiered_lazy_function_compiles_at_threshold() {
    let (vm, ids) = vm_with_lazy(module_of("M", vec![empty_fn("entry")]), vec![empty_fn("Lib.f")]);
    let mut jm = jit_for(&vm);
    jm.ctx.jit_threshold = 3;
    let id = ids[0];
    unsafe {
        assert!(jm.ctx.resolve_fn_by_id_tiered(id).is_none(), "call 1: cold");
        assert!(jm.ctx.resolve_fn_by_id_tiered(id).is_none(), "call 2: cold");
        assert_eq!(compiled_count(&vm), 0);
        assert!(jm.ctx.resolve_fn_by_id_tiered(id).is_some(), "call 3: compiles");
        assert!(jm.ctx.resolve_fn_by_id_tiered(id).is_some(), "then hits");
    }
    assert_eq!(compiled_count(&vm), 1);
    assert_eq!(jm.ctx.slot_probe(id), (3, false), "counter frozen once compiled");
}

#[test]
fn untranslatable_lazy_function_is_negative_cached() {
    // Rejected at the threshold, then never re-scanned or re-counted.
    let (vm, ids) = vm_with_lazy(module_of("M", vec![empty_fn("entry")]), vec![interp_only_fn("Lib.g")]);
    let mut jm = jit_for(&vm);
    jm.ctx.jit_threshold = 2;
    let id = ids[0];
    unsafe {
        for _ in 0..5 {
            assert!(jm.ctx.resolve_fn_by_id_tiered(id).is_none());
        }
        assert!(jm.ctx.resolve_fn_by_id(id).is_none(), "non-tiered resolve honours the verdict");
        assert!(jm.ctx.resolve_osr_entry(id, 0).is_none(), "no OSR variant either");
    }
    assert_eq!(jm.ctx.slot_probe(id), (2, true), "rejected at call 2, counter frozen");
    assert_eq!(compiled_count(&vm), 0);
}

#[test]
fn untranslatable_entry_function_is_negative_cached() {
    let module = module_of("M", vec![interp_only_fn("f")]);
    let vm = VmContext::new();
    let mut jm = JitModule::setup(&module).expect("setup");
    jm.ctx.vm_ctx = (&*vm as *const VmContext) as *mut VmContext;
    unsafe {
        assert!(jm.ctx.resolve_fn_by_id(0).is_none());
        assert!(jm.ctx.resolve_fn_by_id_tiered(0).is_none());
    }
    assert_eq!(jm.ctx.slot_probe(0), (0, true), "rejected on the first (non-tiered) resolve");
}

#[test]
fn osr_entry_for_lazy_function_compiles_once() {
    let (vm, ids) = vm_with_lazy(module_of("M", vec![empty_fn("entry")]), vec![empty_fn("Lib.f")]);
    let jm = jit_for(&vm);
    let id = ids[0];
    unsafe {
        assert!(jm.ctx.resolve_osr_entry(id, 0).is_some(), "lazy functions get OSR entries");
        assert!(jm.ctx.resolve_osr_entry(id, 0).is_some(), "cached");
    }
    assert_eq!(compiled_count(&vm), 1, "one OSR compile per (id, header)");
}
