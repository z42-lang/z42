//! parallel-worker-jit (2026-09-01): concurrency-safety stress tests for the shared
//! JIT compiled-code table (`JitShared`).
//!
//! Before this change the JIT only ever ran on the entry thread; `--jobs N` workers
//! ran the interpreter. Now every worker builds its own per-thread `JitModuleCtx`
//! shell over the SAME `Arc<JitShared>` and runs native code, so the compile-once
//! slot tables (`fn_entries_by_id` `OnceLock`s, `lazy_table`, `osr_entries`) and the
//! `lazy` compiler `Mutex` are exercised concurrently for the first time. These tests
//! drive that path directly — N threads sharing one `JitShared` — to prove:
//!   • concurrent first-compiles of the SAME function converge to one compilation,
//!   • concurrent first-compiles of DISTINCT functions don't corrupt the tables,
//!   • a shared callee compiled via concurrent `jit_call`s stays sound,
//! with no panic / deadlock / data race (run under `cargo test`; ideally also TSan).
//! Value-correctness across engines is covered by the byte-identical self-build
//! (gen1==gen2) + the `--mode jit` golden suite.

use crate::jit::JitModule;
use crate::jit::frame::JitModuleCtx;
use crate::interp::ExecOutcome;
use crate::metadata::bytecode::{BasicBlock, CallInsn, Function, Instruction, Module, Terminator};
use crate::metadata::types::ExecMode;
use crate::vm_context::VmContext;
use std::sync::Arc;

const THREADS: usize = 16;

// ── hand-built function/module fixtures (mirror `lazy_tests.rs`) ─────────────

/// A JIT-translatable function whose body is a bare `return;`.
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

/// A translatable function that `Call`s `callee` (UNRESOLVED method_id → `jit_call`
/// resolves + lazily compiles the callee by name at runtime).
fn caller_of(name: &str, callee: &str) -> Function {
    let mut f = empty_fn(name);
    f.blocks[0].instructions = vec![Instruction::Call(Box::new(CallInsn {
        dst: 0,
        func: callee.to_string(),
        args: Vec::new().into_boxed_slice(),
        method_type_args: Box::default(),
    }))];
    f
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
        type_registry_vec: Vec::new(),
        func_index,
        func_ref_cache_slots: 0,
    }
}

/// Run `entry_name` on a fresh per-thread shell + VmContext over `shared` — exactly
/// what a `--jobs N` worker does. Returns whether the action returned normally.
fn run_on_worker_shell(shared: Arc<crate::jit::frame::JitShared>, entry_name: &str) -> bool {
    let vm = VmContext::new();
    let mut shell = JitModuleCtx { shared, vm_ctx: std::ptr::null_mut() };
    matches!(
        crate::jit::run_fn_on_shell(&mut shell, &vm, entry_name, &[]),
        Ok(ExecOutcome::Returned(_))
    )
}

// ── tests ────────────────────────────────────────────────────────────────────

#[test]
fn concurrent_same_fn_compiles_once() {
    let module = module_of("M", vec![empty_fn("f")]);
    let jm = JitModule::setup(&module).expect("setup");
    let shared = Arc::clone(&jm.ctx.shared);

    let handles: Vec<_> = (0..THREADS).map(|_| {
        let shared = Arc::clone(&shared);
        std::thread::spawn(move || run_on_worker_shell(shared, "f"))
    }).collect();
    for h in handles {
        assert!(h.join().expect("worker thread panicked"), "action must return normally");
    }

    // The single slot is filled exactly once (OnceLock) with a non-rejected entry.
    let entry = shared.fn_entries_by_id[0].get().expect("f must be compiled");
    assert!(!entry.is_rejected(), "f is JIT-translatable → must not be negative-cached");
    drop(jm);
}

#[test]
fn concurrent_distinct_fns_do_not_corrupt_tables() {
    let fns: Vec<Function> = (0..THREADS).map(|i| empty_fn(&format!("f{i}"))).collect();
    let module = module_of("M", fns);
    let jm = JitModule::setup(&module).expect("setup");
    let shared = Arc::clone(&jm.ctx.shared);

    let handles: Vec<_> = (0..THREADS).map(|i| {
        let shared = Arc::clone(&shared);
        std::thread::spawn(move || run_on_worker_shell(shared, &format!("f{i}")))
    }).collect();
    for (i, h) in handles.into_iter().enumerate() {
        assert!(h.join().expect("worker thread panicked"), "f{i} must return normally");
    }

    for i in 0..THREADS {
        assert!(shared.fn_entries_by_id[i].get().is_some(), "f{i} must be compiled");
    }
    drop(jm);
}

#[test]
fn concurrent_callers_compile_shared_callee_once() {
    let module = module_of("M", vec![caller_of("caller", "callee"), empty_fn("callee")]);
    let jm = JitModule::setup(&module).expect("setup");
    let shared = Arc::clone(&jm.ctx.shared);

    let handles: Vec<_> = (0..THREADS).map(|_| {
        let shared = Arc::clone(&shared);
        std::thread::spawn(move || run_on_worker_shell(shared, "caller"))
    }).collect();
    for h in handles {
        assert!(h.join().expect("worker thread panicked"), "caller must return normally");
    }

    // caller (slot 0) + callee (slot 1, compiled via concurrent jit_call) both filled.
    assert!(shared.fn_entries_by_id[0].get().is_some(), "caller must be compiled");
    assert!(shared.fn_entries_by_id[1].get().is_some(), "callee must be compiled");
    drop(jm);
}
