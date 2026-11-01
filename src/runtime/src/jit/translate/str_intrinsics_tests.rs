//! Inline string `Length` / `CharAt` (perf-str-jit-intrinsics).
//!
//! Each case compiles a tiny caller with the real translator, runs it natively and
//! on the interpreter, and checks (a) the two agree — value or exception text — and
//! (b) which path the JIT took: the slow path is the site's helper, which dispatches
//! the `Std.String` wrapper and so bumps that function's tier-up counter; the inline
//! path never touches it. The golden `src/tests/strings/str_intrinsics_jit` covers
//! the same members end to end through the real compiler and stdlib.

use crate::jit::forward::{builtin_forward, chained_forward, ForwardLookup, StrIntrinsic};
use crate::jit::frame::JitFrame;
use crate::jit::invoke::{call_entry, NativeOutcome};
use crate::jit::runtime::install;
use crate::metadata::ir::{
    BasicBlock, BuiltinInsn, CallInsn, Function, Instruction, Module, Terminator, VCallInsn,
};
use crate::metadata::types::ExecMode;
use crate::metadata::vstr::{jit_layout, Str};
use crate::metadata::{IrType, Value};
use crate::vm_context::VmContext;

const GET_LENGTH: usize = 0;
const CHAR_AT: usize = 1;
const GET_ITEM: usize = 2;
const LEN_VCALL: usize = 3;
const AT_CALL: usize = 4;
const AT_VCALL: usize = 5;

fn func(name: &str, params: usize, regs: &[IrType], instr: Instruction, ret: u32) -> Function {
    Function {
        name: name.to_string(),
        param_count: params,
        ret_type: "i64".to_string(),
        exec_mode: ExecMode::Jit,
        blocks: vec![BasicBlock {
            label: "entry".to_string(),
            instructions: vec![instr],
            terminator: Terminator::Ret { reg: Some(ret) },
        }],
        is_static: false,
        visibility: 0,
        method_flags: 0, min_arg: 0, params_from: 0xFF,
        max_reg: regs.len().saturating_sub(1) as u32,
        cold: None,
        reg_types: regs.into(),
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

fn builtin(dst: u32, name: &str, args: &[u32]) -> Instruction {
    Instruction::Builtin(Box::new(BuiltinInsn { dst, name: name.to_string(), args: args.into() }))
}
fn call(dst: u32, f: &str, args: &[u32]) -> Instruction {
    Instruction::Call(Box::new(CallInsn {
        dst, func: f.to_string(), args: args.into(), method_type_args: Box::new([]),
    }))
}
fn vcall(dst: u32, obj: u32, m: &str, args: &[u32]) -> Instruction {
    Instruction::VCall(Box::new(VCallInsn {
        dst, obj, method: m.to_string(), args: args.into(), method_type_args: Box::new([]),
    }))
}

/// The `Std.String` wrappers as z42c emits them, plus three callers:
/// `s.Length` (vcall), `s[i]` (static call of the chained `get_Item`) and
/// `s.CharAt(i)` reached through a vcall.
fn module() -> Module {
    use IrType::*;
    let functions = vec![
        func("Std.String.get_Length", 1, &[Str, I64], builtin(1, "__str_length", &[0]), 1),
        func("Std.String.CharAt", 2, &[Str, I64, Char], builtin(2, "__str_char_at", &[0, 1]), 2),
        func("Std.String.get_Item", 2, &[Str, I64, Char], call(2, "Std.String.CharAt", &[0, 1]), 2),
        func("T.Len", 1, &[Str, I64], vcall(1, 0, "get_Length", &[]), 1),
        func("T.At", 2, &[Str, I64, Char], call(2, "Std.String.get_Item", &[0, 1]), 2),
        func("T.AtV", 2, &[Str, I64, Char], vcall(2, 0, "CharAt", &[1]), 2),
    ];
    let func_index = functions.iter().enumerate().map(|(i, f)| (f.name.clone(), i)).collect();
    Module {
        name: "T".to_string(),
        string_pool: Vec::new(),
        classes: Vec::new(),
        functions,
        type_registry: rustc_hash::FxHashMap::default(),
        func_index,
    }
}

/// Translate-time lookup over the test module. (The production one also requires
/// the owner type to be loaded without a static constructor; this module
/// registers no types.)
struct Callees<'m>(&'m Module);

impl Callees<'_> {
    fn of(&self, f: &Function) -> Option<u32> {
        builtin_forward(f).or_else(|| chained_forward(f, |n| {
            self.0.func_index.get(n).and_then(|&i| builtin_forward(&self.0.functions[i]))
        }))
    }
}

