//! The VM's JIT across threads: compiled code and the tiering entry points work
//! on any thread of the VM, each thread passing its own `VmContext` as the
//! context parameter.
//!
//! Hand-built functions, as in `lazy_tests.rs`; the end-to-end threading suite
//! (`z42.threading` tests, `--mode jit`) covers real `Thread` workloads.

use std::sync::atomic::Ordering;

use crate::vm::exec::call::frames::VmFrame;
use crate::jit::runtime::frame::JitFrame;
use crate::jit::runtime::invoke::{call_entry, NativeOutcome};
use crate::jit::compile::state::{install, install_tuned};
use crate::vm::runtime::{Function, Module};
use crate::metadata::ir::{BasicBlock, CallInsn, Instruction, Terminator};
use crate::metadata::ir::ExecMode;
use crate::object::Value;
use crate::vm::context::VmContext;

fn func(name: &str, instructions: Vec<Instruction>, terminator: Terminator) -> Function {
    Function::new(crate::metadata::ir::Function {
        name: name.to_string(),
        param_count: 0,
        ret_type: "void".to_string(),
        exec_mode: ExecMode::Jit,
        blocks: vec![BasicBlock {
            label: "entry".to_string(),
            instructions,
            terminator,
        }],
        is_static: false,
        visibility: 0,
        method_flags: 0,
        min_arg: 0,
        params_from: 0xFF,
        max_reg: 0,
        cold: None,
        reg_types: Box::new([]),
        block_index: std::collections::HashMap::new(),
        branch_targets: Vec::new(),
        frame_meta: None,
    })
}

/// `return <val>;`
fn returns(name: &str, val: i64) -> Function {
    func(name, vec![Instruction::ConstI64 { dst: 0, val }], Terminator::Ret { reg: Some(0) })
}

/// `throw <val>;`
fn throws(name: &str, val: i64) -> Function {
    func(name, vec![Instruction::ConstI64 { dst: 0, val }], Terminator::Throw { reg: 0 })
}

/// `callee(); return;` — the call binds by name at run time.
fn calls(name: &str, callee: &str) -> Function {
    func(name, vec![Instruction::Call(Box::new(CallInsn {
        dst: 0,
        func: callee.to_string(),
        args: Vec::new().into_boxed_slice(),
        method_type_args: Box::default(),
    }))], Terminator::Ret { reg: None })
}

fn module_of(functions: Vec<Function>) -> Module {
    let func_index = functions.iter().enumerate().map(|(i, f)| (f.name.clone(), i)).collect();
    Module::new(crate::metadata::image::ModuleData {
        name: "M".to_string(),
        string_pool: Vec::new(),
        classes: Vec::new(),
        func_index,
    }, functions, rustc_hash::FxHashMap::default())
}

fn compiled_here(vm: &VmContext) -> u64 {
    vm.counters().jit_methods_compiled.load(Ordering::Relaxed)
}

fn run(vm: &VmContext, entry: &crate::jit::runtime::frame::FnEntry) -> NativeOutcome {
    unsafe { call_entry(vm, entry, JitFrame::new(vm, entry.max_reg, &[])) }
}

#[test]
fn every_context_of_a_vm_sees_the_same_jit() {
    let module = module_of(vec![returns("f", 1)]);
    let vm = VmContext::new();
    let rt = install(&vm, &module, crate::config::runtime_config().jit_config()).expect("install");
    let w = VmContext::new_with_core(vm.core_arc());
    assert!(std::ptr::eq(w.jit().expect("worker sees the JIT"), rt));
    assert!(std::ptr::eq(install(&w, &module, crate::config::runtime_config().jit_config()).expect("reinstall is a no-op"), rt));
    let other = module_of(vec![returns("g", 2)]);
    assert!(install(&w, &other, crate::config::runtime_config().jit_config()).is_err(), "one JIT per VM");
}

#[test]
fn a_context_older_than_the_install_picks_the_jit_up_at_its_next_entry() {
    let module = module_of(vec![returns("f", 5)]);
    let vm = VmContext::new();
    let w = VmContext::new_with_core(vm.core_arc());
    install(&vm, &module, crate::config::runtime_config().jit_config()).expect("install");
    assert!(w.jit().is_none(), "not until its next bottom frame push");
    let regs: Vec<Value> = Vec::new();
    w.push_frame(VmFrame::new(&module.functions[0], &regs));
    w.pop_frame();
    assert!(std::ptr::eq(w.jit().expect("picked up"), vm.jit().unwrap()));
}

