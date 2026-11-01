//! Native field IC (P1-4): each case compiles a small function with the real translator, runs it
//! natively (and, for values, on the interpreter), and checks which accesses left the native path
//! through the helper counter (`helpers::object_field::HELPER_CALLS`).

use crate::metadata::ir::ExecMode;
use crate::gc::PROMOTION_THRESHOLD;
use crate::gc::GcMode;
use crate::jit::forward::ForwardLookup;
use crate::jit::frame::JitFrame;
use crate::jit::helpers::object_field::HELPER_CALLS;
use crate::jit::invoke::{call_entry, NativeOutcome};
use crate::jit::runtime::install;
use crate::vm::runtime::{Function, Module};
use crate::metadata::ir::{BasicBlock, FieldGetInsn, FieldSetInsn, Instruction, Terminator};
use crate::vm::runtime::ids::tokens::TypeId;
use crate::object::{
    FieldAccess, NativeData, ObjectLayout, TypeDesc, TypeDescCold, STRUCT_LEAF_GCREF,
    STRUCT_LEAF_PRIM, TAG_I32, TAG_I64, TAG_OBJECT,
};
use crate::{metadata::IrType, object::Value};
use crate::vm_context::VmContext;
use std::sync::Arc;

#[derive(Clone, Copy)]
enum F { I64, I32, Ref }

/// A class whose fields sit in 8 B slots in the given order, under TypeId `id`.
fn class(name: &str, id: u32, fields: &[(&str, F)]) -> Arc<TypeDesc> {
    let n = fields.len();
    let offs: Vec<u32> = (0..n as u32).map(|i| i * 8).collect();
    let access: Vec<FieldAccess> = fields.iter().zip(&offs).map(|((_, f), &o)| match f {
        F::I64 => FieldAccess::prim(o, 8, TAG_I64),
        F::I32 => FieldAccess::prim(o, 4, TAG_I32),
        F::Ref => FieldAccess::ref_word(o, TAG_OBJECT),
    }).collect();
    let layout = Arc::new(ObjectLayout {
        size: n * 8,
        field_offsets: offs.clone().into(),
        field_sizes: fields.iter().map(|(_, f)| if matches!(f, F::I32) { 4 } else { 8 }).collect(),
        field_kinds: fields.iter().map(|(_, f)| if matches!(f, F::Ref) { STRUCT_LEAF_GCREF } else { STRUCT_LEAF_PRIM }).collect(),
        ref_cells: fields.iter().zip(&offs).filter(|((_, f), _)| matches!(f, F::Ref)).map(|(_, &o)| o).collect(),
        tparam_cells: Box::new([]),
        field_access: access.into(),
    });
    let mut field_index = crate::metadata::NameIndex::new();
    for (i, (fname, _)) in fields.iter().enumerate() {
        field_index.insert(fname.to_string(), i);
    }
    Arc::new(TypeDesc {
        class_flags: 0, visibility: 0, name: name.to_string(), base_name: None,
        fields: Vec::new(), field_index, vtable: Vec::new(),
        vtable_index: crate::metadata::NameIndex::new(),
        cold: Some(Box::new(TypeDescCold { composed_object_layout: Some(layout), ..Default::default() })),
        id: TypeId(id),
    })
}

fn new_obj(vm: &VmContext, td: &Arc<TypeDesc>, n: usize) -> Value {
    vm.heap().alloc_object(Arc::clone(td), vec![Value::Null; n], NativeData::None)
}

fn func(name: &str, params: usize, regs: &[IrType], blocks: Vec<BasicBlock>) -> Function {
    let block_index = blocks.iter().enumerate().map(|(i, b)| (b.label.clone(), i)).collect();
    Function::new(crate::metadata::ir::Function {
        name: name.to_string(),
        param_count: params,
        ret_type: "i64".to_string(),
        exec_mode: ExecMode::Jit,
        blocks,
        is_static: true,
        visibility: 0,
        method_flags: 0,
        min_arg: 0,
        params_from: 0xFF,
        max_reg: regs.len().saturating_sub(1) as u32,
        cold: None,
        reg_types: regs.into(),
        block_index,
        branch_targets: Vec::new(),
        frame_meta: None,
    })
}

