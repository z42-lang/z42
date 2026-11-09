//! Generic call sites pass the bound method arguments to both tiers.
//! Direct reads and forwarded method arguments both compile.

use crate::vm::exec::call::outcome::ExecOutcome;
use crate::vm::runtime::{Function, Module};
use crate::zpkg::ir::{BasicBlock, CallInsn, Instruction, Terminator, VCallInsn};
use crate::zpkg::ir::ExecMode;
use crate::object::Value;
use crate::vm::context::VmContext;
use super::translate::{jit_unsupported_reason, unsupported_reason};

fn func(name: &str, params: usize, is_static: bool, instructions: Vec<Instruction>, ret: Option<u32>) -> Function {
    Function::new(crate::zpkg::ir::Function {
        name: name.to_string(),
        param_count: params,
        ret_type: "object".to_string(),
        exec_mode: ExecMode::Interp,
        blocks: vec![BasicBlock {
            label: "entry".to_string(),
            instructions,
            terminator: Terminator::Ret { reg: ret },
        }],
        is_static,
        visibility: 0,
        method_flags: 0,
        min_arg: 0,
        params_from: 0xFF,
        max_reg: 8,
        cold: None,
        reg_types: Box::new([]),
        block_index: std::collections::HashMap::new(),
        branch_targets: Vec::new(),
        frame_meta: None,
    })
}

fn mta(names: &[&str]) -> Box<[String]> { names.iter().map(|s| s.to_string()).collect() }

fn call(dst: u32, callee: &str, args: &[u32], type_args: &[&str]) -> Instruction {
    Instruction::Call(Box::new(CallInsn {
        dst, func: callee.to_string(), args: args.into(), method_type_args: mta(type_args),
    }))
}

fn vcall(dst: u32, obj: u32, method: &str, type_args: &[&str]) -> Instruction {
    Instruction::VCall(Box::new(VCallInsn {
        dst, obj, method: method.to_string(), args: Box::default(), method_type_args: mta(type_args),
    }))
}

/// `T.Def<T>() { return default(T); }` — reads its type arg.
fn def_body() -> Function {
    func("T.Def", 0, true, vec![Instruction::MethodDefault { dst: 0, param_index: 0 }], Some(0))
}

/// `Std.Int32.Def<T>() { return default(T); }` — the same, as an instance method on `int`
/// (a primitive receiver resolves `Std.Int32.<m>` without any class metadata).
fn int_def_body() -> Function {
    func("Std.Int32.Def", 1, false, vec![Instruction::MethodDefault { dst: 1, param_index: 0 }], Some(1))
}

/// `T.Id<T>(T x) { return x; }` — generic in the signature, blind to its type arg: compiles.
fn id_body() -> Function { func("T.Id", 1, true, vec![], Some(0)) }

fn module_of(functions: Vec<Function>) -> Module {
    let func_index = functions.iter().enumerate().map(|(i, f)| (f.name.clone(), i)).collect();
    Module::new(crate::zpkg::image::ModuleData {
        name: "M".into(),
        string_pool: Vec::new(),
        classes: Vec::new(),
        func_index,
    }, functions, rustc_hash::FxHashMap::default())
}

fn test_module() -> Module {
    module_of(vec![
        def_body(),
        int_def_body(),
        id_body(),
        // %0 = call T.Def<Std.Boolean>(); ret %0
        func("T.CallDef", 0, true, vec![call(0, "T.Def", &[], &["Std.Boolean"])], Some(0)),
        // %0 = 7; %1 = call T.Id<Std.Int32>(%0); ret %1
        func("T.CallId", 0, true, vec![
            Instruction::ConstI64 { dst: 0, val: 7 },
            call(1, "T.Id", &[0], &["Std.Int32"]),
        ], Some(1)),
        // %0 = 5; %1 = vcall %0.Def<Std.Boolean>(); ret %1
        func("T.VCallDef", 0, true, vec![
            Instruction::ConstI64 { dst: 0, val: 5 },
            vcall(1, 0, "Def", &["Std.Boolean"]),
        ], Some(1)),
        func("T.Fwd", 0, true, vec![call(0, "T.Def", &[], &["$mta:0"])], Some(0)),
        func("T.CallFwd", 0, true, vec![call(0, "T.Fwd", &[], &["Std.Boolean"])], Some(0)),
        func("T.VFwd", 0, true, vec![
            Instruction::ConstI64 { dst: 0, val: 5 },
            vcall(1, 0, "Def", &["$mta:0"]),
        ], Some(1)),
        func("T.CallVFwd", 0, true, vec![call(0, "T.VFwd", &[], &["Std.Boolean"])], Some(0)),
        func("T.CallMissing", 0, true, vec![call(0, "T.Def", &[], &[])], Some(0)),
    ])
}