impl ForwardLookup for Callees<'_> {
    fn forward_of_id(&self, id: u32) -> Option<u32> { self.of(self.0.functions.get(id as usize)?) }
    fn forward_of_name(&self, name: &str) -> Option<u32> {
        self.of(&self.0.functions[*self.0.func_index.get(name)?])
    }
}

/// Value or exception, as text — the parity currency between the two engines.
/// This module has no `Std.Exception`, so a builtin error surfaces as its message
/// (a raw string / an interp `Err`); only the message line is compared, not the
/// trace rows the interpreter appends.
fn show(v: Value) -> String {
    match v {
        Value::Str(s) => thrown(s.as_str()),
        other => format!("{other:?}"),
    }
}
fn thrown(msg: &str) -> String {
    format!("throw {}", msg.lines().next().unwrap_or(""))
}

/// Run caller `idx` on the JIT (`init_pending`: a package initializer is pending,
/// which closes the inline gate). Returns the outcome and whether the slow path ran.
fn run_jit(m: &Module, idx: usize, args: &[Value], init_pending: bool) -> (String, bool) {
    let vm = VmContext::new();
    crate::metadata::resolver::resolve_function_tokens(&m.functions[idx], m, &vm);
    let rt = install(&vm, m).expect("install");
    if init_pending {
        use crate::vm_context::cctor::CctorRegistry;
        // The counter the inline gate reads (and `jit_call`'s barrier recounts).
        let reg = std::sync::Arc::as_ptr(&vm.core.cctors) as *const u8;
        let pending = unsafe { &*(reg.add(CctorRegistry::MODULE_PENDING_OFFSET) as *const std::sync::atomic::AtomicUsize) };
        pending.store(1, std::sync::atomic::Ordering::Relaxed);
    }
    let entry = rt.compile_with(&m.functions[idx], &Callees(m)).expect("compile");
    let out = match unsafe { call_entry(&vm, &entry, JitFrame::new(&vm, entry.max_reg, args)) } {
        NativeOutcome::Returned(v) => show(v.unwrap_or(Value::Null)),
        NativeOutcome::Threw => show(vm.take_exception().unwrap_or(Value::Null)),
    };
    let slow = [GET_LENGTH, CHAR_AT, GET_ITEM].iter().any(|&w| rt.slot_probe(w).0 > 0);
    (out, slow)
}

fn run_interp(m: &Module, idx: usize, args: &[Value]) -> String {
    let vm = VmContext::new();
    match crate::interp::exec_function(&vm, m, &m.functions[idx], args) {
        Ok(crate::interp::ExecOutcome::Returned(v)) => show(v.unwrap_or(Value::Null)),
        Ok(crate::interp::ExecOutcome::Thrown(v)) => show(v),
        Err(e) => thrown(&e.to_string()),
    }
}

/// Assert JIT == interp for `args`, and that the JIT took the inline path iff `inline`.
fn check(idx: usize, args: &[Value], inline: bool, expect: &str) {
    let m = module();
    let interp = run_interp(&m, idx, args);
    let (jit, slow) = run_jit(&m, idx, args, false);
    assert_eq!(jit, interp, "{}({args:?}): jit vs interp", m.functions[idx].name);
    assert!(jit.contains(expect), "{}({args:?}) = {jit}, expected {expect}", m.functions[idx].name);
    assert_eq!(!slow, inline, "{}({args:?}): inline path taken?", m.functions[idx].name);
}

fn s(text: &str) -> Value { Value::Str(Str::new_leaked(text)) }

#[test]
fn length_inline_on_ascii_including_empty() {
    check(LEN_VCALL, &[s("hello")], true, "I64(5)");
    check(LEN_VCALL, &[s("")], true, "I64(0)");
}

#[test]
fn length_of_non_ascii_takes_the_helper() {
    check(LEN_VCALL, &[s("héllo你")], false, "I64(6)");
}

#[test]
fn length_of_null_throws_identically() {
    let m = module();
    let interp = run_interp(&m, LEN_VCALL, &[Value::Null]);
    assert_eq!(run_jit(&m, LEN_VCALL, &[Value::Null], false).0, interp);
}

#[test]
fn char_at_inline_on_ascii_both_call_shapes() {
    for f in [AT_CALL, AT_VCALL] {
        check(f, &[s("hello"), Value::I64(0)], true, "Char('h')");
        check(f, &[s("hello"), Value::I64(4)], true, "Char('o')");
    }
}

#[test]
fn char_at_non_ascii_takes_the_helper() {
    for f in [AT_CALL, AT_VCALL] {
        check(f, &[s("héllo"), Value::I64(1)], false, "Char('é')");
        check(f, &[s("héllo"), Value::I64(4)], false, "Char('o')");
    }
}