#[test]
fn function_compiled_on_one_thread_runs_natively_on_another() {
    let module = module_of(vec![returns("f", 42)]);
    let vm = VmContext::new();
    let rt = install(&vm, &module, crate::config::runtime_config().jit_config()).expect("install");
    let code = rt.resolve_fn_by_id(&vm, 0).expect("compiles on the main thread").ptr as usize;
    assert_eq!(compiled_here(&vm), 1);
    std::thread::scope(|s| {
        s.spawn(|| {
            let w = VmContext::new_with_core(vm.core_arc());
            let e = w.jit().expect("shared JIT").resolve_fn_by_id_tiered(&w, 0)
                .expect("already compiled: a hit, no tier counting");
            assert_eq!(e.ptr as usize, code, "the main thread's code");
            match run(&w, e) {
                NativeOutcome::Returned(Some(Value::I64(42))) => {}
                NativeOutcome::Returned(v) => panic!("unexpected return {v:?}"),
                NativeOutcome::Threw => panic!("threw {:?}", w.take_exception()),
            }
            assert_eq!(compiled_here(&w), 0, "nothing compiled on the worker");
            assert_eq!(w.exec.call_stack.depth(), 0, "the native frame was pushed on and popped off the worker's stack");
        }).join().expect("worker");
    });
    assert_eq!(vm.counters_snapshot().jit_methods_compiled, 1, "compiled exactly once VM-wide");
}

#[test]
fn worker_reaching_the_tier_threshold_compiles_in_thread() {
    // `A` calls `B`; the worker runs `A` on the interpreter three times. The
    // interpreter's per-site divert counts `B` on the shared slot, the worker
    // compiles it at the threshold and runs it natively from then on.
    let module = module_of(vec![calls("A", "B"), returns("B", 7)]);
    let vm = VmContext::new();
    install_tuned(&vm, &module, |rt| rt.jit_threshold = 2);
    crate::vm::runtime::resolve::resolve_function_tokens(&module.functions[0], &module, &vm);
    std::thread::scope(|s| {
        s.spawn(|| {
            let w = VmContext::new_with_core(vm.core_arc());
            for _ in 0..3 {
                let r = crate::interp::run::exec_function(&w, &module, &module.functions[0], &[]);
                assert!(matches!(r, Ok(crate::vm::exec::call::outcome::ExecOutcome::Returned(_))), "{r:?}");
            }
            assert_eq!(compiled_here(&w), 1, "B compiled on the worker");
            assert_eq!(w.counters().jit_native_from_interp.load(Ordering::Relaxed), 2,
                "calls 2 and 3 ran natively");
            assert_eq!(w.jit().unwrap().slot_probe(1), (2, false));
        }).join().expect("worker");
    });
    assert_eq!(compiled_here(&vm), 0, "nothing compiled on the main thread");
    assert!(vm.jit().unwrap().peek_fn_by_id(1).is_some(), "the main thread sees the worker's code");
}

#[test]
fn exception_thrown_in_jit_code_on_a_worker_stays_on_the_worker() {
    let module = module_of(vec![throws("f", 13)]);
    let vm = VmContext::new();
    let rt = install(&vm, &module, crate::config::runtime_config().jit_config()).expect("install");
    rt.resolve_fn_by_id(&vm, 0).expect("compiles");
    std::thread::scope(|s| {
        s.spawn(|| {
            let w = VmContext::new_with_core(vm.core_arc());
            let e = w.jit().unwrap().resolve_fn_by_id(&w, 0).expect("compiled");
            assert!(matches!(run(&w, e), NativeOutcome::Threw));
            assert!(matches!(w.take_exception(), Some(Value::I64(13))), "pending on the worker");
            assert_eq!(w.exec.call_stack.depth(), 0, "unwound");
        }).join().expect("worker");
    });
    assert!(vm.peek_exception().is_none(), "the main thread's pending slot is untouched");
}

#[test]
fn compiled_prologue_checks_the_running_threads_stack_limit() {
    let module = module_of(vec![returns("f", 1), calls("r", "r")]);
    let vm = VmContext::new();
    install_tuned(&vm, &module, |rt| rt.jit_threshold = 1);
    // Fatal already set: an overflow below still unwinds as one, but leaves the
    // process-wide report (which other tests read) alone.
    vm.core.threads.fatal.store(true, Ordering::SeqCst);
    let main_limit = crate::vm::exec::stack_guard::limit();
    let core = vm.core_arc();
    let (stored, own, forced, deep) = std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || {
            let w = VmContext::new_with_core(core);
            let rt = w.jit().unwrap();
            let f = rt.resolve_fn_by_id(&w, 0).expect("compiled");
            // The bottom push stores this thread's limit; the call succeeds.
            assert!(matches!(run(&w, f), NativeOutcome::Returned(Some(Value::I64(1)))));
            let stored = w.exec.jit_head.stack_limit.load(Ordering::Relaxed);
            // Under an outer frame (so no bottom push resets it), a limit above every
            // stack address makes the prologue report an overflow: it reads the
            // context's own head.
            let regs: Vec<Value> = Vec::new();
            w.push_frame(VmFrame::new(f.func, &regs));
            w.exec.jit_head.stack_limit.store(usize::MAX, Ordering::Relaxed);
            let forced = matches!(run(&w, f), NativeOutcome::Threw)
                && matches!(w.take_exception(), Some(Value::Str(s)) if s.as_str().contains("stack overflow"));
            w.pop_frame();
            // Unbounded recursion on this thread's 8 MiB stack stops at its own limit.
            let deep = crate::jit::run_fn(&w, "r").expect_err("overflows").to_string();
            (stored, crate::vm::exec::stack_guard::limit(), forced, deep)
        })
        .expect("spawn")
        .join()
        .expect("worker");
    assert_eq!(stored, own, "the worker's context carries the worker's limit");
    assert_ne!(stored, main_limit, "not the main thread's");
    assert!(forced, "the prologue compares against the context's limit");
    assert!(deep.contains("stack overflow"), "{deep}");
}

