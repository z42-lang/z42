//! P1-2 PR 8: `LoadFn` / `MkClos` bind their target's `FnId` when the value is created,
//! and `CallIndirect` dispatches on it — no name on the call path.

use super::*;
use crate::metadata::bytecode::{BasicBlock, Instruction, LoadFnInsn, MkClosInsn, Terminator};
use crate::metadata::tokens::{FnId, UNRESOLVED};
use crate::metadata::types::ExecMode;
use std::sync::atomic::{AtomicU32, Ordering};

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
        forward: Default::default(),
    }
}

fn const_fn(name: &str, val: i32) -> Function {
    func(name, 0, vec![Instruction::ConstI32 { dst: 0, val }], Some(0))
}

fn load_fn(dst: u32, name: &str) -> Instruction {
    Instruction::LoadFn(Box::new(LoadFnInsn { dst, func: name.to_string() }))
}

fn mk_clos(dst: u32, name: &str, captures: &[u32]) -> Instruction {
    Instruction::MkClos(Box::new(MkClosInsn { dst, fn_name: name.to_string(), captures: captures.into() }))
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

fn fn_ref_tokens(f: &Function) -> Vec<u32> {
    f.resolved.get().unwrap().fn_ref_tokens.iter().map(|t| t.load(Ordering::Relaxed)).collect()
}

fn returned(outcome: ExecOutcome) -> Value {
    match outcome {
        ExecOutcome::Returned(Some(v)) => v,
        ExecOutcome::Returned(None) => panic!("returned nothing"),
        ExecOutcome::Thrown(v) => panic!("threw {v:?}"),
    }
}

fn thrown_message(ctx: &VmContext, module: &Module, v: &Value) -> String {
    crate::exception::read_message(v, ctx, module).unwrap_or_else(|| crate::corelib::convert::value_to_str(v))
}

/// An entry-module target binds to its `module.functions` index (= its `FnId`) and the
/// site keeps it: a later execution reads the token, not the name.
#[test]
fn entry_target_binds_its_index_and_the_site_keeps_it() {
    let ctx = VmContext::with_module(module("Entry", vec![const_fn("Entry.A$0", 1), const_fn("Entry.B$0", 2)]));
    let entry = Arc::clone(ctx.module().unwrap());
    let mut frame = Frame::new(&ctx, &[], 4);
    let tok = AtomicU32::new(UNRESOLVED);
    assert!(load_fn_with(&ctx, &entry, &mut frame, "Entry.B$0", Some(&tok)).is_none());
    assert_eq!(*frame.get(0).unwrap(), Value::FuncRef(FnId(1)));
    assert_eq!(tok.load(Ordering::Relaxed), 1, "binding stored into the site");
    // A bound site never looks at the name again (a token naming A wins over the name B).
    tok.store(0, Ordering::Relaxed);
    assert!(load_fn_with(&ctx, &entry, &mut frame, "Entry.B$0", Some(&tok)).is_none());
    assert_eq!(*frame.get(0).unwrap(), Value::FuncRef(FnId(0)));
}

fn load_fn_with(ctx: &VmContext, m: &Module, frame: &mut Frame, name: &str, tok: Option<&AtomicU32>) -> Option<Value> {
    super::load_fn(ctx, m, frame, 0, name, tok)
}

/// A target in a lazily loaded package binds to the function's `FnId` at creation; the
/// value then dispatches by that id even once the name no longer resolves anywhere.
#[test]
fn lazy_target_binds_its_fn_id_and_dispatches_without_its_name() {
    let ctx = VmContext::with_module(module("Entry", vec![const_fn("Entry.H$0", 1)]));
    ctx.install_lazy_loader(None, 0);
    let g = register_lazy(&ctx, const_fn("Lazy.G$0", 7));
    let entry = Arc::clone(ctx.module().unwrap());
    let mut frame = Frame::new(&ctx, &[], 4);
    let tok = AtomicU32::new(UNRESOLVED);
    assert!(load_fn_with(&ctx, &entry, &mut frame, "Lazy.G$0", Some(&tok)).is_none());
    let id = g.id.get().expect("registered");
    assert_eq!(id, FnId(1), "lazy ids follow the entry module's");
    assert_eq!(*frame.get(0).unwrap(), Value::FuncRef(id));
    assert_eq!(tok.load(Ordering::Relaxed), id.0);

    // Fresh loader name space, a context without the per-ctx lookup cache: the name is
    // gone, the value (and the bound site) still work.
    ctx.install_lazy_loader(None, 0);
    let ctx2 = VmContext::new_with_core(Arc::clone(&ctx.core));
    assert!(ctx2.try_lookup_function("Lazy.G$0").is_none());
    let mut frame2 = Frame::new(&ctx2, &[], 4);
    assert!(load_fn_with(&ctx2, &entry, &mut frame2, "Lazy.G$0", Some(&tok)).is_none());
    assert!(super::call_indirect(&ctx2, &entry, &mut frame2, 1, 0, &[]).unwrap().is_none());
    assert_eq!(*frame2.get(1).unwrap(), Value::I64(7));
}

/// A closure binds the same way; its env is still the callee's implicit first argument.
#[test]
fn closure_binds_its_fn_id_and_passes_the_env() {
    // `Lazy.Add$0(env, x)` = env[0] + x.
    let add = func("Lazy.Add$0", 2, vec![
        Instruction::ConstI32 { dst: 2, val: 0 },
        Instruction::ArrayGet { dst: 3, arr: 0, idx: 2 },
        Instruction::Add { dst: 4, a: 3, b: 1 },
    ], Some(4));
    let ctx = VmContext::with_module(module("Entry", vec![const_fn("Entry.H$0", 1)]));
    ctx.install_lazy_loader(None, 0);
    let add = register_lazy(&ctx, add);
    let entry = Arc::clone(ctx.module().unwrap());
    let mut frame = Frame::new(&ctx, &[], 8);
    frame.set(5, Value::I64(40));
    frame.set(6, Value::I64(2));
    let tok = AtomicU32::new(UNRESOLVED);
    assert!(super::mk_clos(&ctx, &entry, &mut frame, 0, "Lazy.Add$0", &[5], Some(&tok)).unwrap().is_none());
    let v = *frame.get(0).unwrap();
    assert_eq!(v.callable_id(), add.id.get());
    assert_eq!(tok.load(Ordering::Relaxed), add.id.get().unwrap().0);
    assert!(super::call_indirect(&ctx, &entry, &mut frame, 1, 0, &[6]).unwrap().is_none());
    assert_eq!(*frame.get(1).unwrap(), Value::I64(42));
}

/// An undefined target throws when the value is created (catchable, the `Call` path's
/// wording) — not at the first invocation — and nothing is cached.
#[test]
fn undefined_target_throws_at_creation() {
    let ctx = VmContext::with_module(module("Entry", vec![const_fn("Entry.H$0", 1)]));
    ctx.install_lazy_loader(None, 0);
    let entry = Arc::clone(ctx.module().unwrap());
    let mut frame = Frame::new(&ctx, &[], 4);
    frame.set(0, Value::I64(5));
    let tok = AtomicU32::new(UNRESOLVED);
    let exc = load_fn_with(&ctx, &entry, &mut frame, "Nowhere.F$0", Some(&tok)).expect("throws");
    assert!(thrown_message(&ctx, &entry, &exc).contains("undefined function `Nowhere.F$0`"));
    assert_eq!(*frame.get(0).unwrap(), Value::I64(5), "dst untouched");
    assert_eq!(tok.load(Ordering::Relaxed), UNRESOLVED);
    let exc = super::mk_clos(&ctx, &entry, &mut frame, 0, "Nowhere.G$0", &[], Some(&tok)).unwrap().expect("throws");
    assert!(thrown_message(&ctx, &entry, &exc).contains("undefined function `Nowhere.G$0`"));
    assert_eq!(tok.load(Ordering::Relaxed), UNRESOLVED);
}

/// Delegate equality is id equality: two sites referencing one function (entry or lazy)
/// produce equal values; different functions, or a closure vs a `FuncRef`, do not. The
/// reflection name comes back from the id.
#[test]
fn delegate_equality_and_name_follow_the_bound_id() {
    let ctx = VmContext::with_module(module("Entry", vec![const_fn("Entry.A$0", 1), const_fn("Entry.B$0", 2)]));
    ctx.install_lazy_loader(None, 0);
    register_lazy(&ctx, const_fn("Lazy.G$0", 7));
    let entry = Arc::clone(ctx.module().unwrap());
    let mut frame = Frame::new(&ctx, &[], 8);
    let (t1, t2, t3, t4) = (AtomicU32::new(UNRESOLVED), AtomicU32::new(UNRESOLVED), AtomicU32::new(UNRESOLVED), AtomicU32::new(UNRESOLVED));
    super::load_fn(&ctx, &entry, &mut frame, 0, "Entry.A$0", Some(&t1));
    super::load_fn(&ctx, &entry, &mut frame, 1, "Entry.A$0", Some(&t2));
    super::load_fn(&ctx, &entry, &mut frame, 2, "Lazy.G$0", Some(&t3));
    super::load_fn(&ctx, &entry, &mut frame, 3, "Lazy.G$0", Some(&t4));
    super::load_fn(&ctx, &entry, &mut frame, 4, "Entry.B$0", None);
    super::mk_clos(&ctx, &entry, &mut frame, 5, "Entry.A$0", &[], None).unwrap();
    let r = |i: u32| *frame.get(i).unwrap();
    let eq = |a: Value, b: Value| crate::corelib::exec_builtin_value(&ctx, "__delegate_eq", &[a, b]).unwrap();
    assert_eq!(eq(r(0), r(1)), Value::Bool(true), "same entry function, two sites");
    assert_eq!(eq(r(2), r(3)), Value::Bool(true), "same lazy function, two sites");
    assert_eq!(eq(r(0), r(4)), Value::Bool(false));
    assert_eq!(eq(r(0), r(2)), Value::Bool(false));
    assert_eq!(eq(r(0), r(5)), Value::Bool(false), "FuncRef vs closure");
    assert!(r(0) == r(1), "`==` agrees");

    let name = |v: Value| crate::corelib::exec_builtin_value(&ctx, "__delegate_fn_name", &[v]).unwrap();
    assert_eq!(name(r(2)), Value::Str("Lazy.G$0".into()));
    assert_eq!(name(r(5)), Value::Str("Entry.A$0".into()));
    // `__make_closure` (WeakRef<TD>.Get) binds the name it is given.
    let env = ctx.heap().alloc_array(vec![]);
    let rebuilt = crate::corelib::exec_builtin_value(&ctx, "__make_closure", &[Value::Str("Lazy.G$0".into()), env]).unwrap();
    assert_eq!(rebuilt.callable_id(), r(2).callable_id());
    // Display names the function (the ctx-aware path interpolation / WriteLine use).
    let shown = crate::interp::dispatch::stringify_dispatch(&ctx, &r(2)).unwrap();
    assert_eq!(shown, "<fn Lazy.G$0>");
}

/// The resolver pre-fills `LoadFn` / `MkClos` sites from what is registered (entry
/// first, then lazily loaded functions); a target not registered yet binds on first
/// execution, and the interpreter's `LoadFn` → `CallIndirect` runs end to end.
#[test]
fn resolver_prefills_and_first_execution_binds() {
    let ctx = VmContext::with_module(module("Entry", vec![
        func("Entry.Main$0", 0, vec![
            load_fn(0, "Lazy.G$0"),
            mk_clos(1, "Entry.H$0", &[]),
            load_fn(2, "Late.F$0"),
            Instruction::CallIndirect { dst: 3, callee: 2, args: Box::new([]) },
        ], Some(3)),
        const_fn("Entry.H$0", 1),
    ]));
    ctx.install_lazy_loader(None, 0);
    let g = register_lazy(&ctx, const_fn("Lazy.G$0", 7));
    let entry = Arc::clone(ctx.module().unwrap());
    let main = &entry.functions[0];
    crate::metadata::resolver::resolve_function_tokens(main, &entry, &ctx);
    assert_eq!(fn_ref_tokens(main), vec![g.id.get().unwrap().0, 1, UNRESOLVED]);

    let late = register_lazy(&ctx, const_fn("Late.F$0", 9));
    let v = returned(super::super::exec_function(&ctx, &entry, main, &[]).unwrap());
    assert_eq!(v, Value::I64(9));
    assert_eq!(fn_ref_tokens(main)[2], late.id.get().unwrap().0, "first execution stores the FnId");
}

/// Against a module that is not the VM's entry module (bare test context), a module
/// function binds to its index; a lazily loaded one has no id there and throws.
#[test]
fn non_entry_module_binds_local_indices_only() {
    let ctx = VmContext::new();
    ctx.install_lazy_loader(None, 0);
    register_lazy(&ctx, const_fn("Lazy.G$0", 7));
    let m = module("M", vec![const_fn("M.A$0", 1), const_fn("M.B$0", 2)]);
    let mut frame = Frame::new(&ctx, &[], 4);
    assert!(load_fn_with(&ctx, &m, &mut frame, "M.B$0", None).is_none());
    assert_eq!(*frame.get(0).unwrap(), Value::FuncRef(FnId(1)));
    assert!(super::call_indirect(&ctx, &m, &mut frame, 1, 0, &[]).unwrap().is_none());
    assert_eq!(*frame.get(1).unwrap(), Value::I64(2));
    assert!(load_fn_with(&ctx, &m, &mut frame, "Lazy.G$0", None).is_some(), "no id outside the entry module");
}
