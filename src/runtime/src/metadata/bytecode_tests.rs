//! Tests for `bytecode::Instruction` layout + serde wire format.
//!
//! slim-instruction-enum (2026-06-11): name-bearing cold variants are boxed
//! (`Variant(Box<XxxInsn>)`) so the enum stays ≤32 B. These tests pin both the
//! size invariant and the (unchanged) JSON wire format.

use super::{CallInsn, Instruction, ObjNewInsn, StaticSetInsn, TypeofInsn};
use super::{BasicBlock, ExceptionEntry, ExecMode, Function, FunctionCold, Terminator};

/// add-offline-symbolication: build a bare Function with the given per-block
/// instruction counts (bodies are dummy `ConstNull`, terminator `Ret`) to
/// exercise the code-offset ↔ (block, instr) mapping.
fn fn_with_block_sizes(sizes: &[usize]) -> Function {
    let blocks = sizes.iter().enumerate().map(|(bi, &n)| BasicBlock {
        label: format!("b{bi}"),
        instructions: (0..n).map(|d| Instruction::ConstNull { dst: d as u32 }).collect(),
        terminator: Terminator::Ret { reg: None },
    }).collect();
    Function {
        name: "T.f".to_string(),
        param_count: 0,
        ret_type: "void".to_string(),
        exec_mode: ExecMode::Interp,
        blocks,
        is_static: false,
        visibility: 0,
        method_flags: 0, min_arg: 0, params_from: 0xFF,
        max_reg: 0,
        cold: None,
        reg_types: Box::new([]),
        block_index: std::collections::HashMap::new(),
        branch_targets: Vec::new(),
        fused_tails: Vec::new(),
        frame_meta: None,
        resolved: std::sync::OnceLock::new(),
    }
}

// interp-frame-presize: build a Function with the given param count, a single
// block whose instructions each write one of `dsts` (as `ConstNull`), and an
// exception table with the given catch registers. Exercises `reg_file_len`.
fn fn_for_reg_len(param_count: usize, dsts: &[u32], catch_regs: &[u32]) -> Function {
    let mut f = fn_with_block_sizes(&[0]);
    f.param_count = param_count;
    f.blocks[0].instructions =
        dsts.iter().map(|&d| Instruction::ConstNull { dst: d }).collect();
    if !catch_regs.is_empty() {
        let et: Vec<ExceptionEntry> = catch_regs.iter().map(|&r| ExceptionEntry {
            try_start:   "b0".to_string(),
            try_end:     "b0".to_string(),
            catch_label: "b0".to_string(),
            catch_type:  None,
            catch_reg:   r,
        }).collect();
        f.cold = Some(Box::new(FunctionCold {
            exception_table: et.into_boxed_slice(),
            ..Default::default()
        }));
    }
    f
}

#[test]
fn reg_file_len_param_only() {
    // 3 params (regs %0..%2), no write exceeds them → COUNT = 3.
    let f = fn_for_reg_len(3, &[0, 1, 2], &[]);
    assert_eq!(f.reg_file_len(), 3);
}

#[test]
fn reg_file_len_writes_exceed_params() {
    // 2 params but an instruction writes %5 → COUNT = 6 (max index 5 + 1).
    let f = fn_for_reg_len(2, &[0, 5], &[]);
    assert_eq!(f.reg_file_len(), 6);
}

#[test]
fn reg_file_len_folds_unreferenced_catch_reg() {
    // The catch reg (%7) is written only by the runtime at catch-install — no
    // instruction has it as `dst`. `reg_file_len` must still fold it in, else
    // the frame under-sizes and OOB-panics on catch. COUNT = 8.
    let f = fn_for_reg_len(1, &[0], &[7]);
    assert_eq!(f.reg_file_len(), 8);
}

#[test]
fn reg_file_len_empty_is_one() {
    // 0 params, no writes, no catch → COUNT = 1 (never 0, so JIT's
    // `reg_file_len - 1` index never underflows).
    let f = fn_for_reg_len(0, &[], &[]);
    assert_eq!(f.reg_file_len(), 1);
}