fn idx(m: &Module, name: &str) -> usize { m.func_index[name] }

fn interp(vm: &VmContext, m: &Module, name: &str) -> Value {
    match crate::interp::run::exec_function(vm, m, &m.functions[idx(m, name)], &[]).expect("runs") {
        ExecOutcome::Returned(v) => v.unwrap_or(Value::Null),
        ExecOutcome::Thrown(exc) => panic!("threw {exc:?}"),
    }
}

#[test]
fn type_arg_reads_and_forwarding_compile() {
    let m = test_module();
    assert_eq!(jit_unsupported_reason(&m.functions[idx(&m, "T.CallDef")]), None, "generic Call site");
    assert_eq!(jit_unsupported_reason(&m.functions[idx(&m, "T.VCallDef")]), None, "generic VCall site");
    assert_eq!(jit_unsupported_reason(&m.functions[idx(&m, "T.Id")]), None, "a generic body blind to its type args");
    assert_eq!(jit_unsupported_reason(&m.functions[idx(&m, "T.Def")]), None);
    assert_eq!(jit_unsupported_reason(&m.functions[idx(&m, "T.Fwd")]), None);
    assert_eq!(unsupported_reason(&vcall(0, 0, "M", &["$mta:1"])), None);
}

/// Without a JIT the interpreter threads the site's type args (the baseline both engines
/// must match).
#[test]
fn the_interpreter_threads_a_sites_type_args() {
    let m = test_module();
    let vm = VmContext::new();
    assert_eq!(interp(&vm, &m, "T.CallDef"), Value::Bool(false), "default(T) with T = bool");
    assert_eq!(interp(&vm, &m, "T.VCallDef"), Value::Bool(false));
    assert_eq!(interp(&vm, &m, "T.CallId"), Value::I64(7));
}

#[cfg(feature = "jit")]
fn jit(vm: &VmContext, m: &Module, name: &str) -> Value {
    use crate::jit::runtime::invoke::{call_entry, NativeOutcome};
    let rt = crate::jit::compile::state::install(vm, m, crate::config::runtime_config().jit_config()).expect("install");
    for i in 0..m.functions.len() { rt.resolve_fn_by_id(vm, i); }
    let entry = rt.resolve_fn_by_id(vm, idx(m, name)).expect("the caller compiles");
    match unsafe { call_entry(vm, entry, crate::jit::runtime::frame::JitFrame::new(vm, entry.max_reg, &[])) } {
        NativeOutcome::Returned(v) => v.unwrap_or(Value::Null),
        NativeOutcome::Threw => panic!("threw {:?}", vm.take_exception()),
    }
}

/// Both native call helpers supply the type arguments to the callee.
#[cfg(feature = "jit")]
#[test]
fn compiled_static_and_virtual_sites_pass_bound_type_args() {
    let m = test_module();
    let vm = VmContext::new();
    assert_eq!(jit(&vm, &m, "T.CallDef"), Value::Bool(false));
    assert!(crate::jit::compile::state::runtime(&vm).unwrap().resolve_fn_by_id(&vm, idx(&m, "T.Def")).is_some());

    let vm = VmContext::new();
    assert_eq!(jit(&vm, &m, "T.VCallDef"), Value::Bool(false));
    assert!(crate::jit::compile::state::runtime(&vm).unwrap().resolve_fn_by_id(&vm, idx(&m, "Std.Int32.Def")).is_some());
}