fn block(label: &str, instructions: Vec<Instruction>, terminator: Terminator) -> BasicBlock {
    BasicBlock { label: label.to_string(), instructions, terminator }
}
fn ret(r: u32) -> Terminator { Terminator::Ret { reg: Some(r) } }
fn br(l: &str) -> Terminator { Terminator::Br { label: l.to_string() } }
fn get(dst: u32, obj: u32, f: &str) -> Instruction {
    Instruction::FieldGet(Box::new(FieldGetInsn { dst, obj, field_name: f.to_string() }))
}
fn set(obj: u32, f: &str, val: u32) -> Instruction {
    Instruction::FieldSet(Box::new(FieldSetInsn { obj, field_name: f.to_string(), val }))
}

fn module(functions: Vec<Function>) -> Module {
    let func_index = functions.iter().enumerate().map(|(i, f)| (f.name.clone(), i)).collect();
    Module::new(crate::metadata::image::ModuleData {
        name: "T".to_string(),
        string_pool: Vec::new(),
        classes: Vec::new(),
        func_index,
    }, functions, rustc_hash::FxHashMap::default())
}

/// The production translator's view of the VM: no call forwarding, the heap's barrier gate.
struct Lookup<'a>(&'a VmContext);
impl ForwardLookup for Lookup<'_> {
    fn forward_of_id(&self, _: u32) -> Option<u32> { None }
    fn forward_of_name(&self, _: &str) -> Option<u32> { None }
    fn barrier_bytes(&self) -> Option<(*const std::sync::atomic::AtomicU8, *const std::sync::atomic::AtomicU8)> {
        self.0.heap().jit_barrier_bytes()
    }
}

/// A compiled function plus its VM; `run` returns (result, helper calls during the run).
struct Jitted<'m> {
    vm: &'m VmContext,
    entry: crate::jit::frame::FnEntry,
}

impl Jitted<'_> {
    fn new<'m>(vm: &'m VmContext, m: &'m Module, idx: usize) -> Jitted<'m> {
        crate::vm::runtime::resolve::resolve_function_tokens(&m.functions[idx], m, vm);
        let rt = install(vm, m).expect("install");
        let entry = rt.compile_with(&m.functions[idx], &Lookup(vm)).expect("compile");
        Jitted { vm, entry }
    }

    fn run(&self, args: &[Value]) -> (Result<Value, String>, usize) {
        let before = HELPER_CALLS.with(|c| c.get());
        let out = match unsafe { call_entry(self.vm, &self.entry, JitFrame::new(self.vm, self.entry.max_reg, args)) } {
            NativeOutcome::Returned(v) => Ok(v.unwrap_or(Value::Null)),
            NativeOutcome::Threw => Err(format!("{:?}", self.vm.take_exception())),
        };
        (out, HELPER_CALLS.with(|c| c.get()) - before)
    }

    /// Run expecting no helper call. A reference store also leaves the native path while any heap
    /// of the process is marking (the SATB gate is process-wide, and other tests' heaps collect
    /// concurrently), so a run that overlapped a mark is retried.
    fn run_native(&self, args: &[Value]) -> Result<Value, String> {
        for _ in 0..1000 {
            let marking_before = crate::gc::satb::marking_any();
            let (out, n) = self.run(args);
            if n == 0 {
                return out;
            }
            assert!(marking_before || crate::gc::satb::marking_any(),
                "left the native path {n}× with no mark running");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("a mark kept running for a second");
    }
}

