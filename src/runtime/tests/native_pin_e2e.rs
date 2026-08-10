//! Spec C4 end-to-end: PinPtr / UnpinPtr / FieldGet on PinnedView.
//!
//! Hand-crafted IR drives the pin pipeline in the absence of the user-side
//! `pinned p = s { ... }` syntax (which lands in C5). Each test verifies
//! one slice of the runtime: construction, field projection, error path.

use std::collections::HashMap;

use z42::metadata::{
    BasicBlock, CallNativeInsn, ExecMode, FieldGetInsn, Function, Instruction, Module, Terminator, Value,
};
use z42::vm_context::VmContext;

fn build_module(name: &str, instructions: Vec<Instruction>, terminator: Terminator) -> Module {
    let func = Function {
        name: format!("{name}.Main"),
        param_count: 0,
        ret_type: "i64".to_string(),
        exec_mode: ExecMode::Interp,
        blocks: vec![BasicBlock {
            label: "entry".to_string(),
            instructions,
            terminator,
        }],
        is_static: true,
        visibility: 0, method_flags: 0, min_arg: 0, params_from: 0xFF,        max_reg: 8,
        cold: None,
        reg_types: Box::new([]),
        block_index: HashMap::new(),
        branch_targets: Vec::new(),
        fused_tails: Vec::new(),
        frame_meta: None,
        resolved: std::sync::OnceLock::new(),
    };
    Module {
        name: name.to_string(),
        string_pool: vec!["hello world".to_string()],
        classes: vec![],
        functions: vec![func],
        type_registry: HashMap::new(),
        type_registry_vec: Vec::new(),
        func_index: HashMap::new(),
        func_ref_cache_slots: 0,
        interned_strings: Vec::new(),
    }
}

fn run_returning(module: &Module) -> anyhow::Result<Option<Value>> {
    let ctx = VmContext::new();
    let func = &module.functions[0];
    z42::interp::run_returning(&ctx, module, func, &[] as &[Value])
}

#[test]
fn pin_str_then_field_len_returns_byte_length() {
    let m = build_module(
        "pin_field_len",
        vec![
            Instruction::ConstStr { dst: 0, idx: 0 },           // r0 = "hello world"
            Instruction::PinPtr { dst: 1, src: 0 },             // r1 = PinnedView{...}
            Instruction::FieldGet(Box::new(FieldGetInsn { dst: 2, obj: 1, field_name: "len".to_string() })),
            Instruction::UnpinPtr { pinned: 1 },
        ],
        Terminator::Ret { reg: Some(2) },
    );
    let out = run_returning(&m).expect("pin field len ok");
    assert_eq!(out, Some(Value::I64("hello world".len() as i64)));
}

#[test]
fn pin_str_then_field_ptr_returns_nonzero() {
    let m = build_module(
        "pin_field_ptr",
        vec![
            Instruction::ConstStr { dst: 0, idx: 0 },
            Instruction::PinPtr { dst: 1, src: 0 },
            Instruction::FieldGet(Box::new(FieldGetInsn { dst: 2, obj: 1, field_name: "ptr".to_string() })),
            Instruction::UnpinPtr { pinned: 1 },
        ],
        Terminator::Ret { reg: Some(2) },
    );
    let out = run_returning(&m).expect("pin field ptr ok");
    let Some(Value::I64(ptr)) = out else {
        panic!("expected I64, got {out:?}");
    };
    assert!(ptr != 0, "PinPtr must produce a non-NULL ptr for a non-empty String");
}

#[test]
fn pin_view_unknown_field_traps() {
    let m = build_module(
        "pin_unknown_field",
        vec![
            Instruction::ConstStr { dst: 0, idx: 0 },
            Instruction::PinPtr { dst: 1, src: 0 },
            Instruction::FieldGet(Box::new(FieldGetInsn { dst: 2, obj: 1, field_name: "lulz".to_string() })),
        ],
        Terminator::Ret { reg: Some(2) },
    );
    let err = run_returning(&m).expect_err("unknown PinnedView field must fail");
    let msg = format!("{err:#}");
    assert!(msg.contains("PinnedView"), "msg = {msg}");
    assert!(msg.contains("lulz"), "msg = {msg}");
}

