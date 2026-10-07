//! P1-2 PR 7: package string pools go into the VM's `StrTable`, so a `ConstStr` in entry-module
//! or lazily loaded code is one id space, interned once per VM and rooted by the table.

use super::*;
use crate::metadata::bytecode::{BasicBlock, Function, Instruction, Module, Terminator};
use crate::metadata::loader::LoadedArtifact;
use crate::metadata::tokens::FnId;
use crate::metadata::types::ExecMode;
use crate::metadata::vstr::Str;
use crate::metadata::Value;
use crate::vm_context::VmContext;

/// `string F() { return <pool[idx]>; }`
fn const_str_fn(name: &str, idx: u32) -> Function {
    Function {
        name: name.to_string(),
        param_count: 0,
        ret_type: "string".to_string(),
        exec_mode: ExecMode::Interp,
        blocks: vec![BasicBlock {
            label: "entry".to_string(),
            instructions: vec![Instruction::ConstStr { dst: 0, idx }],
            terminator: Terminator::Ret { reg: Some(0) },
        }],
        is_static: true,
        visibility: 0,
        method_flags: 0, min_arg: 0, params_from: 0xFF,
        max_reg: 1,
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

fn module(name: &str, pool: &[&str], functions: Vec<Function>) -> Module {
    let func_index = functions.iter().enumerate().map(|(i, f)| (f.name.clone(), i)).collect();
    Module {
        name: name.to_string(),
        string_pool: pool.iter().map(|s| s.to_string()).collect(),
        classes: vec![],
        functions,
        type_registry: Default::default(),
        func_index,
    }
}

fn package(name: &str, pool: &[&str], functions: Vec<Function>) -> LoadedArtifact {
    LoadedArtifact {
        module: module(name, pool, functions),
        entry_hint: None,
        dependencies: vec![],
        import_namespaces: vec![],
        test_index: vec![],
        impl_pairs: vec![],
        package_name: Some(name.to_lowercase()),
    }
}

fn register(ctx: &VmContext, artifact: LoadedArtifact) {
    ctx.core.lazy_loader.write().as_mut().expect("loader installed")
        .register_loaded_artifact(artifact)
        .expect("register package");
}

fn const_operand(f: &Function) -> u32 {
    match f.blocks[0].instructions[0] {
        Instruction::ConstStr { idx, .. } => idx,
        _ => unreachable!(),
    }
}

/// Run `f` in the interpreter against the VM's entry module; its returned string.
fn run(ctx: &VmContext, f: &Function) -> Str {
    let entry = Arc::clone(ctx.module().expect("entry module"));
    match crate::interp::run_returning(ctx, &entry, f, &[]).expect("runs") {
        Some(Value::Str(s)) => s,
        other => panic!("expected a string, got {other:?}"),
    }
}

fn same(a: Str, b: Str) -> bool {
    a.var_ref().ptr_eq(&b.var_ref())
}

/// Entry module with pool `["e0", "e1"]` and `Entry.F$0` returning `"e1"`, loader installed.
fn entry_ctx() -> std::pin::Pin<Box<VmContext>> {
    let ctx = VmContext::with_module(module("Entry", &["e0", "e1"], vec![const_str_fn("Entry.F$0", 1)]));
    ctx.install_lazy_loader(None, 2);
    ctx
}

#[test]
fn package_pools_are_appended_and_operands_shifted_into_the_vm_id_space() {
    let ctx = entry_ctx();
    register(&ctx, package("P1", &["a", "b"], vec![const_str_fn("P1.F$0", 1)]));
    register(&ctx, package("P2", &["c"], vec![const_str_fn("P2.F$0", 0)]));
    let strings = &ctx.core.strings;
    assert_eq!(strings.len(), 5, "entry 2 + P1 2 + P2 1");
    let p1 = ctx.try_lookup_function("P1.F$0").expect("registered");
    let p2 = ctx.try_lookup_function("P2.F$0").expect("registered");
    assert_eq!(const_operand(&p1), 3);
    assert_eq!(const_operand(&p2), 4);
    assert_eq!(strings.text(3), Some("b"));
    assert_eq!(strings.text(4), Some("c"));
}

#[test]
fn lazy_and_entry_literals_are_interned_once_and_then_hit() {
    let ctx = entry_ctx();
    register(&ctx, package("P1", &["a", "b"], vec![const_str_fn("P1.F$0", 1)]));
    let lazy = ctx.try_lookup_function("P1.F$0").expect("registered");
    let entry_fn = &ctx.module().expect("entry").functions[0];

    let l1 = run(&ctx, &lazy);
    let l2 = run(&ctx, &lazy);
    assert_eq!(l1.as_str(), "b");
    assert!(same(l1, l2), "second execution reuses the interned string");
    let e1 = run(&ctx, entry_fn);
    let e2 = run(&ctx, entry_fn);
    assert_eq!(e1.as_str(), "e1");
    assert!(same(e1, e2));
    assert_eq!(ctx.core.strings.interned_count(), 2, "one per executed id");
    assert!(same(ctx.core.strings.get(3).expect("interned"), l1));
}

#[test]
fn reinstalling_the_loader_never_reuses_string_ids() {
    let ctx = entry_ctx();
    register(&ctx, package("P1", &["a", "b"], vec![const_str_fn("P1.F$0", 1)]));
    let old = ctx.try_lookup_function("P1.F$0").expect("registered");
    assert_eq!(old.id.get(), Some(FnId(1)));

    ctx.install_lazy_loader(None, 2);
    register(&ctx, package("P2", &["x", "y"], vec![const_str_fn("P2.F$0", 1)]));
    let new = ctx.try_lookup_function("P2.F$0").expect("registered");
    assert_eq!(const_operand(&new), 5, "the new loader numbers after the old one's ids");
    assert_eq!(run(&ctx, &old).as_str(), "b", "an old function still reads its own literal");
    assert_eq!(run(&ctx, &new).as_str(), "y");
}

#[test]
fn interned_literals_survive_a_collection() {
    let ctx = VmContext::with_module(module("Entry", &["kept"], vec![]));
    let entry = Arc::clone(ctx.module().expect("entry"));
    let heap = ctx.heap();
    heap.set_mode(crate::gc::GcMode::StwMarkSweep);
    let kept = ctx.const_str(&entry, 0).expect("in range");
    let dropped = heap.alloc_str("dropped"); // control: held by nothing the GC sees
    heap.force_collect();
    assert!(!dropped.var_ref().is_live(), "control: an unrooted string is swept");
    assert!(kept.var_ref().is_live(), "the string table roots its interned strings");
    assert_eq!(kept.as_str(), "kept");
    assert!(same(ctx.const_str(&entry, 0).expect("in range"), kept));
}

#[test]
fn threads_of_one_vm_share_a_literal_and_two_vms_do_not() {
    let a = VmContext::with_module(module("Entry", &["lit"], vec![]));
    let a2 = VmContext::new_with_core(a.core_arc());
    let b = VmContext::with_module(module("Entry", &["lit"], vec![]));
    let (ma, mb) = (Arc::clone(a.module().expect("entry")), Arc::clone(b.module().expect("entry")));

    let sa = a.const_str(&ma, 0).expect("in range");
    let sa2 = a2.const_str(&ma, 0).expect("in range");
    assert!(same(sa, sa2), "one interned string per VM");
    let sb = b.const_str(&mb, 0).expect("in range");
    assert!(!same(sa, sb), "a VM interns into its own heap");
    assert_eq!(sb.as_str(), "lit");
    assert_eq!(a.core.strings.interned_count(), 1);
    assert_eq!(b.core.strings.interned_count(), 1);
    // `b` cannot reach `a`'s module through its table.
    assert!(!b.core.strings.is_entry(&ma));
}

#[test]
fn a_module_other_than_the_entry_reads_its_own_pool_uncached() {
    let ctx = VmContext::new();
    ctx.install_lazy_loader(None, 1);
    let foreign = module("Foreign", &["own"], vec![]);
    let s1 = ctx.const_str(&foreign, 0).expect("in its pool");
    let s2 = ctx.const_str(&foreign, 0).expect("in its pool");
    assert_eq!(s1.as_str(), "own");
    assert!(!same(s1, s2), "no cache key for a foreign module's own pool");
    assert_eq!(ctx.core.strings.interned_count(), 0);

    // Past its pool: a lazily loaded package's id, interned in the table.
    register(&ctx, package("P1", &["pkg"], vec![]));
    let p1 = ctx.const_str(&foreign, 1).expect("package id");
    assert_eq!(p1.as_str(), "pkg");
    assert!(same(p1, ctx.const_str(&foreign, 1).expect("package id")));
    assert!(ctx.const_str(&foreign, 2).is_none());
}
