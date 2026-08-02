//! Unit tests for lazy per-function JIT (lazy-per-function-jit, 2026-07-23).
//!
//! These drive the lazy primitive directly — `JitModule::setup` + `run_fn` +
//! `JitModuleCtx::resolve_fn_by_id` — on hand-built empty/interp-only functions.
//! End-to-end coverage of real cross-function `Call`s (a caller compiling its
//! callee on first invocation) lives in the golden suite (`xtask test e2e
//! --mode jit`, all outputs byte-identical to interp); here we prove the
//! compile-on-first-call bookkeeping, the "uncalled ⇒ never compiled" invariant,
//! idempotency, the interp-only skip, and thread-safe single-compilation.

use crate::jit::JitModule;
use crate::metadata::bytecode::{BasicBlock, CallInsn, Function, Instruction, Module, Terminator};
use crate::metadata::types::ExecMode;
use crate::vm_context::VmContext;
use std::sync::atomic::Ordering;

/// A JIT-translatable function whose body is a bare `return;` (no `Call`, no
/// interp-only opcode → `jit_unsupported_reason` is `None`).
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
        resolved: std::sync::OnceLock::new(),
    }
}

/// A function containing an interp-only opcode (`LoadLocalAddr`) →
/// `jit_unsupported_reason` returns `Some`, so it must never be JIT-compiled.
fn interp_only_fn(name: &str) -> Function {
    let mut f = empty_fn(name);
    f.blocks[0].instructions = vec![Instruction::LoadLocalAddr { dst: 0, slot: 0 }];
    f
}