fn interp(m: &Module, idx: usize, args: &[Value]) -> Value {
    let vm = VmContext::new();
    match crate::interp::exec_function(&vm, m, &m.functions[idx], args) {
        Ok(crate::interp::ExecOutcome::Returned(v)) => v.unwrap_or(Value::Null),
        other => panic!("interp: {other:?}"),
    }
}

// ── IC hit / miss ────────────────────────────────────────────────────────────

/// `o.n = v; return o.n` with an `int` and a `long` field.
fn set_get(field: &str, ty: IrType) -> Module {
    module(vec![func("T.SetGet", 2, &[IrType::Ref, ty, ty], vec![
        block("entry", vec![set(0, field, 1), get(2, 0, field)], ret(2)),
    ])])
}

#[test]
fn primitive_get_and_set_go_native_once_the_ic_is_warm() {
    let vm = VmContext::new();
    let a = class("A", 0x7100_0001, &[("n", F::I32), ("l", F::I64)]);
    for (field, ty) in [("n", IrType::I32), ("l", IrType::I64)] {
        let mut m = set_get(field, ty);
        m.functions[0].name = format!("T.SetGet_{field}"); // one JIT per VM: distinct names
        let j = Jitted::new(&vm, &m, 0);
        let o = new_obj(&vm, &a, 2);
        // First run: both sites miss (cold IC) and the helper installs them.
        assert_eq!(j.run(&[o, Value::I64(-5)]), (Ok(Value::I64(-5)), 2), "{field}: cold");
        // Warm: no helper; the value is exactly what the interpreter reads back.
        assert_eq!(j.run(&[o, Value::I64(7)]), (Ok(Value::I64(7)), 0), "{field}: warm");
        assert_eq!(interp(&m, 0, &[new_obj(&vm, &a, 2), Value::I64(7)]), Value::I64(7));
    }
}

#[test]
fn reference_get_and_set_go_native_and_decode_like_the_interpreter() {
    let vm = VmContext::new();
    let a = class("A", 0x7100_0002, &[("n", F::I64), ("s", F::Ref)]);
    let m = set_get("s", IrType::Ref);
    let j = Jitted::new(&vm, &m, 0);
    let o = new_obj(&vm, &a, 2);
    let child = new_obj(&vm, &a, 2);
    assert_eq!(j.run(&[o, child]).1, 2, "cold");
    for v in [child, Value::Str("hi".into()), Value::Null] {
        assert_eq!(j.run_native(&[o, v]), Ok(v), "{v:?}: native both ways");
    }
    // A primitive into a reference field is boxed by the helper; the read unboxes it via the helper.
    assert_eq!(j.run(&[o, Value::I64(9)]), (Ok(Value::I64(9)), 2));
}

#[test]
fn another_receiver_type_takes_the_helper_and_stays_correct() {
    let vm = VmContext::new();
    let a = class("A", 0x7100_0003, &[("n", F::I64), ("m", F::I64)]);
    let b = class("B", 0x7100_0004, &[("m", F::I64), ("x", F::I64), ("n", F::I64)]);
    let m = set_get("n", IrType::I64);
    let j = Jitted::new(&vm, &m, 0);
    let (oa, ob) = (new_obj(&vm, &a, 2), new_obj(&vm, &b, 3));
    assert_eq!(j.run(&[oa, Value::I64(1)]).1, 2, "cold: entry 0 = A");
    assert_eq!(j.run(&[oa, Value::I64(2)]), (Ok(Value::I64(2)), 0));
    // B is in entry 1: the inline guard (entry 0 only) misses, the helper hits its PIC entry.
    assert_eq!(j.run(&[ob, Value::I64(3)]), (Ok(Value::I64(3)), 2));
    assert_eq!(j.run(&[ob, Value::I64(4)]), (Ok(Value::I64(4)), 2));
    let Value::Object(r) = ob else { unreachable!() };
    assert_eq!(r.borrow().field_value(2), Value::I64(4), "B.n is its third cell");
    assert_eq!(r.borrow().field_value(0), Value::I64(0), "B.m untouched");
    assert_eq!(j.run(&[oa, Value::I64(5)]), (Ok(Value::I64(5)), 0), "A still native");
}

