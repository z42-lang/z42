//! Tests for `bytecode::Instruction` / `Function` layout helpers.
//!
//! Name-bearing cold variants are boxed (`Variant(Box<XxxInsn>)`) so the enum
//! stays ≤32 B; `instruction_size_is_slim` pins that invariant.

use super::Instruction;
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
        owner_init: Default::default(),
        id: Default::default(),
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
