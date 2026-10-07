//! P1-2: `Call` site tokens hold `FnId`s — resolve-time binding of calls inside
//! lazily loaded packages, and the interp call path reading those tokens.

use super::*;
use crate::metadata::bytecode::{BasicBlock, CallInsn, Instruction, Terminator};
use crate::metadata::tokens::{FnId, UNRESOLVED};
use crate::metadata::types::ExecMode;
use std::sync::atomic::Ordering;

fn func(name: &str, param_count: usize, instructions: Vec<Instruction>, ret: Option<u32>) -> Function {
    Function {
        name: name.to_string(),
        param_count,
        ret_type: "long".to_string(),
        exec_mode: ExecMode::Interp,
        blocks: vec![BasicBlock {
            label: "entry".to_string(),
            instructions,
            terminator: Terminator::Ret { reg: ret },
        }],
        is_static: true,
        visibility: 0,
        method_flags: 0, min_arg: 0, params_from: 0xFF,
        max_reg: 8,
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

fn call(dst: u32, callee: &str, args: &[u32]) -> Instruction {
    Instruction::Call(Box::new(CallInsn {
        dst,
        func: callee.to_string(),
        args: args.into(),
        method_type_args: Box::new([]),
    }))
}

/// `Lazy.G$0` returns 7 — the callee every test calls.
fn const7(name: &str) -> Function {
    func(name, 0, vec![Instruction::ConstI32 { dst: 0, val: 7 }], Some(0))
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

fn tokens(f: &Function) -> Vec<u32> {
    f.resolved.get().unwrap().method_tokens.iter().map(|t| t.load(Ordering::Relaxed)).collect()
}

fn returned(outcome: ExecOutcome) -> Value {
    match outcome {
        ExecOutcome::Returned(Some(v)) => v,
        _ => panic!("expected a returned value"),
    }
}

/// A function of a lazily loaded package starts out with its calls bound: into its
/// own package and into the entry module, by `FnId`. A site whose argument count the
/// callee cannot take, and a callee nobody registered, stay `UNRESOLVED`.
#[test]
fn lazy_package_calls_bind_to_fn_ids_at_resolve_time() {
    let ctx = VmContext::with_module(module("Entry", vec![const7("Entry.H$0")]));
    ctx.install_lazy_loader(None, 0);
    let g = register_lazy(&ctx, const7("Lazy.G$0"));
    let f = register_lazy(&ctx, func("Lazy.F$0", 0, vec![
        call(0, "Lazy.G$0", &[]),
        call(1, "Entry.H$0", &[]),
        call(2, "Lazy.G$0", &[0]),     // arity skew
        call(3, "Lazy.Missing$0", &[]),
    ], Some(0)));
    let entry = Arc::clone(ctx.module().unwrap());
    crate::metadata::resolver::resolve_function_tokens(&f, &entry, &ctx);

    let g_id = g.id.get().expect("registered");
    assert_eq!(g_id, FnId(1), "lazy ids follow the entry module's");
    assert_eq!(tokens(&f), vec![g_id.0, 0, UNRESOLVED, UNRESOLVED]);
}

/// The interp runs a bound lazy target straight off its token, and binds an
/// unresolved one on first dispatch — storing the callee's `FnId`.
#[test]
fn interp_call_runs_and_binds_lazy_targets_by_fn_id() {
    let ctx = VmContext::with_module(module("Entry", vec![
        func("Entry.Main$0", 0, vec![call(0, "Late.G$0", &[])], Some(0)),
    ]));
    ctx.install_lazy_loader(None, 0);
    let entry = Arc::clone(ctx.module().unwrap());
    let main = &entry.functions[0];
    crate::metadata::resolver::resolve_function_tokens(main, &entry, &ctx);
    assert_eq!(tokens(main), vec![UNRESOLVED], "not registered when Main resolved");

    let g = register_lazy(&ctx, const7("Late.G$0"));
    let v = returned(super::super::exec_function(&ctx, &entry, main, &[]).unwrap());
    assert!(matches!(v, Value::I64(7)));
    assert_eq!(tokens(main), vec![g.id.get().unwrap().0], "first dispatch stores the FnId");
    // Second run: straight off the token.
    let v = returned(super::super::exec_function(&ctx, &entry, main, &[]).unwrap());
    assert!(matches!(v, Value::I64(7)));
}

/// Against a module that is not the VM's entry module (bare `VmContext::new()`),
/// tokens stay that module's own indices: a lazy callee is never cached, since its
/// `FnId` could equal an index of the module.
#[test]
fn non_entry_module_never_caches_a_lazy_fn_id() {
    let ctx = VmContext::new();
    ctx.install_lazy_loader(None, 0);
    let g = register_lazy(&ctx, const7("Lazy.G$0"));
    assert_eq!(g.id.get(), Some(FnId(0)), "same number as the module's first function");
    let m = module("M", vec![
        func("M.A$0", 0, vec![call(0, "Lazy.G$0", &[]), call(1, "M.B$0", &[])], Some(0)),
        const7("M.B$0"),
    ]);
    let a = &m.functions[0];
    crate::metadata::resolver::resolve_function_tokens(a, &m, &ctx);
    assert_eq!(tokens(a), vec![UNRESOLVED, 1]);

    let v = returned(super::super::exec_function(&ctx, &m, a, &[]).unwrap());
    assert!(matches!(v, Value::I64(7)));
    assert_eq!(tokens(a), vec![UNRESOLVED, 1], "the lazy binding is not cached");
}

/// A lazy function shadowed by an entry-module name: name lookup (resolve and
/// first dispatch alike) keeps preferring the entry function.
#[test]
fn entry_function_wins_over_a_same_named_lazy_one() {
    let ctx = VmContext::with_module(module("Entry", vec![
        func("Entry.Main$0", 0, vec![call(0, "Shared.F$0", &[])], Some(0)),
        const7("Shared.F$0"),
    ]));
    ctx.install_lazy_loader(None, 0);
    let lazy = register_lazy(&ctx, func("Shared.F$0", 0, vec![Instruction::ConstI32 { dst: 0, val: 9 }], Some(0)));
    assert_eq!(lazy.id.get(), Some(FnId(2)), "the shadowed lazy function has its own id");
    let entry = Arc::clone(ctx.module().unwrap());
    let main = &entry.functions[0];
    crate::metadata::resolver::resolve_function_tokens(main, &entry, &ctx);
    assert_eq!(tokens(main), vec![1]);
    let v = returned(super::super::exec_function(&ctx, &entry, main, &[]).unwrap());
    assert!(matches!(v, Value::I64(7)));
}

/// Reinstalling the loader (tests only) starts a fresh name space: the old
/// loader's functions no longer resolve by name.
#[test]
fn reinstalled_loader_does_not_see_old_names() {
    let ctx = VmContext::with_module(module("Entry", vec![const7("Entry.H$0")]));
    ctx.install_lazy_loader(None, 0);
    register_lazy(&ctx, const7("Lazy.G$0"));
    assert!(ctx.funcs().id_of("Lazy.G$0").is_some());
    ctx.install_lazy_loader(None, 0);
    assert!(ctx.funcs().id_of("Lazy.G$0").is_none());
    assert!(ctx.try_lookup_function("Lazy.G$0").is_none());
    let g2 = register_lazy(&ctx, const7("Lazy.G$0"));
    assert_eq!(g2.id.get(), Some(FnId(2)), "ids are never reused");
}