#[test]
fn null_receiver_throws_the_interpreter_text() {
    let vm = VmContext::new();
    let a = class("A", 0x7100_0005, &[("n", F::I64)]);
    let m = module(vec![func("T.Get", 1, &[IrType::Ref, IrType::I64], vec![
        block("entry", vec![get(1, 0, "n")], ret(1)),
    ])]);
    let j = Jitted::new(&vm, &m, 0);
    assert_eq!(j.run(&[new_obj(&vm, &a, 1)]).1, 1);
    let (out, helpers) = j.run(&[Value::Null]);
    assert_eq!(helpers, 1, "null leaves the native path");
    let err = out.unwrap_err();
    assert!(err.contains("cannot read field `n` of a null reference"), "{err}");
}

#[test]
fn a_register_typed_unlike_the_field_never_takes_the_native_path() {
    let vm = VmContext::new();
    // The IC caches `n` as an i64 cell; a site reading it into an `int` register mismatches the
    // kind byte and stays on the helper (which decodes by the layout).
    let a = class("A", 0x7100_0006, &[("n", F::I64)]);
    let m = module(vec![func("T.Get", 1, &[IrType::Ref, IrType::I32], vec![
        block("entry", vec![get(1, 0, "n")], ret(1)),
    ])]);
    let j = Jitted::new(&vm, &m, 0);
    let o = new_obj(&vm, &a, 1);
    let Value::Object(r) = o else { unreachable!() };
    r.borrow().set_field_value(0, &Value::I64(1 << 40));
    for _ in 0..2 {
        assert_eq!(j.run(&[o]), (Ok(Value::I64(1 << 40)), 1));
    }
}

// ── hoisting ────────────────────────────────────────────────────────────────

/// `sum = 0; for (i = 0; i < n; i++) sum += o.v;` — `o` is never written: hoisted.
fn sum_loop() -> Module {
    use IrType::*;
    module(vec![func("T.Sum", 2, &[Ref, I64, I64, I64, I64, Bool, I64], vec![
        block("entry", vec![
            Instruction::ConstI64 { dst: 2, val: 0 },
            Instruction::ConstI64 { dst: 3, val: 0 },
            Instruction::ConstI64 { dst: 6, val: 1 },
        ], br("head")),
        block("head", vec![Instruction::Lt { dst: 5, a: 2, b: 1 }],
            Terminator::BrCond { cond: 5, true_label: "body".into(), false_label: "exit".into() }),
        block("body", vec![
            get(4, 0, "v"),
            Instruction::Add { dst: 3, a: 3, b: 4 },
            Instruction::Add { dst: 2, a: 2, b: 6 },
        ], br("head")),
        block("exit", vec![], ret(3)),
    ])])
}

#[test]
fn a_loop_over_an_unwritten_receiver_hoists_the_guard() {
    let vm = VmContext::new();
    let a = class("A", 0x7100_0007, &[("x", F::I64), ("v", F::I64)]);
    let m = sum_loop();
    assert_eq!(super::super::hoist::loop_blocks(&m.functions[0]), vec![false, true, true, false]);
    let j = Jitted::new(&vm, &m, 0);
    let o = new_obj(&vm, &a, 2);
    let Value::Object(r) = o else { unreachable!() };
    r.borrow().set_field_value(1, &Value::I64(3));
    // Cold: the entry guard misses, the first iteration's per-access guard misses and installs.
    assert_eq!(j.run(&[o, Value::I64(100)]), (Ok(Value::I64(300)), 1));
    // Warm: hoisted at entry, no helper at all.
    assert_eq!(j.run(&[o, Value::I64(100)]), (Ok(Value::I64(300)), 0));
    assert_eq!(interp(&m, 0, &[o, Value::I64(100)]), Value::I64(300));
    // Another receiver type: the hoist misses, every iteration takes the per-access guard → helper.
    let b = class("B", 0x7100_0008, &[("v", F::I64)]);
    let ob = new_obj(&vm, &b, 1);
    let Value::Object(rb) = ob else { unreachable!() };
    rb.borrow().set_field_value(0, &Value::I64(2));
    assert_eq!(j.run(&[ob, Value::I64(10)]), (Ok(Value::I64(20)), 10));
}

