//! Super-instruction fusion framework (interp-superinstr-fusion, 2026-08-01).
//!
//! A **super-instruction** is a tail pattern of a basic block that a backend can
//! execute (interp) or emit (JIT) as ONE step instead of a separate instruction
//! plus terminator — cutting per-instruction dispatch on hot loops.
//!
//! # Design (extensible — this is a "processing framework", add rules over time)
//!
//! * [`SuperInstr`] — the recognized fused forms (one variant per pattern).
//! * [`SuperInstr::recognize`] — the **rule table**: given a block + its
//!   pre-resolved [`BranchTargets`], returns the fused tail (or `None`). Adding a
//!   new fusion = add a `SuperInstr` variant + one arm here + one backend handler.
//! * Recognition is **backend-agnostic** and run ONCE at load
//!   ([`Function::fused_tails`], a side table parallel to `branch_targets`), so the
//!   hot path is a single `Vec` index — zero per-iteration recognition cost.
//!
//! # Backends
//!
//! * **Interpreter** consumes `fused_tails` in its exec loop
//!   (`interp::exec_function_body`) and runs the fused step via the SHARED
//!   comparison primitive (`interp::ops::eval_cmp`), the same one the standalone
//!   `Lt`/`Eq`/… handlers use — no duplicated comparison logic.
//! * **JIT** already specializes `BrCond` natively (reads the bool payload +
//!   `brif`, see `jit/translate.rs`). It could adopt this recognizer to unify the
//!   two, but its native path is orthogonal today, so v1 wires only the interp
//!   (which lacked any fusion). The recognizer lives here — not in `interp` — so a
//!   future JIT unification reuses it without moving code.
//!
//! # v1 rule set
//!
//! * [`SuperInstr::CmpBr`] — `cmp %t, %a, %b` as the block's last instruction with
//!   a `BrCond(%t)` terminator (the canonical loop-condition shape). Saves the bool
//!   store→reload round-trip and one dispatch per iteration. `%t` is still written
//!   (cheap) so any other reader of it is unaffected — no liveness analysis needed.

use super::bytecode::{BasicBlock, BranchTargets, Instruction, Reg, Terminator};

/// A comparison operator, shared by the fused recognizer and the interp's
/// standalone cmp handlers (via `interp::ops::eval_cmp`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp { Lt, Le, Gt, Ge, Eq, Ne }

/// A fused block-tail super-instruction. Extend with new variants as rules grow.
#[derive(Debug, Clone)]
pub enum SuperInstr {
    /// Block's last instruction is `cmp dst, a, b` and its terminator is
    /// `BrCond(dst)`. Fused execution: evaluate the comparison and jump to
    /// `t_blk`/`f_blk` directly, skipping the separate `BrCond` dispatch + the
    /// bool re-read. `dst` is still written so unrelated readers are unaffected.
    CmpBr { op: CmpOp, a: Reg, b: Reg, dst: Reg, t_blk: usize, f_blk: usize },
}

impl SuperInstr {
    /// Recognizer rule table. `targets` is the block's pre-resolved branch targets
    /// (must be index-resolved — fusion is skipped for label-fallback blocks, which
    /// are cold/hand-built). Returns the fused tail or `None`.
    pub fn recognize(block: &BasicBlock, targets: &BranchTargets) -> Option<SuperInstr> {
        // ── rule: cmp + BrCond → CmpBr ──────────────────────────────────────
        if let (Terminator::BrCond { cond, .. }, BranchTargets::BrCond(t, f)) =
            (&block.terminator, targets)
        {
            if let Some((op, dst, a, b)) = as_cmp(block.instructions.last()?) {
                if dst == *cond {
                    return Some(SuperInstr::CmpBr { op, a, b, dst, t_blk: *t, f_blk: *f });
                }
            }
        }
        // (future rules go here: LoadArith, ArithStore, …)
        None
    }
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

/// Precompute the fused-tail side table for a function's blocks (parallel to
/// `branch_targets`). Called once at load. Blocks with no fusable tail get `None`.
pub fn compute_fused_tails(blocks: &[BasicBlock], branch_targets: &[BranchTargets]) -> Vec<Option<SuperInstr>> {
    // Kill-switch / A/B knob: `Z42_NO_FUSION` disables recognition so the interp
    // takes the un-fused path — lets an operator turn the optimization off without a
    // rebuild (e.g. to isolate a suspected regression), and is how the framework's
    // isolated speedup is measured.
    if std::env::var("Z42_NO_FUSION").is_ok() {
        return vec![None; blocks.len()];
    }
    let out: Vec<Option<SuperInstr>> = blocks.iter().zip(branch_targets.iter())
        .map(|(b, t)| SuperInstr::recognize(b, t))
        .collect();
    if std::env::var("Z42_FUSION_DEBUG").is_ok() {
        let n = out.iter().filter(|o| o.is_some()).count();
        if n > 0 { eprintln!("[FUSION] {} of {} blocks fused (CmpBr)", n, blocks.len()); }
    }
    out
}
