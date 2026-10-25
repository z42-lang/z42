//! Dynamic-site guard-inlining tests: hand-built IR with an EMPTY `reg_types`,
//! so every Add/Eq/… site is dynamically typed and goes through the runtime-tag
//! guards (`emit_arith_guarded_binop` / `emit_cmp_guarded`) or falls to the fat
//! helper. Each case asserts the JIT and the interpreter agree.

use crate::metadata::bytecode::{BasicBlock, Function, Instruction, Module, Terminator};
use crate::metadata::types::ExecMode;
use crate::metadata::Value;
use crate::vm_context::VmContext;

fn func(name: &str, instructions: Vec<Instruction>, ret: u32) -> Function {
    Function {
        name: name.to_string(),
        param_count: 0,
        ret_type: "object".to_string(),
        exec_mode: ExecMode::Interp,
        blocks: vec![BasicBlock {
            label: "entry".to_string(),
            instructions,
            terminator: Terminator::Ret { reg: Some(ret) },
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

fn module_of(strings: &[&str], functions: Vec<Function>) -> Module {
    let func_index = functions.iter().enumerate().map(|(i, f)| (f.name.clone(), i)).collect();
    Module {
        name: "M".into(),
        string_pool: strings.iter().map(|s| s.to_string()).collect(),
        classes: Vec::new(),
        functions,
        type_registry: rustc_hash::FxHashMap::default(),
        func_index,
    }
}

fn interp(vm: &VmContext, m: &Module, name: &str) -> Value {
    let idx = m.func_index[name];
    match crate::interp::exec_function(vm, m, &m.functions[idx], &[]).expect("runs") {
        crate::interp::ExecOutcome::Returned(v) => v.unwrap_or(Value::Null),
        crate::interp::ExecOutcome::Thrown(exc) => panic!("threw {exc:?}"),
    }
}

#[cfg(feature = "jit")]
fn jit(vm: &VmContext, m: &Module, name: &str) -> Value {
    use crate::jit::invoke::{call_entry, NativeOutcome};
    let rt = crate::jit::runtime::install(vm, m).expect("install");
    for i in 0..m.functions.len() { rt.resolve_fn_by_id(vm, i); }
    let entry = rt.resolve_fn_by_id(vm, m.func_index[name]).expect("compiles");
    match unsafe { call_entry(vm, entry, crate::jit::frame::JitFrame::new(vm, entry.max_reg, &[])) } {
        NativeOutcome::Returned(v) => v.unwrap_or(Value::Null),
        NativeOutcome::Threw => panic!("threw {:?}", vm.take_exception()),
    }
}

/// Assert both engines produce `expected` (Str compared by content).
#[cfg(feature = "jit")]
fn both_engines(m: &Module, name: &str, expected: &Value) {
    let vm_interp = VmContext::new();
    let got_i = interp(&vm_interp, m, name);
    let vm_jit = VmContext::new();
    let got_j = jit(&vm_jit, m, name);
    let same = match (expected, &got_i, &got_j) {
        (Value::Str(w), Value::Str(i), Value::Str(j)) =>
            w.as_str() == i.as_str() && w.as_str() == j.as_str(),
        (w, i, j) => w == i && w == j,
    };
    assert!(same, "{name}: interp={got_i:?} jit={got_j:?} expected={expected:?}");
}

macro_rules! k { ($v:expr) => { Value::I64($v) } }
macro_rules! b { ($v:expr) => { Value::Bool($v) } }
macro_rules! s { ($v:expr) => { Value::Str(crate::metadata::vstr::Str::from($v)) } }

use Instruction::{Add as IAdd, ConstF64, ConstI64, ConstNull, ConstStr, Eq as IEq, Ge as IGe, Lt as ILt, Mul as IMul, Ne as INe, Sub as ISub};

#[test]
fn guarded_eq_ne_on_i64_and_str_and_null() {
    let m = module_of(&["ab", "ac", "x"], vec![
        func("eq_ii_t", vec![ConstI64 { dst: 1, val: 5 }, ConstI64 { dst: 2, val: 5 }, IEq { dst: 0, a: 1, b: 2 }], 0),
        func("eq_ii_f", vec![ConstI64 { dst: 1, val: 5 }, ConstI64 { dst: 2, val: 6 }, IEq { dst: 0, a: 1, b: 2 }], 0),
        func("ne_ii",   vec![ConstI64 { dst: 1, val: 5 }, ConstI64 { dst: 2, val: 6 }, INe { dst: 0, a: 1, b: 2 }], 0),
        func("eq_ss_t", vec![ConstStr { dst: 1, idx: 0 }, ConstStr { dst: 2, idx: 0 }, IEq { dst: 0, a: 1, b: 2 }], 0),
        func("eq_ss_f", vec![ConstStr { dst: 1, idx: 0 }, ConstStr { dst: 2, idx: 1 }, IEq { dst: 0, a: 1, b: 2 }], 0),
        func("eq_sn_f", vec![ConstStr { dst: 1, idx: 0 }, ConstNull { dst: 2 }, IEq { dst: 0, a: 1, b: 2 }], 0),
        func("ne_sn_t", vec![ConstStr { dst: 1, idx: 0 }, ConstNull { dst: 2 }, INe { dst: 0, a: 1, b: 2 }], 0),
        func("eq_ns_f", vec![ConstNull { dst: 1 }, ConstStr { dst: 2, idx: 0 }, IEq { dst: 0, a: 1, b: 2 }], 0),
        func("eq_nn_t", vec![ConstNull { dst: 1 }, ConstNull { dst: 2 }, IEq { dst: 0, a: 1, b: 2 }], 0),
        func("eq_fx_t", vec![ConstF64 { dst: 1, val: 2.0 }, ConstI64 { dst: 2, val: 2 }, IEq { dst: 0, a: 1, b: 2 }], 0),
        func("eq_xf_t", vec![ConstI64 { dst: 1, val: 2 }, ConstF64 { dst: 2, val: 2.0 }, IEq { dst: 0, a: 1, b: 2 }], 0),
    ]);
    #[cfg(feature = "jit")]
    {
        both_engines(&m, "eq_ii_t", &b!(true));
        both_engines(&m, "eq_ii_f", &b!(false));
        both_engines(&m, "ne_ii", &b!(true));
        both_engines(&m, "eq_ss_t", &b!(true));
        both_engines(&m, "eq_ss_f", &b!(false));
        both_engines(&m, "eq_sn_f", &b!(false));
        both_engines(&m, "ne_sn_t", &b!(true));
        both_engines(&m, "eq_ns_f", &b!(false));
        both_engines(&m, "eq_nn_t", &b!(true));
        both_engines(&m, "eq_fx_t", &b!(true));
        both_engines(&m, "eq_xf_t", &b!(true));
    }
    // Interpreter-only sanity (mirrors the jit asserts).
    let vm = VmContext::new();
    assert_eq!(interp(&vm, &m, "eq_ss_f"), Value::Bool(false));
    assert_eq!(interp(&vm, &m, "eq_fx_t"), Value::Bool(true));
}

#[test]
fn guarded_ordering_on_i64() {
    let m = module_of(&["a"], vec![
        func("lt_t",  vec![ConstI64 { dst: 1, val: -1 }, ConstI64 { dst: 2, val: 1 }, ILt { dst: 0, a: 1, b: 2 }], 0),
        func("lt_f",  vec![ConstI64 { dst: 1, val: 1 }, ConstI64 { dst: 2, val: -1 }, ILt { dst: 0, a: 1, b: 2 }], 0),
        func("ge_t",  vec![ConstI64 { dst: 1, val: 3 }, ConstI64 { dst: 2, val: 3 }, IGe { dst: 0, a: 1, b: 2 }], 0),
        func("lt_ss", vec![ConstStr { dst: 1, idx: 0 }, ConstI64 { dst: 2, val: 1 }, ILt { dst: 0, a: 1, b: 2 }], 0),
    ]);
    #[cfg(feature = "jit")]
    {
        both_engines(&m, "lt_t", &b!(true));
        both_engines(&m, "lt_f", &b!(false));
        both_engines(&m, "ge_t", &b!(true));
    }
    // Str < I64 falls to the helper and fails with the type mismatch in
    // either form the engines surface it (Thrown / Err).
    let vm = VmContext::new();
    let idx = m.func_index["lt_ss"];
    match crate::interp::exec_function(&vm, &m, &m.functions[idx], &[]) {
        Ok(crate::interp::ExecOutcome::Thrown(_)) | Err(_) => {}
        other => panic!("expected throw, got {other:?}"),
    }
}

#[test]
fn guarded_add_sub_mul_on_i64_and_str() {
    let m = module_of(&["ab", "cd", "x"], vec![
        func("add_wrap", vec![ConstI64 { dst: 1, val: i64::MAX }, ConstI64 { dst: 2, val: 1 }, IAdd { dst: 0, a: 1, b: 2 }], 0),
        func("sub",     vec![ConstI64 { dst: 1, val: 7 }, ConstI64 { dst: 2, val: 3 }, ISub { dst: 0, a: 1, b: 2 }], 0),
        func("mul",     vec![ConstI64 { dst: 1, val: 6 }, ConstI64 { dst: 2, val: 7 }, IMul { dst: 0, a: 1, b: 2 }], 0),
        func("add_ss",  vec![ConstStr { dst: 1, idx: 0 }, ConstStr { dst: 2, idx: 1 }, IAdd { dst: 0, a: 1, b: 2 }], 0),
        func("add_si",  vec![ConstStr { dst: 1, idx: 2 }, ConstI64 { dst: 2, val: 5 }, IAdd { dst: 0, a: 1, b: 2 }], 0),
    ]);
    #[cfg(feature = "jit")]
    {
        both_engines(&m, "add_wrap", &k!(i64::MIN));
        both_engines(&m, "sub", &k!(4));
        both_engines(&m, "mul", &k!(42));
        both_engines(&m, "add_ss", &s!("abcd"));
        both_engines(&m, "add_si", &s!("x5"));
    }
    let vm = VmContext::new();
    assert_eq!(interp(&vm, &m, "add_ss"), s!("abcd"));
    assert_eq!(interp(&vm, &m, "add_si"), s!("x5"));
}
