//! Method-level generic call sites (jit-generic-call-sites): a `Call` / `VCall` carrying
//! `method_type_args` compiles — the site bakes its type args and passes them to
//! `jit_call_mta` / `jit_vcall_mta` — while a *body* that reads its type args
//! (`MethodDefault` here) and a site forwarding the caller's (`$mta:N`) stay on the
//! interpreter. Both engines must agree, a type-arg-reading callee must still see its
//! arguments through a compiled caller, and the interpreter must divert a generic call to
//! a compiled callee (the mixed-mode path these sites were shut out of).

use crate::interp::ExecOutcome;
use crate::vm::runtime::{Function, Module};
use crate::metadata::ir::{BasicBlock, CallInsn, Instruction, Terminator, VCallInsn};
use crate::metadata::ir::ExecMode;
use crate::object::Value;
use crate::vm_context::VmContext;
use super::translate::{jit_unsupported_reason, unsupported_reason};

fn func(name: &str, params: usize, is_static: bool, instructions: Vec<Instruction>, ret: Option<u32>) -> Function {
    Function::new(crate::metadata::ir::Function {
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

/// `T.Def<T>() { return default(T); }` — reads its type arg: stays interp.
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
    Module::new(crate::metadata::image::ModuleData {
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
        // A site forwarding its own (nonexistent) type arg: `$mta:0`.
        func("T.Fwd", 0, true, vec![call(0, "T.Def", &[], &["$mta:0"])], Some(0)),
    ])
}

fn idx(m: &Module, name: &str) -> usize { m.func_index[name] }

fn interp(vm: &VmContext, m: &Module, name: &str) -> Value {
    match crate::interp::exec_function(vm, m, &m.functions[idx(m, name)], &[]).expect("runs") {
        ExecOutcome::Returned(v) => v.unwrap_or(Value::Null),
        ExecOutcome::Thrown(exc) => panic!("threw {exc:?}"),
    }
}

#[test]
fn a_generic_call_site_compiles_but_a_type_arg_reader_and_a_forwarder_do_not() {
    let m = test_module();
    assert_eq!(jit_unsupported_reason(&m.functions[idx(&m, "T.CallDef")]), None, "generic Call site");
    assert_eq!(jit_unsupported_reason(&m.functions[idx(&m, "T.VCallDef")]), None, "generic VCall site");
    assert_eq!(jit_unsupported_reason(&m.functions[idx(&m, "T.Id")]), None, "a generic body blind to its type args");
    assert!(jit_unsupported_reason(&m.functions[idx(&m, "T.Def")]).unwrap().contains("MethodDefault"));
    assert!(jit_unsupported_reason(&m.functions[idx(&m, "T.Fwd")]).unwrap().contains("$mta:"));
    assert!(unsupported_reason(&vcall(0, 0, "M", &["$mta:1"])).unwrap().contains("forwarding"));
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
    use crate::jit::invoke::{call_entry, NativeOutcome};
    let rt = crate::jit::runtime::install(vm, m).expect("install");
    for i in 0..m.functions.len() { rt.resolve_fn_by_id(vm, i); }
    let entry = rt.resolve_fn_by_id(vm, idx(m, name)).expect("the caller compiles");
    match unsafe { call_entry(vm, entry, crate::jit::frame::JitFrame::new(vm, entry.max_reg, &[])) } {
        NativeOutcome::Returned(v) => v.unwrap_or(Value::Null),
        NativeOutcome::Threw => panic!("threw {:?}", vm.take_exception()),
    }
}

/// A compiled caller passes the site's type args to an interpreted (type-arg-reading)
/// callee — `default(T)` sees `T`, through `jit_call_mta` and `jit_vcall_mta` alike.
#[cfg(feature = "jit")]
#[test]
fn a_compiled_generic_site_still_delivers_the_type_args_to_an_interpreted_callee() {
    let m = test_module();
    let vm = VmContext::new();
    assert_eq!(jit(&vm, &m, "T.CallDef"), Value::Bool(false), "jit_call_mta → interp T.Def");
    let rt = vm.jit().expect("installed");
    assert!(rt.resolve_fn_by_id(&vm, idx(&m, "T.Def")).is_none(), "the type-arg reader stays interp");
    assert!(rt.resolve_fn_by_id(&vm, idx(&m, "T.Fwd")).is_none(), "a forwarding site stays interp");
    let vm2 = VmContext::new();
    assert_eq!(jit(&vm2, &m, "T.VCallDef"), Value::Bool(false), "jit_vcall_mta → interp Std.Int32.Def");
}

/// A compiled caller calls a compiled generic callee natively (no type args needed: the
/// callee cannot read them — that is why it compiled).
#[cfg(feature = "jit")]
#[test]
fn a_compiled_generic_site_calls_a_compiled_callee_natively() {
    let m = test_module();
    let vm = VmContext::new();
    assert_eq!(jit(&vm, &m, "T.CallId"), Value::I64(7));
    let rt = vm.jit().expect("installed");
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
    let rt = crate::jit::runtime::install(&vm, &m).expect("install");
    assert!(rt.resolve_fn_by_id(&vm, idx(&m, "T.Id")).is_some(), "callee compiled up front");
    let before = vm.counters().jit_native_from_interp.load(Ordering::Relaxed);
    assert_eq!(interp(&vm, &m, "T.CallId"), Value::I64(7));
    assert_eq!(vm.counters().jit_native_from_interp.load(Ordering::Relaxed), before + 1,
        "the generic call site diverted to the compiled callee");
    // The type-arg reader is never compiled, so its generic site keeps interpreting it —
    // with the type args.
    assert!(rt.resolve_fn_by_id(&vm, idx(&m, "T.Def")).is_none());
    assert_eq!(interp(&vm, &m, "T.CallDef"), Value::Bool(false));
    assert_eq!(interp(&vm, &m, "T.VCallDef"), Value::Bool(false));
}