/// A JIT-translatable function whose body calls `callee` then returns. Its
/// `resolved` token table is left unset, so at translate time the `Call`'s
/// method_id is UNRESOLVED and `jit_call` resolves `callee` by name at runtime
/// (module.func_index) — the path that lazily compiles the callee.
fn caller_of(name: &str, callee: &str) -> Function {
    let mut f = empty_fn(name);
    f.blocks[0].instructions = vec![Instruction::Call(Box::new(CallInsn {
        dst: 0,
        func: callee.to_string(),
        args: Vec::new().into_boxed_slice(),
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
fn setup_compiles_nothing() {
    let module = module_of("M", vec![empty_fn("entry"), empty_fn("a"), empty_fn("b")]);
    let vm = VmContext::new();
    let _jm = JitModule::setup(&module).expect("setup");
    assert_eq!(compiled_count(&vm), 0, "setup must not compile any function");
}

#[test]
fn run_entry_compiles_only_the_entry() {
    let module = module_of("M", vec![empty_fn("entry"), empty_fn("unused_a"), empty_fn("unused_b")]);
    let vm = VmContext::new();
    let mut jm = JitModule::setup(&module).expect("setup");
    jm.run_fn(&vm, "entry").expect("run entry");
    assert_eq!(compiled_count(&vm), 1,
        "only the called entry compiles; the two unused functions stay uncompiled");
}

#[test]
fn second_run_does_not_recompile() {
    let module = module_of("M", vec![empty_fn("entry")]);
    let vm = VmContext::new();
    let mut jm = JitModule::setup(&module).expect("setup");
    jm.run_fn(&vm, "entry").expect("run 1");
    jm.run_fn(&vm, "entry").expect("run 2");
    assert_eq!(compiled_count(&vm), 1, "already-compiled entry is not recompiled on re-run");
}

#[test]
fn distinct_functions_each_compile_once() {
    let module = module_of("M", vec![empty_fn("a"), empty_fn("b")]);
    let vm = VmContext::new();
    let mut jm = JitModule::setup(&module).expect("setup");
    jm.run_fn(&vm, "a").expect("run a");
    jm.run_fn(&vm, "b").expect("run b");
    assert_eq!(compiled_count(&vm), 2, "each distinct called function compiles exactly once");
}

#[test]
fn interp_only_function_is_not_compiled() {
    // `resolve_fn_by_id` returns None for a function the JIT can't translate,
    // leaving it to the interpreter fallback — and never bumps the compile count.
    let module = module_of("M", vec![interp_only_fn("f")]);
    let vm = VmContext::new();
    let mut jm = JitModule::setup(&module).expect("setup");
    jm.ctx.vm_ctx = (&*vm as *const VmContext) as *mut VmContext;
    let resolved = unsafe { jm.ctx.resolve_fn_by_id(0) };
    assert!(resolved.is_none(), "interp-only function must not resolve to a JIT entry");
    assert_eq!(compiled_count(&vm), 0, "interp-only function is never compiled");
}

#[test]
fn caller_lazily_compiles_callee_mid_execution() {
    // The crux of lazy JIT: A is JIT-compiled and running native code; calling B
    // triggers B's lazy compile (declare + translate + finalize_definitions)
    // WHILE A's native frame is still live on the stack. This asserts that
    // finalizing a new function does not invalidate the already-running caller's
    // code pages — the property that makes per-function lazy compilation sound.
    let module = module_of("M", vec![caller_of("A", "B"), empty_fn("B")]);
    let vm = VmContext::new();
    let mut jm = JitModule::setup(&module).expect("setup");
    // runtime-jit-tiering: this test is about the mid-execution compile property
    // (orthogonal to tiering), so force threshold=1 → B compiles on its first
    // jit_call. (Deterministic regardless of Z42_JIT_THRESHOLD in the env.)
    jm.ctx.jit_threshold = 1;
    jm.run_fn(&vm, "A").expect("run A, which calls (and lazily compiles) B");
    assert_eq!(compiled_count(&vm), 2,
        "the caller A and its callee B — lazily compiled mid-call — are both compiled");
}

#[test]
fn tiering_cold_jit_call_callee_stays_interp() {
    // runtime-jit-tiering Phase 1a: a jit_call'd callee below the threshold does
    // NOT compile — it runs on the interpreter (cold tier). Here A (entry, always
    // compiled) calls B once with threshold=2 → B stays uncompiled.
    let module = module_of("M", vec![caller_of("A", "B"), empty_fn("B")]);
    let vm = VmContext::new();
    let mut jm = JitModule::setup(&module).expect("setup");
    jm.ctx.jit_threshold = 2;
    jm.run_fn(&vm, "A").expect("run A");
    assert_eq!(compiled_count(&vm), 1,
        "only entry A compiles; B (1 jit_call < threshold 2) stays on the interpreter");
}

#[test]
fn tiering_rejected_marker_is_negative_cache() {
    // The tri-state slot: a null-ptr FnEntry means Rejected (not JIT-translatable
    // / compile-failed), cached so `jit_unsupported_reason` isn't re-run every call.
    let r = crate::jit::frame::FnEntry::rejected();
    assert!(r.is_rejected(), "null-ptr FnEntry is the Rejected marker");
    assert!(r.ptr.is_null());
}

#[test]
fn concurrent_first_call_compiles_exactly_once() {
    // Two threads race to first-call the same slot; the Mutex + OnceLock
    // double-check must compile it exactly once.
    let module = module_of("M", vec![empty_fn("f")]);
    let vm = VmContext::new();
    let mut jm = JitModule::setup(&module).expect("setup");
    jm.ctx.vm_ctx = (&*vm as *const VmContext) as *mut VmContext;
    let ctx = &*jm.ctx; // &JitModuleCtx is Sync (unsafe impl)
    std::thread::scope(|s| {
        let h1 = s.spawn(|| unsafe { ctx.resolve_fn_by_id(0).is_some() });
        let h2 = s.spawn(|| unsafe { ctx.resolve_fn_by_id(0).is_some() });
        assert!(h1.join().expect("t1"), "thread 1 resolves a valid entry");
        assert!(h2.join().expect("t2"), "thread 2 resolves a valid entry");
    });
    assert_eq!(compiled_count(&vm), 1, "concurrent first-calls compile the function exactly once");
}
