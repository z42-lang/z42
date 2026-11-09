//! VM-local block-tail execution plans.

/// A comparison operator, shared by the fused recognizer and the interp's
/// standalone cmp handlers (via `interp::ops::eval_cmp`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

/// A fused block-tail super-instruction. Extend with new variants as rules grow.
#[derive(Debug, Clone)]
pub enum SuperInstr {
    /// Block's last instruction is `cmp dst, a, b` and its terminator is
    /// `BrCond(dst)`. Fused execution: evaluate the comparison and jump to
    /// `t_blk`/`f_blk` directly, skipping the separate `BrCond` dispatch + the
    /// bool re-read. `dst` is still written so unrelated readers are unaffected.
    ///
    /// interp-typed-superinstr : `typed` is set when `reg_types`
    /// confirm both operands are `I64` — the interpreter then compares via
    /// unchecked i64 extraction (no discriminant branch). This is the loop
    /// condition (`i < n`) fast path.
    CmpBr {
        op: CmpOp,
        a: Reg,
        b: Reg,
        dst: Reg,
        t_blk: usize,
        f_blk: usize,
        typed: bool,
    },
}

use crate::zpkg::ir::{BasicBlock, BranchTargets, Instruction, Reg, Terminator};
use crate::zpkg::ir::reg_type::IrType;


impl SuperInstr {
    /// Recognizer rule table. `targets` is the block's pre-resolved branch targets
    /// (must be index-resolved — fusion is skipped for label-fallback blocks, which
    /// are cold/hand-built). `reg_types` is the function's per-register static type
    /// table (empty when unavailable → all fusions stay untyped). Returns the fused
    /// tail or `None`.
    pub fn recognize(block: &BasicBlock, targets: &BranchTargets, reg_types: &[IrType], typing_enabled: bool) -> Option<SuperInstr> {
        // ── rule: cmp + BrCond → CmpBr ──────────────────────────────────────
        if let (Terminator::BrCond { cond, .. }, BranchTargets::BrCond(t, f)) =
            (&block.terminator, targets)
        {
            if let Some((op, dst, a, b)) = as_cmp(block.instructions.last()?) {
                if dst == *cond {
                    // interp-typed-superinstr: both operands statically integer →
                    // interp compares via unchecked i64 (skips tag branch). Uses
                    // `is_integer` (I8..U64), NOT `is_i64`: the VM stores every
                    // narrow integer as `Value::I64`, so `as_i64_unchecked` is
                    // sound for all of them — and loop counters are emitted as
                    // I32 by the compiler, so an I64-only gate would never fire.
                    // Comparison stays signed-i64, exactly as the untyped
                    // `numeric_lt` path already does (byte-identical).
                    //
                    // `typing_enabled` is the `Z42_NO_TYPED_FUSION` A/B knob
                    // (parallel to `Z42_NO_FUSION`), read once at load — forces the
                    // untyped path so the typed win can be measured on the same
                    // binary without a rebuild.
                    let typed = typing_enabled && is_int(reg_types, a) && is_int(reg_types, b);
                    return Some(SuperInstr::CmpBr { op, a, b, dst, t_blk: *t, f_blk: *f, typed });
                }
            }
        }
        // (future rules go here: ArithChain, LoadArith, …)
        None
    }
}

/// True iff `reg_types[r]` is an integer type (I8..U64) — all stored as
/// `Value::I64` at runtime (out-of-range / unavailable → false).
#[inline]
fn is_int(reg_types: &[IrType], r: Reg) -> bool {
    reg_types.get(r as usize).copied().map(|t| t.is_integer()).unwrap_or(false)
}

/// If `ins` is a comparison, return `(op, dst, a, b)` (all `Reg` = `u32`).
fn as_cmp(ins: &Instruction) -> Option<(CmpOp, Reg, Reg, Reg)> {
    match ins {
        Instruction::Lt { dst, a, b } => Some((CmpOp::Lt, *dst, *a, *b)),
        Instruction::Le { dst, a, b } => Some((CmpOp::Le, *dst, *a, *b)),
        Instruction::Gt { dst, a, b } => Some((CmpOp::Gt, *dst, *a, *b)),
        Instruction::Ge { dst, a, b } => Some((CmpOp::Ge, *dst, *a, *b)),
        Instruction::Eq { dst, a, b } => Some((CmpOp::Eq, *dst, *a, *b)),
        Instruction::Ne { dst, a, b } => Some((CmpOp::Ne, *dst, *a, *b)),
        _ => None,
    }
}

#[cfg(test)]
#[path = "fusion_tests.rs"]
mod fusion_tests;
