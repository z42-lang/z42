//! VM-local block-tail execution plans.
use crate::metadata::ir::Reg;

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