#[test]
fn pin_empty_array_returns_zero_length_view() {
    // Spec C10 — empty Array<u8> pin now accepted; produces a zero-length
    // PinnedView (ptr address may be the empty-Box dangling sentinel; len = 0).
    let m = build_module(
        "pin_empty_array",
        vec![
            Instruction::ArrayNewLit(Box::new(z42::metadata::bytecode::ArrayNewLitInsn { dst: 0, elems: vec![].into(), element_type: String::new(), stack_alloc: false })),
            Instruction::PinPtr { dst: 1, src: 0 },
            Instruction::FieldGet(Box::new(FieldGetInsn { dst: 2, obj: 1, field_name: "len".to_string() })),
            Instruction::UnpinPtr { pinned: 1 },
        ],
        Terminator::Ret { reg: Some(2) },
    );
    let out = run_returning(&m).expect("empty Array<u8> pin ok");
    assert_eq!(out, Some(Value::I64(0)));
}

#[test]
fn pin_array_u8_snapshots_bytes() {
    // Spec C10 — Array of I64 in 0..=255 pins as a byte buffer.
    use z42::vm_context::VmContext;
    let ctx = VmContext::new();
    let m = build_module(
        "pin_byte_array",
        vec![
            Instruction::ConstI64 { dst: 0, val: 0x68 }, // 'h'
            Instruction::ConstI64 { dst: 1, val: 0x69 }, // 'i'
            Instruction::ArrayNewLit(Box::new(z42::metadata::bytecode::ArrayNewLitInsn { dst: 2, elems: vec![0, 1].into(), element_type: String::new(), stack_alloc: false })),
            Instruction::PinPtr { dst: 3, src: 2 },
            Instruction::FieldGet(Box::new(FieldGetInsn { dst: 4, obj: 3, field_name: "len".to_string() })),
            Instruction::UnpinPtr { pinned: 3 },
        ],
        Terminator::Ret { reg: Some(4) },
    );
    let func = &m.functions[0];
    let out = z42::interp::run_returning(&ctx, &m, func, &[] as &[Value])
        .expect("byte array pin ok");
    assert_eq!(out, Some(Value::I64(2)));
    // After UnpinPtr the owned-buffer table should be empty.
    assert_eq!(
        ctx.pinned_owned_buffer_count(),
        0,
        "UnpinPtr must release the owned Box<[u8]>"
    );
}

#[test]
fn pin_array_with_out_of_range_element_traps() {
    // Out-of-range element should throw Std.InvalidMarshalException; in a
    // stdlib-less test module the marshal-failure path surfaces as
    // "stdlib type `Std.InvalidMarshalException` not loaded".
    let m = build_module(
        "pin_oor",
        vec![
            Instruction::ConstI64 { dst: 0, val: 256 },          // out of u8 range
            Instruction::ArrayNewLit(Box::new(z42::metadata::bytecode::ArrayNewLitInsn { dst: 1, elems: vec![0].into(), element_type: String::new(), stack_alloc: false })),
            Instruction::PinPtr { dst: 2, src: 1 },
        ],
        Terminator::Ret { reg: None },
    );
    let err = run_returning(&m).expect_err("element 256 must fail");
    let msg = format!("{err:#}");
    assert!(msg.contains("InvalidMarshalException"), "msg = {msg}");
}

#[test]
fn pin_array_with_negative_element_traps() {
    let m = build_module(
        "pin_neg",
        vec![
            Instruction::ConstI64 { dst: 0, val: -1 },
            Instruction::ArrayNewLit(Box::new(z42::metadata::bytecode::ArrayNewLitInsn { dst: 1, elems: vec![0].into(), element_type: String::new(), stack_alloc: false })),
            Instruction::PinPtr { dst: 2, src: 1 },
        ],
        Terminator::Ret { reg: None },
    );
    let err = run_returning(&m).expect_err("element -1 must fail");
    let msg = format!("{err:#}");
    assert!(msg.contains("InvalidMarshalException"), "msg = {msg}");
}

#[test]
fn unpin_then_str_still_usable() {
    // PinPtr / UnpinPtr is zero-cost on the RC backend; the source string
    // remains accessible afterwards.
    let m = build_module(
        "pin_unpin_then_use",
        vec![
            Instruction::ConstStr { dst: 0, idx: 0 },
            Instruction::PinPtr { dst: 1, src: 0 },
            Instruction::UnpinPtr { pinned: 1 },
            Instruction::FieldGet(Box::new(FieldGetInsn { dst: 2, obj: 0, field_name: "Length".to_string() })),
        ],
        Terminator::Ret { reg: Some(2) },
    );
    let out = run_returning(&m).expect("post-unpin String access ok");
    assert_eq!(out, Some(Value::I64("hello world".chars().count() as i64)));
}
