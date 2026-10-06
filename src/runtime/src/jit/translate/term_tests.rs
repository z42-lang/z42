//! Terminator translation tests that need hand-built IR (shapes z42c never emits).

use crate::jit::JitModule;
use crate::metadata::bytecode::{BasicBlock, Function, Instruction, Module, Terminator};
use crate::metadata::types::ExecMode;
use crate::vm_context::VmContext;

fn block(label: &str, instructions: Vec<Instruction>, terminator: Terminator) -> BasicBlock {
    BasicBlock { label: label.to_string(), instructions, terminator }
}

/// `entry: r0 = 5; br_cond r0 → yes / no`. `reg_types` is empty, so the
/// translation can't prove r0 is Bool and goes through `jit_get_bool`.
fn br_cond_on_int() -> Function {
    Function {
        name: "f".to_string(),
        param_count: 0,
        ret_type: "void".to_string(),
        exec_mode: ExecMode::Jit,
        blocks: vec![
            block("entry", vec![Instruction::ConstI64 { dst: 0, val: 5 }],
                  Terminator::BrCond { cond: 0, true_label: "yes".to_string(), false_label: "no".to_string() }),
            block("yes", Vec::new(), Terminator::Ret { reg: None }),
            block("no", Vec::new(), Terminator::Ret { reg: None }),
        ],
        is_static: false,
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

#[test]
fn br_cond_on_non_bool_throws_instead_of_branching() {
    let f = br_cond_on_int();
    let module = Module {
        name: "M".to_string(),
        string_pool: Vec::new(),
        classes: Vec::new(),
        func_index: [(f.name.clone(), 0)].into_iter().collect(),
        functions: vec![f],
        type_registry: rustc_hash::FxHashMap::default(),
    };
    let vm = VmContext::new();
    let mut jm = JitModule::setup(&module).expect("setup");
    let err = jm.run_fn(&vm, "f").expect_err("a non-Bool condition must throw, not take a branch");
    assert!(err.to_string().contains("expected bool"), "unexpected error: {err}");
}
