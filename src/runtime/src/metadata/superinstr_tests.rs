//! interp-typed-superinstr (2026-08-01): recognizer tests for the fused
//! super-instruction rule table — covers CmpBr recognition, the `typed`
//! (integer `reg_types`) fast-path flag, and the untyped fallback.

use crate::metadata::bytecode::{BasicBlock, BranchTargets, Instruction, Terminator};
use crate::metadata::ir_type::IrType;
use super::{CmpOp, SuperInstr};

/// Build a block whose last instruction is `cmp dst, a, b` and whose
/// terminator is `BrCond(dst)` — the canonical loop-condition shape.
fn cmp_br_block(dst: u32, a: u32, b: u32) -> BasicBlock {
    BasicBlock {
        label: "L".into(),
        instructions: vec![Instruction::Lt { dst, a, b }],
        terminator: Terminator::BrCond {
            cond: dst,
            true_label: "T".into(),
            false_label: "F".into(),
        },
    }
}

#[test]
fn recognizes_cmp_br_and_types_it_when_operands_are_integer() {
    let block = cmp_br_block(2, 0, 1);
    let targets = BranchTargets::BrCond(3, 4);
    // reg 0 = I64, reg 1 = I32 (both stored as Value::I64 at runtime), reg 2 = Bool (the cmp result).
    let reg_types = [IrType::I64, IrType::I32, IrType::Bool];
    match SuperInstr::recognize(&block, &targets, &reg_types, true) {
        Some(SuperInstr::CmpBr { op, a, b, dst, t_blk, f_blk, typed }) => {
            assert_eq!((op, a, b, dst, t_blk, f_blk), (CmpOp::Lt, 0, 1, 2, 3, 4));
            assert!(typed, "I64+I32 operands ⇒ typed (both are Value::I64 at runtime)");
        }
        other => panic!("expected typed CmpBr, got {other:?}"),
    }
}

#[test]
fn cmp_br_is_untyped_when_an_operand_is_non_integer() {
    let block = cmp_br_block(2, 0, 1);
    let targets = BranchTargets::BrCond(3, 4);
    // reg 1 is a string → cannot use the unchecked i64 path.
    let reg_types = [IrType::I64, IrType::Str, IrType::Bool];
    match SuperInstr::recognize(&block, &targets, &reg_types, true) {
        Some(SuperInstr::CmpBr { typed, .. }) => assert!(!typed, "non-integer operand ⇒ untyped"),
        other => panic!("expected CmpBr, got {other:?}"),
    }
}

#[test]
fn cmp_br_is_untyped_when_reg_types_unavailable() {
    // Empty reg_types (e.g. hand-built / pre-typing functions) ⇒ safe fallback.
    let block = cmp_br_block(2, 0, 1);
    let targets = BranchTargets::BrCond(3, 4);
    match SuperInstr::recognize(&block, &targets, &[], true) {
        Some(SuperInstr::CmpBr { typed, .. }) => assert!(!typed, "no reg_types ⇒ untyped"),
        other => panic!("expected CmpBr, got {other:?}"),
    }
}

#[test]
fn typing_disabled_forces_untyped_even_for_integer_operands() {
    // The `Z42_NO_TYPED_FUSION` A/B knob (typing_enabled=false) forces the
    // untyped path so the typed win can be measured on the same binary.
    let block = cmp_br_block(2, 0, 1);
    let targets = BranchTargets::BrCond(3, 4);
    let reg_types = [IrType::I64, IrType::I64, IrType::Bool];
    match SuperInstr::recognize(&block, &targets, &reg_types, false) {
        Some(SuperInstr::CmpBr { typed, .. }) => assert!(!typed, "typing disabled ⇒ untyped"),
        other => panic!("expected CmpBr, got {other:?}"),
    }
}

#[test]
fn no_fusion_when_terminator_is_not_brcond_on_the_cmp_dst() {
    // cmp writes reg 2 but the BrCond branches on reg 9 → not the loop-condition
    // shape, no fusion.
    let block = BasicBlock {
        label: "L".into(),
        instructions: vec![Instruction::Lt { dst: 2, a: 0, b: 1 }],
        terminator: Terminator::BrCond { cond: 9, true_label: "T".into(), false_label: "F".into() },
    };
    let targets = BranchTargets::BrCond(3, 4);
    let reg_types = [IrType::I64, IrType::I64, IrType::Bool];
    assert!(SuperInstr::recognize(&block, &targets, &reg_types, true).is_none(),
        "BrCond on a different reg than the cmp dst ⇒ no fusion");
}
