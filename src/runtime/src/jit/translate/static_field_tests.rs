//! Native `StaticGet` / `StaticSet`: each case compiles small functions through the production
//! path (`JitRuntime::resolve_fn_by_id`), runs them natively and on the interpreter, and checks
//! which accesses left the native path through the helper counter
//! (`helpers::object::STATIC_HELPER_CALLS`).

use crate::jit::frame::JitFrame;
use crate::jit::helpers::object::STATIC_HELPER_CALLS;
use crate::jit::invoke::{call_entry, NativeOutcome};
use crate::jit::runtime::install;
use crate::metadata::bytecode::{
    BasicBlock, FieldDesc, Function, Instruction, Module, StaticGetInsn, StaticSetInsn, Terminator,
};
use crate::metadata::tokens::TypeId;
use crate::metadata::types::{ExecMode, NativeData, ObjectLayout, TypeDesc, TypeDescCold};
use crate::metadata::{IrType, Value};
use crate::vm_context::VmContext;
use std::sync::Arc;

// ── building blocks ─────────────────────────────────────────────────────────

fn func(name: &str, params: usize, regs: &[IrType], blocks: Vec<BasicBlock>) -> Function {
    let block_index = blocks.iter().enumerate().map(|(i, b)| (b.label.clone(), i)).collect();
    Function {
        name: name.to_string(), param_count: params, ret_type: "object".to_string(),
        exec_mode: ExecMode::Jit, blocks, is_static: true, visibility: 0,
        method_flags: 0, min_arg: 0, params_from: 0xFF,
        max_reg: regs.len().saturating_sub(1) as u32, cold: None, reg_types: regs.into(),
        block_index, branch_targets: Vec::new(), fused_tails: Vec::new(), frame_meta: None,
        resolved: std::sync::OnceLock::new(), owner_init: Default::default(), id: Default::default(), forward: Default::default(),
    }
}

fn block(label: &str, instructions: Vec<Instruction>, terminator: Terminator) -> BasicBlock {
    BasicBlock { label: label.to_string(), instructions, terminator }
}
fn ret(r: u32) -> Terminator { Terminator::Ret { reg: Some(r) } }
fn sget(dst: u32, field: &str) -> Instruction {
    Instruction::StaticGet(Box::new(StaticGetInsn { dst, field: field.to_string() }))
}
fn sset(field: &str, val: u32) -> Instruction {
    Instruction::StaticSet(Box::new(StaticSetInsn { field: field.to_string(), val }))
}

/// `F = arg; return F;` with the argument register typed `ty`.
fn set_get(name: &str, field: &str, ty: IrType) -> Function {
    func(name, 1, &[ty, IrType::Unknown], vec![block("entry", vec![sset(field, 0), sget(1, field)], ret(1))])
}

/// `return F;`
fn get(name: &str, field: &str) -> Function {
    func(name, 0, &[IrType::Unknown], vec![block("entry", vec![sget(0, field)], ret(0))])
}

/// A class `name` declaring the given static fields (`(name, type tag)`), optionally with a static
/// constructor `<name>.$cctor`.
fn class(name: &str, id: u32, statics: &[(&str, &str)], cctor: bool) -> Arc<TypeDesc> {
    let layout = Arc::new(ObjectLayout {
        size: 0, field_offsets: Box::new([]), field_sizes: Box::new([]), field_kinds: Box::new([]),
        ref_cells: Box::new([]), tparam_cells: Box::new([]), field_access: Box::new([]),
    });
    let static_fields = statics.iter().map(|(n, t)| FieldDesc {
        name: n.to_string(), type_tag: t.to_string(), attributes: Box::new([]), visibility: 0,
    }).collect();
    Arc::new(TypeDesc {
        class_flags: 0, visibility: 0, name: name.to_string(), base_name: None,
        fields: Vec::new(), field_index: crate::metadata::NameIndex::new(), vtable: Vec::new(),
        vtable_index: crate::metadata::NameIndex::new(),
        cold: Some(Box::new(TypeDescCold {
            composed_object_layout: Some(layout),
            static_fields,
            cctor_func: cctor.then(|| format!("{name}.$cctor").into_boxed_str()),
            ..Default::default()
        })),
        id: TypeId(id),
    })
}

fn module(functions: Vec<Function>, types: Vec<Arc<TypeDesc>>) -> Module {
    let func_index = functions.iter().enumerate().map(|(i, f)| (f.name.clone(), i)).collect();
    Module {
        name: "T".to_string(), string_pool: Vec::new(), classes: Vec::new(), functions,
        type_registry: types.into_iter().map(|t| (t.name.clone(), t)).collect(), func_index,
    }
}