/// `sum = 0; while (o != null) { sum += o.v; o = o.next; }` — `o` changes every iteration: never
/// hoisted, every access guards its own receiver (here alternating between two layouts).
#[test]
fn a_receiver_reassigned_in_the_loop_is_guarded_per_access() {
    use IrType::*;
    let m = module(vec![func("T.Walk", 1, &[Ref, I64, I64, Bool, Ref], vec![
        block("entry", vec![
            Instruction::ConstI64 { dst: 1, val: 0 },
            Instruction::ConstNull { dst: 4 },
        ], br("head")),
        block("head", vec![Instruction::Eq { dst: 3, a: 0, b: 4 }],
            Terminator::BrCond { cond: 3, true_label: "exit".into(), false_label: "body".into() }),
        block("body", vec![
            get(2, 0, "v"),
            Instruction::Add { dst: 1, a: 1, b: 2 },
            get(0, 0, "next"),
        ], br("head")),
        block("exit", vec![], ret(1)),
    ])]);
    let vm = VmContext::new();
    let a = class("A", 0x7100_0009, &[("v", F::I64), ("next", F::Ref)]);
    let b = class("B", 0x7100_000A, &[("next", F::Ref), ("pad", F::I64), ("v", F::I64)]);
    // a(1) → b(10) → a(100) → b(1000)
    let nodes = [new_obj(&vm, &a, 2), new_obj(&vm, &b, 3), new_obj(&vm, &a, 2), new_obj(&vm, &b, 3)];
    for (i, (n, v)) in nodes.iter().zip([1, 10, 100, 1000]).enumerate() {
        let Value::Object(r) = n else { unreachable!() };
        let (vslot, nslot) = if i % 2 == 0 { (0, 1) } else { (2, 0) };
        r.borrow().set_field_value(vslot, &Value::I64(v));
        if let Some(next) = nodes.get(i + 1) {
            r.borrow().set_field_value(nslot, next);
        }
    }
    let j = Jitted::new(&vm, &m, 0);
    assert_eq!(j.run(&[nodes[0]]).0, Ok(Value::I64(1111)));
    // Warm: the A nodes go native, the B nodes (entry 1 of both sites) take the helper.
    assert_eq!(j.run(&[nodes[0]]), (Ok(Value::I64(1111)), 4));
    assert_eq!(interp(&m, 0, &[nodes[0]]), Value::I64(1111));
}

// ── write barrier on native reference stores ────────────────────────────────

fn store_fn() -> Module {
    module(vec![func("T.Store", 2, &[IrType::Ref, IrType::Ref], vec![
        block("entry", vec![set(0, "o", 1)], ret(1)),
    ])])
}

fn age_old(v: &Value) {
    let Value::Object(r) = v else { unreachable!() };
    r.var_ref().raise_gen_age_to(PROMOTION_THRESHOLD);
}
fn remembered(v: &Value) -> bool {
    let Value::Object(r) = v else { unreachable!() };
    r.var_ref().is_remembered()
}
fn alive(v: &Value) -> bool {
    let Value::Object(r) = v else { unreachable!() };
    r.var_ref().is_live()
}