#[test]
fn char_at_out_of_range_throws_the_same_message() {
    for f in [AT_CALL, AT_VCALL] {
        check(f, &[s("hello"), Value::I64(5)], false, "index 5 is out of range for a string of length 5");
        check(f, &[s(""), Value::I64(0)], false, "for a string of length 0");
        check(f, &[s("héllo"), Value::I64(9)], false, "for a string of length 5");
        // Negative: the unsigned bounds check routes it to the helper, same exception.
        check(f, &[s("hello"), Value::I64(-1)], false, "index -1 is out of range");
    }
}

#[test]
fn static_call_with_a_pending_package_init_keeps_the_barrier_path() {
    let m = module();
    let (out, slow) = run_jit(&m, AT_CALL, &[s("hello"), Value::I64(1)], true);
    assert!(out.contains("Char('e')") && slow, "a pending package init closes the gate: {out}");
    // The vcall form runs no barrier, so it ignores the gate.
    let (out, slow) = run_jit(&m, LEN_VCALL, &[s("hello")], true);
    assert!(out.contains("I64(5)") && !slow, "{out}");
}

#[test]
fn recognition_is_by_forwarded_builtin() {
    let id = |n| crate::corelib::builtin_id_of(n).expect("builtin").0;
    assert_eq!(StrIntrinsic::of_builtin(id("__str_length")), Some(StrIntrinsic::Length));
    assert_eq!(StrIntrinsic::of_builtin(id("__str_char_at")), Some(StrIntrinsic::CharAt));
    assert_eq!(StrIntrinsic::of_builtin(id("__str_byte_length")), None);
}

/// The native view of a handle (`vstr::jit_layout`) reads what the Rust accessors do.
#[test]
fn jit_layout_matches_the_str_accessors() {
    for text in ["", "abc", "héllo", "x".repeat(300).as_str()] {
        let st = Str::new_leaked(text);
        // A `Value::Str` register payload is exactly the handle's 8 bytes.
        let bits: u64 = unsafe { std::mem::transmute::<Str, u64>(st) };
        let hdr = (bits & jit_layout::HANDLE_ADDR_MASK) as *const u8;
        unsafe {
            let len = std::ptr::read_unaligned(hdr.add(jit_layout::LEN_OFFSET as usize) as *const u32);
            assert_eq!(len as usize, st.len(), "{text:?} len");
            let flags = *hdr.add(jit_layout::FLAGS_OFFSET as usize);
            assert_eq!(flags & jit_layout::ASCII_BIT != 0, st.is_known_ascii(), "{text:?} ascii bit");
            let data = std::slice::from_raw_parts(hdr.add(jit_layout::DATA_OFFSET as usize), st.len());
            assert_eq!(data, text.as_bytes(), "{text:?} bytes");
        }
    }
}

/// `char` comparisons (`s[i] == '7'`, `c >= '0'`) are native for two `Char`
/// operands: code-point order, same as the interpreter, for all six operators.
#[test]
fn char_comparisons_match_the_interpreter() {
    use IrType::*;
    let ops: [(&str, fn(u32, u32, u32) -> Instruction); 6] = [
        ("eq", |dst, a, b| Instruction::Eq { dst, a, b }),
        ("ne", |dst, a, b| Instruction::Ne { dst, a, b }),
        ("lt", |dst, a, b| Instruction::Lt { dst, a, b }),
        ("le", |dst, a, b| Instruction::Le { dst, a, b }),
        ("gt", |dst, a, b| Instruction::Gt { dst, a, b }),
        ("ge", |dst, a, b| Instruction::Ge { dst, a, b }),
    ];
    let functions: Vec<Function> = ops.iter()
        .map(|(n, mk)| func(&format!("T.{n}"), 2, &[Char, Char, Bool], mk(2, 0, 1), 2))
        .collect();
    let func_index = functions.iter().enumerate().map(|(i, f)| (f.name.clone(), i)).collect();
    let m = Module {
        name: "T".to_string(), string_pool: Vec::new(), classes: Vec::new(), functions,
        type_registry: rustc_hash::FxHashMap::default(), func_index,
    };
    let chars = ['0', '7', '9', 'a', 'z', 'é', '你', '\u{10FFFF}'];
    for idx in 0..ops.len() {
        for &x in &chars {
            for &y in &chars {
                let args = [Value::Char(x), Value::Char(y)];
                let (jit, _) = run_jit(&m, idx, &args, false);
                assert_eq!(jit, run_interp(&m, idx, &args), "{}({x:?}, {y:?})", ops[idx].0);
            }
        }
    }
}
