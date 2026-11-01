//! Terminator translation tests that need hand-built IR (shapes z42c never emits).

use crate::jit::runtime::install;
use crate::metadata::ir::{BasicBlock, Function, Instruction, Module, Terminator};
use crate::metadata::types::ExecMode;
use crate::vm_context::VmContext;

fn block(label: &str, instructions: Vec<Instruction>, terminator: Terminator) -> BasicBlock {
    BasicBlock { label: label.to_string(), instructions, terminator }
}

fn br_cond(cond: u32, t: &str, f: &str) -> Terminator {
    Terminator::BrCond { cond, true_label: t.to_string(), false_label: f.to_string() }
}

fn function(blocks: Vec<BasicBlock>, max_reg: u32) -> Function {
    Function {
        name: "f".to_string(),
        param_count: 0,
        ret_type: "void".to_string(),
        exec_mode: ExecMode::Jit,
        blocks,
        is_static: false,
        visibility: 0,
        method_flags: 0, min_arg: 0, params_from: 0xFF,
        max_reg,
        cold: None,
        reg_types: Box::new([]),
        block_index: std::collections::HashMap::new(),
        branch_targets: Vec::new(),
        fused_tails: Vec::new(),
        frame_meta: None,
        resolved: std::sync::OnceLock::new(),
        owner_init: Default::default(),
        id: Default::default(),
        forward: Default::default(),
    }
}

fn module(f: Function) -> Module {
    Module {
        name: "M".to_string(),
        string_pool: Vec::new(),
        classes: Vec::new(),
        func_index: [(f.name.clone(), 0)].into_iter().collect(),
        functions: vec![f],
        type_registry: rustc_hash::FxHashMap::default(),
    }
}

/// `entry: r0 = 5; br_cond r0 → yes / no`. `reg_types` is empty, so the
/// translation can't prove r0 is Bool and goes through `jit_get_bool`.
fn br_cond_on_int() -> Function {
    function(vec![
        block("entry", vec![Instruction::ConstI64 { dst: 0, val: 5 }], br_cond(0, "yes", "no")),
        block("yes", Vec::new(), Terminator::Ret { reg: None }),
        block("no", Vec::new(), Terminator::Ret { reg: None }),
    ], 1)
}

#[test]
fn br_cond_on_non_bool_throws_instead_of_branching() {
    let module = module(br_cond_on_int());
    let vm = VmContext::new();
    install(&vm, &module).expect("install");
    let err = crate::jit::run_fn(&vm, "f").expect_err("a non-Bool condition must throw, not take a branch");
    assert!(err.to_string().contains("expected bool"), "unexpected error: {err}");
}

/// Runs `f` JIT-compiled and returns how many safepoint polls it made (each
/// poll's fast path decrements the thread's `safepoint_skip` counter).
/// `indexed` runs the loader's block-index pass first, as for a loaded package.
fn safepoint_polls(f: Function, indexed: bool) -> u32 {
    use std::sync::atomic::Ordering;
    let mut module = module(f);
    if indexed { crate::metadata::loader::build_block_indices(&mut module); }
    let vm = VmContext::new();
    install(&vm, &module).expect("install");
    const START: u32 = 1_000_000;
    vm.safepoint_skip.store(START, Ordering::Relaxed);
    crate::jit::run_fn(&vm, "f").expect("run");
    START - vm.safepoint_skip.load(Ordering::Relaxed)
}

/// `entry: br_cond true → yes / no` — both targets forward.
fn forward_br_cond() -> Function {
    function(vec![
        block("entry", vec![Instruction::ConstBool { dst: 0, val: true }], br_cond(0, "yes", "no")),
        block("yes", Vec::new(), Terminator::Ret { reg: None }),
        block("no", Vec::new(), Terminator::Ret { reg: None }),
    ], 1)
}

/// `r0 = 0; loop: r0 += 1; br_cond r0 < 10 → loop / exit` — the loop's only
/// back-edge is a BrCond.
fn br_cond_loop() -> Function {
    function(vec![
        block("entry", vec![
            Instruction::ConstI64 { dst: 0, val: 0 },
            Instruction::ConstI64 { dst: 1, val: 1 },
            Instruction::ConstI64 { dst: 2, val: 10 },
        ], Terminator::Br { label: "loop".to_string() }),
        block("loop", vec![
            Instruction::Add { dst: 0, a: 0, b: 1 },
            Instruction::Lt { dst: 3, a: 0, b: 2 },
        ], br_cond(3, "loop", "exit")),
        block("exit", Vec::new(), Terminator::Ret { reg: None }),
    ], 4)
}

#[test]
fn forward_br_cond_does_not_poll_the_safepoint() {
    for indexed in [false, true] {
        assert_eq!(safepoint_polls(forward_br_cond(), indexed), 1, "only the entry poll (indexed: {indexed})");
    }
}

#[test]
fn br_cond_back_edge_polls_every_iteration() {
    for indexed in [false, true] {
        assert_eq!(safepoint_polls(br_cond_loop(), indexed), 1 + 10, "entry + one per iteration (indexed: {indexed})");
    }
}
