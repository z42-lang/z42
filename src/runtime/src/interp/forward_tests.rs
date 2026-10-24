//! `[Native]` stub folding end to end on hand-built modules, both engines: a call of a
//! wrapper runs its builtin directly, returns the same value, and a failing builtin's
//! exception carries the same stack trace as the unfolded call (the wrapper's row
//! synthesized for a frameless builtin, a light frame for an old-table one). Internal
//! builtin errors are fatal in both engines.
//!
//! The builtins are the test-only ones of `corelib::abi::test_builtins`.

use std::sync::Arc;

use crate::metadata::bytecode::{BasicBlock, BuiltinInsn, CallInsn, Function, Instruction, Module, Terminator};
use crate::metadata::tokens::TypeId;
use crate::metadata::types::{ExecMode, FieldSlot, TypeDesc};
use crate::metadata::Value;
use crate::vm_context::VmContext;
use crate::interp::ExecOutcome;

fn func(name: &str, params: usize, instructions: Vec<Instruction>, terminator: Terminator) -> Function {
    Function {
        name: name.to_string(),
        param_count: params,
        ret_type: "object".to_string(),
        exec_mode: ExecMode::Interp,
        blocks: vec![BasicBlock { label: "entry".to_string(), instructions, terminator }],
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

/// `%n = builtin <name>(%0..%n-1); ret %n` (or `ret` void).
fn wrapper(name: &str, builtin: &str, params: usize, void: bool) -> Function {
    let dst = params as u32;
    func(name, params, vec![Instruction::Builtin(Box::new(BuiltinInsn {
        dst, name: builtin.to_string(), args: (0..params as u32).collect(),
    }))], Terminator::Ret { reg: (!void).then_some(dst) })
}

/// `%n = call <target>(%0..%n-1); ret %n` — a chained forwarder.
fn chain(name: &str, target: &str, params: usize) -> Function {
    let dst = params as u32;
    func(name, params, vec![Instruction::Call(Box::new(CallInsn {
        dst, func: target.to_string(), args: (0..params as u32).collect(), method_type_args: Box::default(),
    }))], Terminator::Ret { reg: Some(dst) })
}

/// `%i = const args[i] …; %n = call <callee>(%0..); ret %n`.
fn caller(name: &str, callee: &str, args: &[i64]) -> Function {
    let n = args.len() as u32;
    let mut ins: Vec<Instruction> = args.iter().enumerate()
        .map(|(i, &v)| Instruction::ConstI64 { dst: i as u32, val: v }).collect();
    ins.push(Instruction::Call(Box::new(CallInsn {
        dst: n, func: callee.to_string(), args: (0..n).collect(), method_type_args: Box::default(),
    })));
    func(name, 0, ins, Terminator::Ret { reg: Some(n) })
}

fn exception_type(name: &str, base: Option<&str>) -> Arc<TypeDesc> {
    let fields = vec![
        FieldSlot { name: "Message".into(),        type_tag: "str".into(), visibility: 0 },
        FieldSlot { name: "StackTrace".into(),     type_tag: "str".into(), visibility: 0 },
        FieldSlot { name: "InnerException".into(), type_tag: "Std.Exception".into(), visibility: 0 },
    ];
    let mut field_index = crate::metadata::NameIndex::new();
    for (i, f) in fields.iter().enumerate() { field_index.insert(f.name.to_string(), i); }
    let own: Box<[FieldSlot]> = fields.clone().into();
    Arc::new(TypeDesc {
        class_flags: 0, visibility: 0, name: name.into(), id: TypeId::UNRESOLVED,
        base_name: base.map(str::to_owned), fields, field_index,
        vtable: vec![], vtable_index: crate::metadata::NameIndex::new(),
        cold: Some(Box::new(crate::metadata::types::TypeDescCold { own_fields: own, ..Default::default() })),
    })
}

/// A module of `functions` that knows the exception classes the tests throw.
pub(crate) fn module_of(functions: Vec<Function>) -> Module {
    let func_index = functions.iter().enumerate().map(|(i, f)| (f.name.clone(), i)).collect();
    let mut type_registry = rustc_hash::FxHashMap::default();
    for (n, b) in [
        ("Std.Exception", None),
        ("Std.FormatException", Some("Std.Exception")),
        ("Std.ArgumentException", Some("Std.Exception")),
        ("Std.ArgumentNullException", Some("Std.ArgumentException")),
    ] {
        type_registry.insert(n.to_string(), exception_type(n, b));
    }
    Module { name: "M".into(), string_pool: Vec::new(), classes: Vec::new(), functions, type_registry, func_index }
}

fn idx(m: &Module, name: &str) -> usize { m.func_index[name] }

/// Run `name` on the interpreter.
fn interp(vm: &VmContext, m: &Module, name: &str) -> anyhow::Result<ExecOutcome> {
    crate::interp::exec_function(vm, m, &m.functions[idx(m, name)], &[])
}

/// What a run produced: `Ok(value)` or `Err(trace / error text)`.
fn outcome(vm: &VmContext, m: &Module, r: anyhow::Result<ExecOutcome>) -> Result<Value, String> {
    match r {
        Ok(ExecOutcome::Returned(v)) => Ok(v.unwrap_or(Value::Null)),
        Ok(ExecOutcome::Thrown(exc)) => Err(thrown_text(vm, m, &exc)),
        Err(e) => Err(format!("error: {e}")),
    }
}

fn thrown_text(vm: &VmContext, m: &Module, exc: &Value) -> String {
    match exc {
        Value::Object(o) => format!(
            "{}: {}\n{}",
            o.type_desc().name,
            crate::exception::read_message(exc, vm, m).unwrap_or_default(),
            crate::exception::read_stack_trace(exc, vm, m).unwrap_or_default()),
        other => format!("raw: {}", crate::corelib::convert::value_to_str(other)),
    }
}

#[cfg(feature = "jit")]
fn jit(vm: &VmContext, m: &Module, name: &str) -> Result<Value, String> {
    use crate::jit::invoke::{call_entry, NativeOutcome};
    let rt = crate::jit::runtime::install(vm, m).expect("install");
    // Compile every function eagerly: the wrappers then fold through their compiled
    // entries (`FnEntry::forward`), the caller through `jit_call`.
    for i in 0..m.functions.len() { rt.resolve_fn_by_id(vm, i); }
    let entry = rt.resolve_fn_by_id(vm, idx(m, name)).expect("compiles");
    match unsafe { call_entry(vm, entry, crate::jit::frame::JitFrame::new(vm, entry.max_reg, &[])) } {
        NativeOutcome::Returned(v) => Ok(v.unwrap_or(Value::Null)),
        NativeOutcome::Threw => {
            let exc = vm.take_exception().expect("pending exception");
            Err(thrown_text(vm, m, &exc))
        }
    }
}

fn add_module() -> Module {
    module_of(vec![wrapper("T.C.Add", "__test_abi_add", 2, false), caller("T.Main", "T.C.Add", &[2, 3])])
}

#[test]
fn a_wrapper_call_folds_to_its_builtin() {
    let m = add_module();
    let vm = VmContext::new();
    assert_eq!(outcome(&vm, &m, interp(&vm, &m, "T.Main")).map(|v| format!("{v:?}")), Ok("I64(5)".into()));
    let id = crate::corelib::builtin_id_of("__test_abi_add").unwrap().0;
    assert_eq!(m.functions[idx(&m, "T.C.Add")].forward.get(), Some(Some(id)), "the call folded");
}

#[cfg(feature = "jit")]
#[test]
fn a_wrapper_call_folds_under_the_jit_too() {
    let m = add_module();
    let vm = VmContext::new();
    assert_eq!(jit(&vm, &m, "T.Main").map(|v| format!("{v:?}")), Ok("I64(5)".into()));
}

fn fail_module() -> Module {
    module_of(vec![
        wrapper("T.C.Fail", "__test_abi_fail", 1, true),
        caller("T.Main", "T.C.Fail", &[0]),
    ])
}

#[test]
fn a_folded_failure_has_the_unfolded_trace() {
    let m = fail_module();
    let vm = VmContext::new();
    let folded = outcome(&vm, &m, interp(&vm, &m, "T.Main")).unwrap_err();
    assert!(folded.starts_with("Std.FormatException: kind 0\n"), "{folded}");
    let lines: Vec<&str> = folded.lines().skip(1).collect();
    assert!(lines[0].starts_with("  at T.C.Fail"), "the wrapper's row, innermost: {folded}");
    assert!(lines[1].starts_with("  at T.Main"), "{folded}");

    // The same call unfolded (the wrapper's activation runs): identical text.
    let m2 = fail_module();
    m2.functions[idx(&m2, "T.C.Fail")].forward.set(None);
    let vm2 = VmContext::new();
    assert_eq!(outcome(&vm2, &m2, interp(&vm2, &m2, "T.Main")).unwrap_err(), folded);
}

#[cfg(feature = "jit")]
#[test]
fn a_folded_failure_has_the_same_trace_under_the_jit() {
    let m = fail_module();
    let vm = VmContext::new();
    let expected = outcome(&vm, &m, interp(&vm, &m, "T.Main")).unwrap_err();
    let m2 = fail_module();
    let vm2 = VmContext::new();
    assert_eq!(jit(&vm2, &m2, "T.Main").unwrap_err(), expected);
}

fn chain_module() -> Module {
    module_of(vec![
        wrapper("T.C.Inner", "__test_abi_fail", 1, true),
        chain("T.C.Outer", "T.C.Inner", 1),
        caller("T.Main", "T.C.Outer", &[0]),
    ])
}

#[test]
fn a_chained_forward_folds_and_keeps_both_rows() {
    let m = chain_module();
    let vm = VmContext::new();
    let folded = outcome(&vm, &m, interp(&vm, &m, "T.Main")).unwrap_err();
    let id = crate::corelib::builtin_id_of("__test_abi_fail").unwrap().0;
    assert_eq!(m.functions[idx(&m, "T.C.Outer")].forward.get(), Some(Some(id)));
    let rows: Vec<&str> = folded.lines().skip(1).map(str::trim).collect();
    assert!(rows[0].starts_with("at T.C.Inner") && rows[1].starts_with("at T.C.Outer")
        && rows[2].starts_with("at T.Main"), "{folded}");

    let m2 = chain_module();
    for f in &m2.functions { f.forward.set(None); }
    let vm2 = VmContext::new();
    assert_eq!(outcome(&vm2, &m2, interp(&vm2, &m2, "T.Main")).unwrap_err(), folded);
    #[cfg(feature = "jit")]
    {
        let m3 = chain_module();
        let vm3 = VmContext::new();
        assert_eq!(jit(&vm3, &m3, "T.Main").unwrap_err(), folded);
    }
}

/// A builtin that may run z42 code (`reenters`) folds under a light frame for the
/// wrapper: its error's trace shows the wrapper exactly as the unfolded call; one that
/// cannot run z42 code folds frameless.
#[test]
fn a_reentrant_old_table_builtin_folds_under_a_light_frame() {
    // `__load_module` given an integer: a typed builtin's argument error (P1-8 批次 6
    // 迁入 `#[builtin(reenters)]`;此前是旧表 + `LEGACY_REENTERS` 的样板,文案不同)。
    let build = || module_of(vec![
        wrapper("T.C.Load", "__load_module", 1, false),
        caller("T.Main", "T.C.Load", &[7]),
    ]);
    let id = crate::corelib::builtin_id_of("__load_module").unwrap().0;
    assert!(!crate::corelib::folds_frameless(id), "reflection / module loading may run z42 code");
    assert!(crate::corelib::folds_frameless(crate::corelib::builtin_id_of("__math_sqrt").unwrap().0));
    let m = build();
    let vm = VmContext::new();
    let folded = outcome(&vm, &m, interp(&vm, &m, "T.Main")).unwrap_err();
    assert!(folded.starts_with("Std.Exception: __load_module: argument `path` expects a string"), "{folded}");
    assert!(folded.lines().nth(1).is_some_and(|l| l.trim_start().starts_with("at T.C.Load")), "{folded}");
    assert_ne!(m.functions[idx(&m, "T.C.Load")].forward.get(), Some(Some(id)), "a framed fold token");
    assert_eq!(vm.call_stack.depth(), 0, "the light frame is popped");

    let m2 = build();
    m2.functions[idx(&m2, "T.C.Load")].forward.set(None);
    let vm2 = VmContext::new();
    assert_eq!(outcome(&vm2, &m2, interp(&vm2, &m2, "T.Main")).unwrap_err(), folded);
    #[cfg(feature = "jit")]
    {
        let m3 = build();
        let vm3 = VmContext::new();
        assert_eq!(jit(&vm3, &m3, "T.Main").unwrap_err(), folded);
    }
}

fn internal_module() -> Module {
    module_of(vec![wrapper("T.C.Bad", "__test_abi_internal", 0, false), caller("T.Main", "T.C.Bad", &[])])
}

#[test]
fn an_internal_error_is_fatal_in_the_interpreter() {
    let _g = crate::stack_guard::FATAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let m = internal_module();
    let vm = VmContext::new();
    let err = interp(&vm, &m, "T.Main").expect_err("not a z42 exception");
    assert!(err.to_string().contains("fatal error: internal error in builtin `__test_abi_internal`"), "{err}");
    assert!(crate::stack_guard::is_fatal(&vm));
    let report = crate::stack_guard::take_report().expect("a fatal report");
    assert!(report.contains("broken invariant (test)") && report.contains("T.Main"), "{report}");
}

#[cfg(feature = "jit")]
#[test]
fn an_internal_error_is_fatal_under_the_jit() {
    let _g = crate::stack_guard::FATAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let m = internal_module();
    let vm = VmContext::new();
    let err = jit(&vm, &m, "T.Main").expect_err("unwinds");
    assert!(err.contains("internal error in builtin `__test_abi_internal`"), "{err}");
    assert!(crate::stack_guard::is_fatal(&vm), "the JIT path sets the fatal flag too");
    let report = crate::stack_guard::take_report().expect("a fatal report");
    assert!(report.contains("broken invariant (test)"), "{report}");
}

/// While a fatal error unwinds, a failing builtin does not become a z42 exception
/// (both engines share `abi::error::raise`; the JIT used to skip this check).
#[test]
fn no_exception_is_built_while_a_fatal_error_unwinds() {
    let _g = crate::stack_guard::FATAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let m = fail_module();
    let vm = VmContext::new();
    let _ = crate::stack_guard::fatal_error(&vm, "test: already unwinding");
    let _ = crate::stack_guard::take_report();
    match crate::corelib::abi::error::raise(&vm, &m, "__test_abi_fail",
        crate::corelib::abi::BuiltinError::Format("x".into()), &[]) {
        crate::corelib::abi::error::Raised::Fatal(_) => {}
        _ => panic!("expected the raw fatal error"),
    }
}
