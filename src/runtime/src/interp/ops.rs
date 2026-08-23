/// Register-level helper operations for the interpreter execution loop.
///
/// These functions operate directly on the register slice (`&[Value]`)
/// and are specific to the interpreter's execution model.
/// Value conversion helpers (value_to_str, arg_str, etc.) live in corelib::convert.

use crate::metadata::Value;
use anyhow::{bail, Result};

/// Collect register values into a Vec.
pub(super) fn collect_args(regs: &[Value], reg_indices: &[u32]) -> Result<Vec<Value>> {
    reg_indices.iter()
        .map(|&r| regs.get(r as usize).cloned().ok_or_else(|| anyhow::anyhow!("undefined register %{r}")))
        .collect()
}


/// Extract a bool from a register.
pub(super) fn bool_val(regs: &[Value], reg: u32) -> Result<bool> {
    match regs.get(reg as usize) {
        Some(Value::Bool(b)) => Ok(*b),
        Some(other) => bail!("expected bool in register %{reg}, got {:?}", other),
        None => bail!("undefined register %{reg}"),
    }
}

/// Integer/float binary operation with automatic widening.
///
/// converge-vm-arith-semantics (H3): register fetch stays here (the
/// "undefined register" error is interp-model-specific); the scalar rule +
/// type-mismatch is [`crate::semantics::int_binop`], shared with the JIT helper.
pub(super) fn int_binop(
    regs: &[Value],
    a: u32,
    b: u32,
    int_op:   impl Fn(i64, i64) -> i64,
    float_op: impl Fn(f64, f64) -> f64,
) -> Result<Value> {
    let va = regs.get(a as usize).ok_or_else(|| anyhow::anyhow!("undefined register %{a}"))?;
    let vb = regs.get(b as usize).ok_or_else(|| anyhow::anyhow!("undefined register %{b}"))?;
    crate::semantics::int_binop(va, vb, int_op, float_op)
}

/// Integer-only binary operation (bitwise/shift). Rejects floats.
/// Scalar rule shared via [`crate::semantics::int_bitop`] (see `int_binop`).
pub(super) fn int_bitop(
    regs: &[Value],
    a: u32,
    b: u32,
    op: impl Fn(i64, i64) -> i64,
) -> Result<Value> {
    let va = regs.get(a as usize).ok_or_else(|| anyhow::anyhow!("undefined register %{a}"))?;
    let vb = regs.get(b as usize).ok_or_else(|| anyhow::anyhow!("undefined register %{b}"))?;
    crate::semantics::int_bitop(va, vb, op)
}

/// Numeric less-than comparison with automatic widening.
///
/// fix-char-comparison (2026-05-24): `Value::Char` cases added so
/// `c < '0'` / `c >= '9'` etc. work in user code without explicit
/// `(int)c` casts. Char-to-Char compares its codepoint; mixed Char/I64
/// auto-widens Char to I64 (matches how the parallel Eq path treats
/// chars). Pre-fix, every char range check (yaml `_LooksLikeInt` etc.)
/// bailed with "type mismatch in comparison: Char vs Char".
/// interp-superinstr-fusion: the SHARED comparison primitive. Evaluates a
/// [`CmpOp`](crate::metadata::superinstr::CmpOp) over two registers to a `bool`.
/// Used by BOTH the standalone `Lt`/`Le`/`Gt`/`Ge`/`Eq`/`Ne` handlers (via
/// `exec_value`) AND the fused `CmpBr` super-instruction, so the comparison logic
/// lives in exactly one place (no interp/fusion duplication).
#[inline]
pub(super) fn eval_cmp(op: crate::metadata::superinstr::CmpOp, regs: &[Value], a: u32, b: u32) -> Result<bool> {
    // converge-vm-arith-semantics (H3): register fetch here, comparison rule in
    // [`crate::semantics::eval_cmp`] (shared with JIT helper `jit_lt`/`jit_eq`/…).
    crate::semantics::eval_cmp(op, reg_ref(regs, a)?, reg_ref(regs, b)?)
}

#[inline]
fn reg_ref(regs: &[Value], r: u32) -> Result<&Value> {
    regs.get(r as usize).ok_or_else(|| anyhow::anyhow!("undefined register %{r}"))
}

/// interp-typed-superinstr (2026-08-01): the **typed** comparison primitive for
/// super-instructions whose operands `reg_types` confirm are both `I64`. Index
/// access stays bounds-checked (panics, never UB); only the *type* extraction is
/// unchecked (`as_i64_unchecked`), skipping the discriminant branch that
/// [`eval_cmp`] / `numeric_lt` pay for the dynamic case. Infallible — an I64/I64
/// compare can't type-mismatch.
///
/// # Panics
/// If `a` or `b` is out of range (a compiler bug — the index-in-bounds
/// invariant is implied by `typed`, since `is_i64(reg_types, r)` requires
/// `r < reg_types.len() == regs.len()`).
#[inline]
pub(super) fn eval_cmp_i64(op: crate::metadata::superinstr::CmpOp, regs: &[Value], a: u32, b: u32) -> bool {
    use crate::metadata::superinstr::CmpOp;
    // SAFETY: the typed super-instruction was recognized only when
    // `reg_types[a] == reg_types[b] == I64`, the same invariant the JIT's raw
    // i64 arithmetic trusts. `debug_assert` inside `as_i64_unchecked` catches a
    // violation in debug builds.
    let x = unsafe { regs[a as usize].as_i64_unchecked() };
    let y = unsafe { regs[b as usize].as_i64_unchecked() };
    match op {
        CmpOp::Lt => x <  y,
        CmpOp::Le => x <= y,
        CmpOp::Gt => x >  y,
        CmpOp::Ge => x >= y,
        CmpOp::Eq => x == y,
        CmpOp::Ne => x != y,
    }
}

pub(super) fn numeric_lt(regs: &[Value], a: u32, b: u32) -> Result<bool> {
    let va = regs.get(a as usize).ok_or_else(|| anyhow::anyhow!("undefined register %{a}"))?;
    let vb = regs.get(b as usize).ok_or_else(|| anyhow::anyhow!("undefined register %{b}"))?;
    crate::semantics::numeric_lt(va, vb)
}

/// Convert a Value to a usize index/size, rejecting negative values.
pub(super) fn to_usize(v: &Value, ctx: &str) -> Result<usize> {
    match v {
        Value::I64(n) if *n >= 0 => Ok(*n as usize),
        other => bail!("{}: expected non-negative integer, got {:?}", ctx, other),
    }
}