#[cfg(feature = "jit")]
#[test]
fn native_calls_bind_forwarded_type_args_and_preserve_missing_args() {
    let m = test_module();
    let vm = VmContext::new();
    assert_eq!(jit(&vm, &m, "T.CallFwd"), Value::Bool(false));
    assert!(crate::jit::compile::state::runtime(&vm).unwrap().resolve_fn_by_id(&vm, idx(&m, "T.Fwd")).is_some());

    let vm = VmContext::new();
    assert_eq!(jit(&vm, &m, "T.CallVFwd"), Value::Bool(false));
    assert!(crate::jit::compile::state::runtime(&vm).unwrap().resolve_fn_by_id(&vm, idx(&m, "T.VFwd")).is_some());

    let vm = VmContext::new();
    assert_eq!(jit(&vm, &m, "T.CallMissing"), Value::Null);
}

#[cfg(feature = "jit")]
#[test]
fn a_compiled_generic_site_calls_a_compiled_callee_natively() {
    let m = test_module();
    let vm = VmContext::new();
    assert_eq!(jit(&vm, &m, "T.CallId"), Value::I64(7));
    let rt = crate::jit::compile::state::runtime(&vm).expect("installed");
    assert!(rt.resolve_fn_by_id(&vm, idx(&m, "T.Id")).is_some(), "the type-arg-blind generic body compiled");
}

/// The interpreter's mixed-mode divert now covers a generic call: with the callee compiled,
/// an interpreted generic site runs it natively (`jit_native_from_interp` counts it).
#[cfg(feature = "jit")]
#[test]
fn the_interpreter_diverts_a_generic_call_to_a_compiled_callee() {
    use std::sync::atomic::Ordering;
    let m = test_module();
    let vm = VmContext::new();
    let rt = crate::jit::compile::state::install(&vm, &m, crate::config::runtime_config().jit_config()).expect("install");
    assert!(rt.resolve_fn_by_id(&vm, idx(&m, "T.Id")).is_some(), "callee compiled up front");
    let before = vm.counters().jit_native_from_interp.load(Ordering::Relaxed);
    assert_eq!(interp(&vm, &m, "T.CallId"), Value::I64(7));
    assert_eq!(vm.counters().jit_native_from_interp.load(Ordering::Relaxed), before + 1,
        "the generic call site diverted to the compiled callee");
    let before = vm.counters().jit_native_from_interp.load(Ordering::Relaxed);
    assert!(rt.resolve_fn_by_id(&vm, idx(&m, "T.Def")).is_some());
    assert!(rt.resolve_fn_by_id(&vm, idx(&m, "Std.Int32.Def")).is_some());
    assert_eq!(interp(&vm, &m, "T.CallDef"), Value::Bool(false));
    assert_eq!(interp(&vm, &m, "T.VCallDef"), Value::Bool(false));
    assert_eq!(vm.counters().jit_native_from_interp.load(Ordering::Relaxed), before + 2);
}

#[cfg(feature = "jit")]
#[test]
fn method_parameter_array_compiles_with_bound_and_missing_args() {
    use crate::zpkg::ir::ArrayNewInsn;
    use crate::object::ElemType;

    let allocation = Instruction::ArrayNew(Box::new(ArrayNewInsn {
        dst: 1,
        size: 0,
        elem_tag: 0,
        element_type: String::new(),
        stack_alloc: false,
        type_param_kind: 1,
        type_param_index: 0,
    }));
    let caller = |name, args: &[&str]| func(name, 0, true, vec![
        Instruction::ConstI64 { dst: 0, val: 1 },
        call(1, "T.Allocate", &[0], args),
    ], Some(1));
    let module = module_of(vec![
        func("T.Allocate", 1, true, vec![allocation], Some(1)),
        caller("T.IntArray", &["Std.Int32"]),
        caller("T.MissingArray", &[]),
    ]);
    for (name, default, element_type) in [
        ("T.IntArray", Value::I64(0), ElemType::intern("Std.Int32")),
        ("T.MissingArray", Value::Null, ElemType::empty()),
    ] {
        let vm = VmContext::new();
        for value in [interp(&vm, &module, name), jit(&vm, &module, name)] {
            let Value::Array(array) = value else { panic!("expected array, got {value:?}") };
            let array = array.borrow();
            assert_eq!(array.len(), 1);
            assert_eq!(array.get_boxed(0), default);
            assert_eq!(array.element_type(), element_type);
        }
        assert!(crate::jit::compile::state::runtime(&vm).unwrap().resolve_fn_by_id(&vm, idx(&module, "T.Allocate")).is_some());
    }
}
