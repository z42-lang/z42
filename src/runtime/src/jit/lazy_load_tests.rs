//! Unit tests for JIT lazy *loading* (make-vm-loading-lazy, 2026-07-24).
//!
//! Distinct from `lazy_tests.rs` (which covers lazy *compilation* of merged
//! functions): these cover the name→id→entry resolution the loader path relies
//! on — `resolve_id_by_name`, the synthetic-id routing in `resolve_fn_by_id`,
//! and graceful degradation when a name resolves nowhere.
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
        type_registry: std::collections::HashMap::new(),
        type_registry_vec: Vec::new(),
        func_index,
        func_ref_cache_slots: 0,
        interned_strings: Vec::new(),
    }
}

fn compiled_count(vm: &VmContext) -> u64 {
    vm.counters().jit_methods_compiled.load(Ordering::Relaxed)
}

#[test]
fn resolve_id_by_name_maps_merged_function_to_its_index() {
    // A name present in the merged module resolves to its `func_index` id, which
    // is always `< merged_len` (so `resolve_fn_by_id` takes the merged path).
    let module = module_of("M", vec![empty_fn("entry"), empty_fn("a"), empty_fn("b")]);
    let merged_len = module.functions.len() as u32;
    let vm = VmContext::new();
    let mut jm = JitModule::setup(&module).expect("setup");
    jm.ctx.vm_ctx = (&*vm as *const VmContext) as *mut VmContext;
    unsafe {
        assert_eq!(jm.ctx.resolve_id_by_name("entry"), Some(0));
        assert_eq!(jm.ctx.resolve_id_by_name("a"), Some(1));
        assert_eq!(jm.ctx.resolve_id_by_name("b"), Some(2));
        assert!(jm.ctx.resolve_id_by_name("b").unwrap() < merged_len,
            "merged ids stay below merged_len — never synthetic");
    }
}

#[test]
fn resolve_fn_by_name_routes_through_id_and_compiles() {
    // The rewritten `resolve_fn_by_name` (resolve_id_by_name → resolve_fn_by_id)
    // must still compile a merged function exactly once.
    let module = module_of("M", vec![empty_fn("a")]);
    let vm = VmContext::new();
    let mut jm = JitModule::setup(&module).expect("setup");
    jm.ctx.vm_ctx = (&*vm as *const VmContext) as *mut VmContext;
    let e = unsafe { jm.ctx.resolve_fn_by_name("a") };
    assert!(e.is_some(), "merged function resolves via name→id→entry");
    assert_eq!(compiled_count(&vm), 1);
}

#[test]
fn synthetic_id_beyond_lazy_table_resolves_to_none() {
    // A synthetic id (>= merged_len) with no registered lazy slot must return
    // None gracefully — never an out-of-bounds panic. Guards the lazy-slot
    // routing added to `resolve_fn_by_id`.
    let module = module_of("M", vec![empty_fn("a")]);
    let merged_len = module.functions.len();
    let vm = VmContext::new();
    let mut jm = JitModule::setup(&module).expect("setup");
    jm.ctx.vm_ctx = (&*vm as *const VmContext) as *mut VmContext;
    let r = unsafe { jm.ctx.resolve_fn_by_id(merged_len + 7) };
    assert!(r.is_none(), "unregistered synthetic id → None, no panic");
    assert_eq!(compiled_count(&vm), 0);
}

#[test]
fn resolve_id_by_name_unknown_without_loader_is_none() {
    // A name absent from the merged module, with no lazy loader installed
    // (VmContext::new has none), resolves to None — jit_call then falls back to
    // `cross_zpkg_via_interp`. This is the tier-3 "resolve nowhere" path.
    let module = module_of("M", vec![empty_fn("a")]);
    let vm = VmContext::new();
    let mut jm = JitModule::setup(&module).expect("setup");
    jm.ctx.vm_ctx = (&*vm as *const VmContext) as *mut VmContext;
    assert_eq!(unsafe { jm.ctx.resolve_id_by_name("nonexistent.Fn") }, None);
    // Sanity: UNRESOLVED is distinct from any real id, so a null-IC tier-3 miss
    // is unambiguous.
    assert_ne!(0u32, UNRESOLVED);
}