#[test]
fn code_offset_roundtrip() {
    // Packed encoding: offset = (block << 16) | instr.
    let f = fn_with_block_sizes(&[2, 1, 3]);

    // Spot-check known sites (instr slots + terminator slots).
    assert_eq!(f.linear_offset(0, 0), 0);
    assert_eq!(f.linear_offset(0, 1), 1);
    assert_eq!(f.linear_offset(0, 2), 2);        // b0 terminator slot
    assert_eq!(f.linear_offset(1, 0), 0x1_0000); // block 1
    assert_eq!(f.linear_offset(2, 0), 0x2_0000);
    assert_eq!(f.linear_offset(2, 2), 0x2_0002);

    // Full round-trip over every valid (block, instr) including terminator slots.
    for (bi, b) in f.blocks.iter().enumerate() {
        for instr in 0..=(b.instructions.len() as u32) {
            let off = f.linear_offset(bi as u32, instr);
            assert_eq!(
                f.offset_to_site(off), (bi as u32, instr),
                "roundtrip mismatch at block {bi} instr {instr} (offset {off})"
            );
        }
    }

    // Offsets are strictly monotonic across the whole function.
    let mut prev = None;
    for bi in 0..f.blocks.len() as u32 {
        let off = f.linear_offset(bi, 0);
        if let Some(p) = prev { assert!(off > p, "offset not monotonic across blocks"); }
        prev = Some(off);
    }
}

#[test]
fn instruction_size_is_slim() {
    let sz = std::mem::size_of::<Instruction>();
    assert!(sz <= 32, "Instruction = {sz} B (slim-instruction-enum target ≤32)");
}

/// A boxed newtype variant whose inner type is a struct must, under
/// `#[serde(tag = "op")]`, merge the tag into the struct's fields — producing
/// the exact same `{"op":..., <fields>}` JSON as the pre-boxing struct variant.
#[test]
fn boxed_variant_json_wire_format_unchanged() {
    let call = Instruction::Call(Box::new(CallInsn {
        dst: 3,
        func: "Foo.bar".into(),
        args: vec![1, 2].into(),
        method_type_args: Box::default(),
    }));
    let json = serde_json::to_value(&call).unwrap();

    // Flat shape: tag + payload fields at the top level, no Box wrapper key.
    assert_eq!(json["op"], "call");
    assert_eq!(json["dst"], 3);
    assert_eq!(json["func"], "Foo.bar");
    assert_eq!(json["args"], serde_json::json!([1, 2]));
    assert!(json.get("0").is_none(), "newtype index key leaked into JSON");
    assert!(json.get("data").is_none(), "wrapper field leaked into JSON");

    // Round-trip: JSON → Instruction → JSON must be byte-identical.
    let back: Instruction = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(serde_json::to_value(&back).unwrap(), json);
}

/// `ObjNew` carries `Box<[String]> type_args` with `#[serde(default)]`; confirm
/// the boxed payload still flattens and round-trips, including the default.
#[test]
fn objnew_typeargs_roundtrip() {
    let obj = Instruction::ObjNew(Box::new(ObjNewInsn {
        dst: 0,
        class_name: "Std.Collections.List".into(),
        ctor_name: "List.ctor".into(),
        args: vec![].into(),
        type_args: vec!["int".to_string()].into(),
        stack_alloc: false,
    }));
    let json = serde_json::to_value(&obj).unwrap();
    assert_eq!(json["op"], "obj_new");
    assert_eq!(json["class_name"], "Std.Collections.List");
    assert_eq!(json["type_args"], serde_json::json!(["int"]));

    let back: Instruction = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(serde_json::to_value(&back).unwrap(), json);
}

/// add-reflection-generic-type-definition: `Typeof` carries `Box<[String]>
/// type_args` (`#[serde(default)]`); confirm the boxed payload round-trips,
/// including a non-empty constructed-generic arg list.
#[test]
fn typeof_typeargs_roundtrip() {
    let tof = Instruction::Typeof(Box::new(TypeofInsn {
        dst: 3,
        type_name: "Demo.Box".into(),
        type_args: vec!["int".to_string()].into(),
    }));
    let json = serde_json::to_value(&tof).unwrap();
    assert_eq!(json["op"], "typeof");
    assert_eq!(json["type_name"], "Demo.Box");
    assert_eq!(json["type_args"], serde_json::json!(["int"]));

    let back: Instruction = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(serde_json::to_value(&back).unwrap(), json);
}

/// A boxed variant with no `dst` (`StaticSet`) still flattens correctly.
#[test]
fn staticset_json_wire_format_unchanged() {
    let set = Instruction::StaticSet(Box::new(StaticSetInsn {
        field: "Mod.counter".into(),
        val: 7,
    }));
    let json = serde_json::to_value(&set).unwrap();
    assert_eq!(json["op"], "static_set");
    assert_eq!(json["field"], "Mod.counter");
    assert_eq!(json["val"], 7);

    let back: Instruction = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(serde_json::to_value(&back).unwrap(), json);
}