/// A VM running `build()` as its entry module, its JIT installed, every static constructor of the
/// module registered (as `app::run` does after merging).
fn vm_of(build: &dyn Fn() -> Module) -> std::pin::Pin<Box<VmContext>> {
    let vm = VmContext::with_module(build());
    let m = Arc::clone(vm.module().expect("entry module"));
    install(&vm, &m).expect("install");
    for td in m.type_registry.values() {
        vm.register_cctor_of(td);
    }
    vm
}

/// Run entry function `idx` natively on `vm`: (result, static helper calls during the run).
fn run_on(vm: &VmContext, idx: usize, args: &[Value]) -> (Result<Value, String>, usize) {
    let entry = vm.jit().expect("JIT").resolve_fn_by_id(vm, idx).expect("compiles").clone();
    let before = STATIC_HELPER_CALLS.with(|c| c.get());
    let out = match unsafe { call_entry(vm, &entry, JitFrame::new(vm, entry.max_reg, args)) } {
        NativeOutcome::Returned(v) => Ok(v.unwrap_or(Value::Null)),
        NativeOutcome::Threw => Err(thrown_text(vm.take_exception())),
    };
    (out, STATIC_HELPER_CALLS.with(|c| c.get()) - before)
}

/// Run expecting no helper call. A reference store also leaves the native path while any heap of
/// the process is marking (the SATB gate is process-wide), so a run that overlapped a mark is retried.
fn run_native(vm: &VmContext, idx: usize, args: &[Value]) -> Result<Value, String> {
    for _ in 0..1000 {
        let marking_before = crate::gc::satb::marking_any();
        let (out, n) = run_on(vm, idx, args);
        if n == 0 {
            return out;
        }
        assert!(marking_before || crate::gc::satb::marking_any(), "left the native path {n}× with no mark running");
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("a mark kept running for a second");
}

fn thrown_text(v: Option<Value>) -> String {
    match v {
        Some(Value::Str(s)) => s.as_str().to_string(),
        other => format!("{other:?}"),
    }
}

/// The interpreter's answer for entry function `idx` on a fresh VM of the same module.
fn interp(build: &dyn Fn() -> Module, idx: usize, args: &[Value]) -> Result<Value, String> {
    let vm = vm_of(build);
    let m = Arc::clone(vm.module().expect("entry module"));
    match crate::interp::exec_function(&vm, &m, &m.functions[idx], args) {
        Ok(crate::interp::ExecOutcome::Returned(v)) => Ok(v.unwrap_or(Value::Null)),
        Ok(crate::interp::ExecOutcome::Thrown(v)) => Err(thrown_text(Some(v))),
        Err(e) => panic!("interp: {e:#}"),
    }
}

// ── values ──────────────────────────────────────────────────────────────────

/// One `F = v; return F;` per kind, each on its own field.
fn kinds_module() -> Module {
    use IrType::*;
    let kinds = [I32, I64, F64, Bool, Char, Str, Ref, U8, Ref];
    let fs = kinds.iter().enumerate()
        .map(|(i, &ty)| set_get(&format!("T.SetGet{i}"), &format!("T.S.f{i}"), ty)).collect();
    module(fs, Vec::new())
}

#[test]
fn every_kind_round_trips_natively_and_matches_the_interpreter() {
    let vm = vm_of(&kinds_module);
    let obj_td = class("T.Obj", 701, &[], false);
    let obj = vm.heap().alloc_object(Arc::clone(&obj_td), Vec::new(), NativeData::None);
    let arr = vm.heap().alloc_array(vec![Value::I64(1)]);
    let cases = [
        Value::I64(-5), Value::I64(1 << 40), Value::F64(-2.5), Value::Bool(true),
        Value::Char('é'), Value::Str("hi".into()), obj, Value::I64(200), arr,
    ];
    for (i, v) in cases.iter().enumerate() {
        // First run: the set finds the barrier unsettled → the helper runs it, records it and
        // claims the cell's kind; the get is already native.
        assert_eq!(run_on(&vm, i, &[*v]), (Ok(*v), 1), "case {i}: cold");
        // Warm: no helper at all, and the value is the interpreter's.
        assert_eq!(run_native(&vm, i, &[*v]), Ok(*v), "case {i}: warm");
        if !matches!(v, Value::Object(_) | Value::Array(_)) {
            assert_eq!(interp(&kinds_module, i, &[*v]), Ok(*v), "case {i}: interp");
        }
    }
    // `bool` / `char` take only the payload's significant bytes: false / another char round-trip too.
    assert_eq!(run_native(&vm, 3, &[Value::Bool(false)]), Ok(Value::Bool(false)));
    assert_eq!(run_native(&vm, 4, &[Value::Char('z')]), Ok(Value::Char('z')));
    // A reference field set to null natively; the null read is the helper's (it confirms the field).
    assert_eq!(run_on(&vm, 5, &[Value::Null]).1, 1, "null read → helper");
}

/// `long` then `string` then `double` into one field: the protocol boxes the mismatches.
fn kind_change_module() -> Module {
    module(vec![
        set_get("T.L", "T.S.x", IrType::I64),
        set_get("T.Str", "T.S.x", IrType::Str),
        set_get("T.D", "T.S.x", IrType::F64),
        set_get("T.U", "T.S.x", IrType::Unknown),
    ], Vec::new())
}

#[test]
fn a_kind_change_is_boxed_by_the_helper_and_read_back_through_it() {
    let vm = vm_of(&kind_change_module);
    assert_eq!(run_on(&vm, 0, &[Value::I64(3)]), (Ok(Value::I64(3)), 1));
    assert_eq!(run_native(&vm, 0, &[Value::I64(4)]), Ok(Value::I64(4)));
    // The cell's kind is `long`: a string is boxed (set → helper) and unboxed (get → helper).
    assert_eq!(run_on(&vm, 1, &[Value::Str("s".into())]), (Ok(Value::Str("s".into())), 2));
    // From a box nothing goes back to a claimed kind: every access stays on the helper.
    assert_eq!(run_on(&vm, 2, &[Value::F64(1.5)]), (Ok(Value::F64(1.5)), 2));
    assert_eq!(run_on(&vm, 0, &[Value::I64(5)]), (Ok(Value::I64(5)), 2));
    // An erased (`Unknown`) value register always stores through the helper.
    assert_eq!(run_on(&vm, 3, &[Value::I64(6)]).1, 2);
    for (i, v) in [Value::I64(3), Value::Str("s".into()), Value::F64(1.5), Value::I64(6)].into_iter().enumerate() {
        assert_eq!(interp(&kind_change_module, i, &[v]), Ok(v));
    }
}

fn declared_module() -> Module {
    module(vec![get("T.N", "T.Z.n"), get("T.R", "T.Z.r"), get("T.M", "T.Z.missing")],
        vec![class("T.Z", 702, &[("n", "int"), ("r", "string")], false)])
}

#[test]
fn a_never_assigned_value_static_reads_zero_and_then_goes_native() {
    let vm = vm_of(&declared_module);
    // `static int n;` never assigned: the helper writes back its zero, later reads are native.
    assert_eq!(run_on(&vm, 0, &[]), (Ok(Value::I64(0)), 1));
    assert_eq!(run_on(&vm, 0, &[]), (Ok(Value::I64(0)), 0));
    // A null reference static: every read confirms it on the helper.
    for _ in 0..2 {
        assert_eq!(run_on(&vm, 1, &[]), (Ok(Value::Null), 1));
    }
    // An undeclared field throws the interpreter's text.
    let (out, _) = run_on(&vm, 2, &[]);
    assert_eq!(out.map_err(|e| e.contains("not declared")), Err(true));
    assert_eq!(interp(&declared_module, 0, &[]), Ok(Value::I64(0)));
}

#[test]
fn a_running_mark_sends_reference_stores_to_the_helper() {
    let vm = vm_of(&kinds_module);
    let s = Value::Str("x".into());
    let _ = run_on(&vm, 5, &[s]); // settle the barrier
    run_native(&vm, 5, &[s]).expect("native outside a mark");
    // A mark of an unrelated heap id: this thread records nothing, but the gate is global.
    crate::gc::satb::begin_marking(u64::MAX - 11, 1);
    let under_mark = run_on(&vm, 5, &[s]).1;
    crate::gc::satb::end_marking(u64::MAX - 11);
    assert_eq!(under_mark, 1, "SATB must see the overwritten word");
}

// ── the type-initialization barrier ─────────────────────────────────────────

/// `T.C` with `static int V, runs;` and a static constructor `runs = runs + 1; <spin>; V = 7;`
/// (`throws` → it throws `42` instead of setting `V`); `T.Read` returns `T.C.V`.
fn cctor_module(throws: bool, spin: i64) -> Module {
    use IrType::*;
    let mut body = vec![
        sget(0, "T.C.runs"),
        Instruction::ConstI64 { dst: 1, val: 1 },
        Instruction::Add { dst: 0, a: 0, b: 1 },
        sset("T.C.runs", 0),
        Instruction::ConstI64 { dst: 2, val: 0 },
        Instruction::ConstI64 { dst: 3, val: spin },
    ];
    body.push(Instruction::ConstI64 { dst: 4, val: if throws { 42 } else { 7 } });
    let tail = if throws { Terminator::Throw { reg: 4 } } else { Terminator::Ret { reg: None } };
    let cctor = func("T.C.$cctor", 0, &[I64, I64, I64, I64, I64, Bool], vec![
        block("entry", body, Terminator::Br { label: "head".into() }),
        block("head", vec![Instruction::Lt { dst: 5, a: 2, b: 3 }],
            Terminator::BrCond { cond: 5, true_label: "spin".into(), false_label: "exit".into() }),
        block("spin", vec![Instruction::Add { dst: 2, a: 2, b: 1 }], Terminator::Br { label: "head".into() }),
        block("exit", if throws { vec![] } else { vec![sset("T.C.V", 4)] }, tail),
    ]);
    module(vec![get("T.Read", "T.C.V"), cctor],
        vec![class("T.C", 703, &[("V", "int"), ("runs", "int")], true)])
}

fn runs(vm: &VmContext) -> Value {
    vm.static_get("T.C.runs")
}

#[test]
fn the_first_compiled_access_runs_the_static_constructor_exactly_once() {
    let build = || cctor_module(false, 0);
    let vm = vm_of(&build);
    assert_eq!(runs(&vm), Value::Null, "nothing ran yet");
    assert_eq!(run_on(&vm, 0, &[]), (Ok(Value::I64(7)), 1), "first access: helper → cctor");
    assert_eq!(runs(&vm), Value::I64(1));
    assert_eq!(run_on(&vm, 0, &[]), (Ok(Value::I64(7)), 0), "settled: native");
    assert_eq!(runs(&vm), Value::I64(1), "at most once");
    assert_eq!(interp(&build, 0, &[]), Ok(Value::I64(7)));

    // A newly registered static constructor elsewhere moves the epoch: one helper pass, then native.
    vm.core.cctors.register("Other.D", "Other.D.$cctor");
    assert_eq!(run_on(&vm, 0, &[]), (Ok(Value::I64(7)), 1));
    assert_eq!(run_on(&vm, 0, &[]), (Ok(Value::I64(7)), 0));

    // A re-run clears every static and the initializers run again — not skipped by a stale record.
    vm.static_fields_clear();
    assert_eq!(run_on(&vm, 0, &[]), (Ok(Value::I64(7)), 1));
    assert_eq!(runs(&vm), Value::I64(1), "ran again after the clear");
}

#[test]
fn a_throwing_static_constructor_fails_every_access_and_runs_once() {
    let build = || cctor_module(true, 0);
    let vm = vm_of(&build);
    let want = "the type initializer for `T.C` threw an exception";
    for _ in 0..3 {
        let (out, helpers) = run_on(&vm, 0, &[]);
        assert!(out.as_ref().is_err_and(|e| e.contains(want)), "{out:?}");
        assert_eq!(helpers, 1, "a failed type never settles");
    }
    assert_eq!(runs(&vm), Value::I64(1), "never retried");
    let interp_out = interp(&build, 0, &[]);
    assert!(interp_out.as_ref().is_err_and(|e| e.contains(want)), "{interp_out:?}");
}

#[test]
fn concurrent_first_accesses_from_compiled_code_run_the_static_constructor_once() {
    const THREADS: usize = 8;
    let build = || cctor_module(false, 20_000);
    let vm = vm_of(&build);
    vm.jit().expect("JIT").resolve_fn_by_id(&vm, 0).expect("compiled before the threads start");
    let start = std::sync::Barrier::new(THREADS);
    std::thread::scope(|s| {
        for _ in 0..THREADS {
            s.spawn(|| {
                let w = VmContext::new_with_core(vm.core_arc());
                start.wait();
                // Every thread sees the initialized value: the losers wait for the winner.
                assert_eq!(run_on(&w, 0, &[]).0, Ok(Value::I64(7)));
                assert_eq!(run_on(&w, 0, &[]), (Ok(Value::I64(7)), 0), "settled for every thread");
            });
        }
    });
    assert_eq!(runs(&vm), Value::I64(1), "exactly one thread ran the static constructor");
}