#[test]
fn explicit_configs_drive_independent_vm_tier_thresholds() {
    let fast_config = crate::config::RuntimeConfig::from_getter(|key| match key {
        "Z42_JIT_THRESHOLD" => Some("1".to_string()),
        _ => None,
    }).jit_config();
    let slow_config = crate::config::RuntimeConfig::from_getter(|key| match key {
        "Z42_JIT_THRESHOLD" => Some("3".to_string()),
        _ => None,
    }).jit_config();
    let fast_module = module_of(vec![returns("f", 7)]);
    let slow_module = module_of(vec![returns("f", 7)]);
    let fast = VmContext::new();
    let slow = VmContext::new();
    let fast_rt = install(&fast, &fast_module, fast_config).unwrap();
    let slow_rt = install(&slow, &slow_module, slow_config).unwrap();
    assert!(fast_rt.resolve_fn_by_id_tiered(&fast, 0).is_some());
    assert!(slow_rt.resolve_fn_by_id_tiered(&slow, 0).is_none());
    assert!(slow_rt.resolve_fn_by_id_tiered(&slow, 0).is_none());
    assert_eq!(compiled_here(&fast), 1);
    assert_eq!(compiled_here(&slow), 0);
    let slow_entry = slow_rt.resolve_fn_by_id_tiered(&slow, 0).unwrap();
    assert!(matches!(run(&slow, slow_entry), NativeOutcome::Returned(Some(Value::I64(7)))));
    assert_eq!(compiled_here(&slow), 1);
}

#[test]
fn reinstall_keeps_the_published_config_and_tier() {
    let config = crate::config::RuntimeConfig::from_getter(|key| match key {
        "Z42_JIT_THRESHOLD" => Some("3".to_string()),
        _ => None,
    }).jit_config();
    let module = module_of(vec![returns("f", 9)]);
    let vm = VmContext::new();
    let rt = install(&vm, &module, config).unwrap();
    let mut later = config;
    later.threshold = 1;
    assert!(std::ptr::eq(rt, install(&vm, &module, later).unwrap()));
    assert!(rt.resolve_fn_by_id_tiered(&vm, 0).is_none());
    assert!(rt.resolve_fn_by_id_tiered(&vm, 0).is_none());
    assert!(rt.resolve_fn_by_id_tiered(&vm, 0).is_some());
    assert_eq!(compiled_here(&vm), 1);
}

#[test]
fn explicit_mixed_entry_config_counts_before_compiling() {
    use crate::vm::exec::engine::{TierBackend, TierTarget};
    use crate::vm::exec::call::{args::CallArgs, outcome::ExecOutcome};
    let config = crate::config::RuntimeConfig::from_getter(|key| match key {
        "Z42_JIT_THRESHOLD" => Some("10".to_string()),
        "Z42_JIT_INTERP_TIERUP" => Some("2".to_string()),
        "Z42_OSR_THRESHOLD" => Some("7".to_string()),
        _ => None,
    }).jit_config();
    let module = module_of(vec![returns("f", 11)]);
    let vm = VmContext::new();
    let rt = install(&vm, &module, config).unwrap();
    assert_eq!(rt.osr_threshold(), 7);
    assert!(rt.invoke(&vm, TierTarget::Existing(&module.functions[0]), CallArgs::Values(&[]), &[]).is_none());
    assert_eq!(compiled_here(&vm), 0);
    let outcome = rt.invoke(&vm, TierTarget::Existing(&module.functions[0]), CallArgs::Values(&[]), &[]).unwrap().unwrap();
    assert!(matches!(outcome, ExecOutcome::Returned(Some(Value::I64(11)))));
    assert_eq!(compiled_here(&vm), 1);
}
