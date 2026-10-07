//! P1-2 PR 8: `CallIndirect` dispatches on the callee value's bound `FnId` through the JIT
//! slot table — a lazily loaded target is counted and compiled at the tier threshold like
//! any `Call`, from native code and from an interpreter frame alike.

use crate::jit::JitModule;
use crate::metadata::bytecode::{BasicBlock, Function, Instruction, LoadFnInsn, Module, Terminator};
use crate::metadata::resolver::ResolvedTokens;
use crate::metadata::tokens::UNRESOLVED;
use crate::metadata::types::ExecMode;
use crate::vm_context::VmContext;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

fn func(name: &str, instructions: Vec<Instruction>) -> Function {
    Function {
        name: name.to_string(),
        param_count: 0,
        ret_type: "void".to_string(),
        exec_mode: ExecMode::Jit,
        blocks: vec![BasicBlock {
            label: "entry".to_string(),
            instructions,
            terminator: Terminator::Ret { reg: None },
        }],
        is_static: true,
        visibility: 0,
        method_flags: 0, min_arg: 0, params_from: 0xFF,
        max_reg: 2,
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

/// `f = Lib.F; f(); f(); f();`
fn main_calling_lib_f_three_times() -> Function {
    let call = || Instruction::CallIndirect { dst: 1, callee: 0, args: Box::new([]) };
    func("App.Main", vec![
        Instruction::LoadFn(Box::new(LoadFnInsn { dst: 0, func: "Lib.F".to_string() })),
        call(), call(), call(),
    ])
}

fn module_of(functions: Vec<Function>) -> Module {
    let func_index = functions.iter().enumerate().map(|(i, f)| (f.name.clone(), i)).collect();
    Module {
        name: "App".to_string(),
        string_pool: Vec::new(),
        classes: Vec::new(),
        functions,
        type_registry: rustc_hash::FxHashMap::default(),
        func_index,
    }
}

/// A VM whose entry module is `App.Main` + a lazily loaded `Lib.F`; returns `Lib.F`.
fn vm_with_lazy_target(main: Function) -> (std::pin::Pin<Box<VmContext>>, Arc<Function>) {
    let vm = VmContext::with_module(module_of(vec![main]));
    vm.install_lazy_loader(None, 0);
    let f = Arc::new(func("Lib.F", vec![]));
    assert!(vm.core.lazy_loader.write().as_mut().unwrap().insert_function(f.name.clone(), Arc::clone(&f)));
    (vm, f)
}

fn compiled(vm: &VmContext) -> u64 {
    vm.counters().jit_methods_compiled.load(Ordering::Relaxed)
}

/// The site is bound before `App.Main` compiles (resolver pre-fill), so `LoadFn` is a
/// native store; the three indirect calls count `Lib.F`'s slot by its `FnId` and the
/// second one compiles it (threshold 2).
#[test]
fn call_indirect_counts_and_compiles_the_lazy_target_slot() {
    let (vm, f) = vm_with_lazy_target(main_calling_lib_f_three_times());
    let id = f.id.get().expect("registered").0 as usize;
    let mut jm = JitModule::setup(vm.module().unwrap()).expect("setup");
    jm.ctx.jit_threshold = 2;
    jm.run_fn(&vm, "App.Main").expect("runs");
    let main = &vm.module().unwrap().functions[0];
    let tok = main.resolved.get().unwrap().fn_ref_tokens[0].load(Ordering::Relaxed);
    assert_eq!(tok as usize, id, "LoadFn site bound to the lazy FnId");
    assert_eq!(jm.ctx.slot_probe(id), (2, false), "counted on its own slot, compiled at 2");
    assert!(jm.ctx.peek_fn_by_id(id).is_some(), "Lib.F compiled");
    assert_eq!(compiled(&vm), 2, "App.Main + Lib.F");
}

/// A site still unbound when its function compiles goes through `jit_load_fn`, which
/// binds the target on first execution and stores the token for later ones.
#[test]
fn unbound_load_fn_site_binds_on_first_execution() {
    let main = main_calling_lib_f_three_times();
    let _ = main.resolved.set(ResolvedTokens {
        fn_ref_tokens: vec![AtomicU32::new(UNRESOLVED)],
        site_index: vec![vec![0, UNRESOLVED, UNRESOLVED, UNRESOLVED]],
        ..Default::default()
    });
    let (vm, f) = vm_with_lazy_target(main);
    let id = f.id.get().unwrap().0;
    let mut jm = JitModule::setup(vm.module().unwrap()).expect("setup");
    jm.ctx.jit_threshold = 2;
    jm.run_fn(&vm, "App.Main").expect("runs");
    let main = &vm.module().unwrap().functions[0];
    assert_eq!(main.resolved.get().unwrap().fn_ref_tokens[0].load(Ordering::Relaxed), id);
    assert_eq!(jm.ctx.slot_probe(id as usize), (2, false));
}

/// An undefined target throws from `jit_load_fn` (at creation), before any call.
#[test]
fn undefined_target_throws_from_the_load_fn_helper() {
    let main = func("App.Main", vec![
        Instruction::LoadFn(Box::new(LoadFnInsn { dst: 0, func: "Nowhere.F".to_string() })),
    ]);
    let vm = VmContext::with_module(module_of(vec![main]));
    vm.install_lazy_loader(None, 0);
    let mut jm = JitModule::setup(vm.module().unwrap()).expect("setup");
    let err = jm.run_fn(&vm, "App.Main").expect_err("throws");
    assert!(err.to_string().contains("undefined function `Nowhere.F`"), "{err}");
}

/// From an interpreter frame (a cold-tier caller under a JIT run), `CallIndirect` counts
/// the target on the same slot and runs it natively once compiled.
#[test]
fn interp_call_indirect_routes_to_the_jit_slot() {
    let (vm, f) = vm_with_lazy_target(main_calling_lib_f_three_times());
    let id = f.id.get().unwrap().0 as usize;
    let module = Arc::clone(vm.module().unwrap());
    let mut jm = JitModule::setup(&module).expect("setup");
    jm.ctx.jit_threshold = 2;
    jm.ctx.vm_ctx = (&*vm as *const VmContext) as *mut VmContext;
    vm.set_jit_ctx(&*jm.ctx as *const crate::jit::frame::JitModuleCtx as usize);
    let main = &module.functions[0];
    crate::metadata::resolver::resolve_function_tokens(main, &module, &vm);
    let before = vm.counters().jit_native_from_interp.load(Ordering::Relaxed);
    let r = crate::interp::exec_function(&vm, &module, main, &[]);
    vm.set_jit_ctx(0);
    jm.ctx.vm_ctx = std::ptr::null_mut();
    assert!(matches!(r, Ok(crate::interp::ExecOutcome::Returned(_))));
    assert_eq!(jm.ctx.slot_probe(id), (2, false));
    assert_eq!(vm.counters().jit_native_from_interp.load(Ordering::Relaxed) - before, 2,
        "calls 2 and 3 ran natively");
}