/// The old→young edge a compiled store creates survives the next minor: the store either took
/// the helper's barrier or found the owner already remembered.
#[test]
fn an_old_to_young_edge_stored_natively_survives_a_minor() {
    let vm = VmContext::new();
    vm.heap().set_mode(GcMode::GenerationalMarkSweep);
    let a = class("A", 0x7100_000B, &[("o", F::Ref), ("n", F::I64)]);
    let m = store_fn();
    let j = Jitted::new(&vm, &m, 0);

    // The owner is not a root: an old object survives a minor anyway, and the minor traces an old
    // root's children — only an unrooted old owner shows whether the edge was recorded.
    let owner = new_obj(&vm, &a, 2);
    // Warm the IC on a young owner: the cold store is the helper's, the warm one is native (a young
    // owner needs no barrier).
    assert_eq!(j.run(&[owner, new_obj(&vm, &a, 2)]).1, 1);
    j.run_native(&[owner, new_obj(&vm, &a, 2)]).expect("young owner: native");

    age_old(&owner);
    assert!(!remembered(&owner));
    // Old, unremembered owner + young value: the helper stores and remembers.
    assert_eq!(j.run(&[owner, new_obj(&vm, &a, 2)]).1, 1, "barrier needed → helper");
    assert!(remembered(&owner));
    // Remembered: the next young store is native, and the minor still finds it.
    let child = new_obj(&vm, &a, 2);
    j.run_native(&[owner, child]).expect("remembered owner: native");
    vm.heap().force_collect();
    assert!(alive(&child), "the young child stored natively must survive the minor");
    let Value::Object(r) = owner else { unreachable!() };
    assert_eq!(r.borrow().field_value(0), child);
}

#[test]
fn an_old_value_into_an_old_owner_needs_no_barrier() {
    let vm = VmContext::new();
    vm.heap().set_mode(GcMode::GenerationalMarkSweep);
    let a = class("A", 0x7100_000C, &[("o", F::Ref)]);
    let m = store_fn();
    let j = Jitted::new(&vm, &m, 0);
    let (owner, old_child) = (new_obj(&vm, &a, 1), new_obj(&vm, &a, 1));
    let _ = j.run(&[owner, Value::Null]); // warm the IC
    age_old(&owner);
    age_old(&old_child);
    j.run_native(&[owner, old_child]).expect("old → old: native");
    j.run_native(&[owner, Value::Null]).expect("null: native");
    assert!(!remembered(&owner));
    // A string is a var block too: a young one goes to the helper, which remembers the owner.
    assert_eq!(j.run(&[owner, Value::Str("young".into())]).1, 1);
    assert!(remembered(&owner));
}

/// Negative control for the survival test: the same edge written without any barrier is lost.
#[test]
fn the_same_edge_without_a_barrier_is_lost() {
    let vm = VmContext::new();
    vm.heap().set_mode(GcMode::GenerationalMarkSweep);
    let a = class("A", 0x7100_000D, &[("o", F::Ref)]);
    let owner = new_obj(&vm, &a, 1);
    age_old(&owner);
    let child = new_obj(&vm, &a, 1);
    let Value::Object(r) = owner else { unreachable!() };
    r.borrow().set_ref_raw_for_test(0, &child);
    vm.heap().force_collect();
    assert!(!alive(&child), "control: an unrecorded old→young edge does not keep the child");
}

#[test]
fn a_running_mark_sends_reference_stores_to_the_helper() {
    let vm = VmContext::new();
    let a = class("A", 0x7100_000E, &[("o", F::Ref)]);
    let m = store_fn();
    let j = Jitted::new(&vm, &m, 0);
    let owner = new_obj(&vm, &a, 1);
    let _ = j.run(&[owner, Value::Null]); // warm the IC
    j.run_native(&[owner, Value::Null]).expect("native outside a mark");
    // A mark of an unrelated heap id: this thread records nothing, but the gate is global.
    crate::gc::satb::begin_marking(u64::MAX - 7, 1);
    let under_mark = j.run(&[owner, Value::Null]).1;
    crate::gc::satb::end_marking(u64::MAX - 7);
    assert_eq!(under_mark, 1, "SATB must see the overwritten word");
}
