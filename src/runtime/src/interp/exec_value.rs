/// Value-shuffling instructions: constants, copy, arithmetic, comparison,
/// logical, unary, bitwise, string formation. Pure register operations —
/// none of these can throw user exceptions; all errors are VM-internal.

use crate::metadata::{Module, Value};
use crate::vm_context::VmContext;
use anyhow::{bail, Result};

use super::dispatch::{obj_to_string, value_to_str};
use super::ops::{bool_val, int_binop, int_bitop};
use super::Frame;

// ── Constants ────────────────────────────────────────────────────────────

pub(super) fn const_str(
    ctx: &VmContext, module: &Module, frame: &mut Frame, dst: u32, idx: u32,
) -> Result<()> {
    let i = idx as usize;
    // unify-gc-heap PR-4: string bytes live in the GC heap now, so the interned
    // pool can't be materialized at module-load time (no heap exists then). Intern
    // lazily from the live heap + cache per-context (`intern_const_str`, main pool),
    // falling back to the lazy-overflow pool for indices past it. This preserves the
    // amortization the old pre-interned `Vec<Str>` gave (first hit allocs, later hits
    // copy the 8-byte handle), but heap-safe.
    let s = if let Some(s) = ctx.intern_const_str(module, i) {
        s
    } else if let Some(arc) = ctx.try_lookup_string(i) {
        // ConstStr from a lazily-loaded function — idx is offset past main pool.
        arc
    } else {
        bail!("string pool index {idx} out of range");
    };
    frame.set(dst, Value::Str(s));
    Ok(())
}

pub(super) fn const_i32(frame: &mut Frame, dst: u32, val: i32)   { frame.set(dst, Value::I64(val as i64)); }
pub(super) fn const_i64(frame: &mut Frame, dst: u32, val: i64)   { frame.set(dst, Value::I64(val)); }
pub(super) fn const_f64(frame: &mut Frame, dst: u32, val: f64)   { frame.set(dst, Value::F64(val)); }
pub(super) fn const_bool(frame: &mut Frame, dst: u32, val: bool) { frame.set(dst, Value::Bool(val)); }
pub(super) fn const_char(frame: &mut Frame, dst: u32, val: char) { frame.set(dst, Value::Char(val)); }
pub(super) fn const_null(frame: &mut Frame, dst: u32)            { frame.set(dst, Value::Null); }

pub(super) fn copy(frame: &mut Frame, dst: u32, src: u32) -> Result<()> {
    let v = frame.get(src)?.clone();
    frame.set(dst, v);
    Ok(())
}

// ── Arithmetic ───────────────────────────────────────────────────────────

pub(super) fn add(ctx: &VmContext, frame: &mut Frame, dst: u32, a: u32, b: u32) -> Result<()> {
    let result = match (frame.get(a)?, frame.get(b)?) {
        // fuse-str-concat-alloc: allocate the concatenation as one fused GC block,
        // skipping the intermediate `format!` String (mixed arms still build one
        // `String` for the non-string operand via `value_to_str`).
        (Value::Str(sa), Value::Str(sb)) => Value::Str(ctx.heap().alloc_str_concat2(sa, sb)),
        (Value::Str(sa), vb)             => Value::Str(ctx.heap().alloc_str_concat2(sa, &value_to_str(vb))),
        (va, Value::Str(sb))             => Value::Str(ctx.heap().alloc_str_concat2(&value_to_str(va), sb)),
        // 2026-04-28 vm-wrapping-int-arith: wrapping_add（与 Rust release build /
        // C# unchecked int / Java int 一致），解锁 hash / PRNG / 校验和算法
        _ => int_binop(&frame.regs, a, b, i64::wrapping_add, |x, y| x + y)?,
    };
    frame.set(dst, result);
    Ok(())
}

pub(super) fn sub(frame: &mut Frame, dst: u32, a: u32, b: u32) -> Result<()> {
    frame.set(dst, int_binop(&frame.regs, a, b, i64::wrapping_sub, |x, y| x - y)?);
    Ok(())
}

pub(super) fn mul(frame: &mut Frame, dst: u32, a: u32, b: u32) -> Result<()> {
    frame.set(dst, int_binop(&frame.regs, a, b, i64::wrapping_mul, |x, y| x * y)?);
    Ok(())
}

/// fix-int-div-by-zero-panic (2026-05-25): pre-fix, `int / 0` and
/// `int % 0` panicked the VM via Rust's `i64::div` instead of throwing
/// a catchable z42 exception. (Float divide-by-zero correctly yielded
/// Infinity per IEEE 754 — no fix needed there.) Now: detect zero
/// divisor for the integer case before the panic point and surface
/// `Std.DivideByZeroException` via `make_stdlib_exception` so callers
/// can `try / catch (DivideByZeroException)`.
///
/// Signature changed to `Result<Option<Value>>` (Some = thrown user
/// exception, None = success) mirroring the `Call` / `Builtin`
/// propagation pattern.
pub(super) fn div(
    ctx: &VmContext, module: &Module, frame: &mut Frame, dst: u32, a: u32, b: u32,
) -> Result<Option<Value>> {
    if let Some(thrown) = check_int_div_by_zero(ctx, module, &frame.regs, b, "/")? {
        return Ok(Some(thrown));
    }
    frame.set(dst, int_binop(&frame.regs, a, b, |x, y| x / y, |x, y| x / y)?);
    Ok(None)
}

pub(super) fn rem(
    ctx: &VmContext, module: &Module, frame: &mut Frame, dst: u32, a: u32, b: u32,
) -> Result<Option<Value>> {
    if let Some(thrown) = check_int_div_by_zero(ctx, module, &frame.regs, b, "%")? {
        return Ok(Some(thrown));
    }
    frame.set(dst, int_binop(&frame.regs, a, b, |x, y| x % y, |x, y| x % y)?);
    Ok(None)
}

/// Build a `Std.DivideByZeroException` if `regs[b]` is `Value::I64(0)`.
/// Float divisors fall through (IEEE 754 gives Infinity / NaN); mixed
/// I64/F64 also falls through (the int_binop float widening handles
/// the zero case via float semantics). Returns `Ok(Some(exc))` to
/// indicate a user exception to propagate, `Ok(None)` to continue.
///
/// The zero-divisor decision + exception name + message are sourced from
/// [`crate::semantics`] (shared with the JIT helper's `throw_int_div_by_zero`);
/// only the `make_stdlib_exception` construction (needs ctx/module) stays here.
fn check_int_div_by_zero(
    ctx: &VmContext, module: &Module, regs: &[Value], b: u32, op: &str,
) -> Result<Option<Value>> {
    if regs.get(b as usize).is_some_and(crate::semantics::is_int_div_by_zero) {
        return Ok(Some(crate::exception::make_stdlib_exception(
            ctx, module, crate::semantics::DIV_BY_ZERO_EXC,
            crate::semantics::div_by_zero_msg(op),
        )?));
    }
    Ok(None)
}

// ── Comparison ───────────────────────────────────────────────────────────

// interp-superinstr-fusion: all six comparisons route through the shared
// `ops::eval_cmp` primitive (also used by the fused `CmpBr` super-instruction).
use crate::metadata::superinstr::CmpOp;

pub(super) fn eq(frame: &mut Frame, dst: u32, a: u32, b: u32) -> Result<()> {
    frame.set(dst, Value::Bool(super::ops::eval_cmp(CmpOp::Eq, &frame.regs, a, b)?));
    Ok(())
}

pub(super) fn ne(frame: &mut Frame, dst: u32, a: u32, b: u32) -> Result<()> {
    frame.set(dst, Value::Bool(super::ops::eval_cmp(CmpOp::Ne, &frame.regs, a, b)?));
    Ok(())
}

pub(super) fn lt(frame: &mut Frame, dst: u32, a: u32, b: u32) -> Result<()> {
    frame.set(dst, Value::Bool(super::ops::eval_cmp(CmpOp::Lt, &frame.regs, a, b)?));
    Ok(())
}

pub(super) fn le(frame: &mut Frame, dst: u32, a: u32, b: u32) -> Result<()> {
    frame.set(dst, Value::Bool(super::ops::eval_cmp(CmpOp::Le, &frame.regs, a, b)?));
    Ok(())
}

pub(super) fn gt(frame: &mut Frame, dst: u32, a: u32, b: u32) -> Result<()> {
    frame.set(dst, Value::Bool(super::ops::eval_cmp(CmpOp::Gt, &frame.regs, a, b)?));
    Ok(())
}

pub(super) fn ge(frame: &mut Frame, dst: u32, a: u32, b: u32) -> Result<()> {
    frame.set(dst, Value::Bool(super::ops::eval_cmp(CmpOp::Ge, &frame.regs, a, b)?));
    Ok(())
}

// ── Logical ──────────────────────────────────────────────────────────────

pub(super) fn and(frame: &mut Frame, dst: u32, a: u32, b: u32) -> Result<()> {
    frame.set(dst, Value::Bool(bool_val(&frame.regs, a)? && bool_val(&frame.regs, b)?));
    Ok(())
}

pub(super) fn or(frame: &mut Frame, dst: u32, a: u32, b: u32) -> Result<()> {
    frame.set(dst, Value::Bool(bool_val(&frame.regs, a)? || bool_val(&frame.regs, b)?));
    Ok(())
}

pub(super) fn not(frame: &mut Frame, dst: u32, src: u32) -> Result<()> {
    frame.set(dst, Value::Bool(!bool_val(&frame.regs, src)?));
    Ok(())
}

// ── Unary arithmetic ─────────────────────────────────────────────────────

pub(super) fn neg(frame: &mut Frame, dst: u32, src: u32) -> Result<()> {
    let res = match frame.get(src)? {
        Value::I64(n) => Value::I64(-n),
        Value::F64(f) => Value::F64(-f),
        other => bail!("Neg: expected numeric, got {:?}", other),
    };
    frame.set(dst, res);
    Ok(())
}

// ── Bitwise ──────────────────────────────────────────────────────────────

pub(super) fn bit_and(frame: &mut Frame, dst: u32, a: u32, b: u32) -> Result<()> {
    frame.set(dst, int_bitop(&frame.regs, a, b, |x, y| x & y)?);
    Ok(())
}

pub(super) fn bit_or(frame: &mut Frame, dst: u32, a: u32, b: u32) -> Result<()> {
    frame.set(dst, int_bitop(&frame.regs, a, b, |x, y| x | y)?);
    Ok(())
}

pub(super) fn bit_xor(frame: &mut Frame, dst: u32, a: u32, b: u32) -> Result<()> {
    frame.set(dst, int_bitop(&frame.regs, a, b, |x, y| x ^ y)?);
    Ok(())
}

pub(super) fn bit_not(frame: &mut Frame, dst: u32, src: u32) -> Result<()> {
    let res = match frame.get(src)? {
        Value::I64(n) => Value::I64(!n),
        other => bail!("BitNot: expected integral, got {:?}", other),
    };
    frame.set(dst, res);
    Ok(())
}

pub(super) fn shl(frame: &mut Frame, dst: u32, a: u32, b: u32) -> Result<()> {
    frame.set(dst, int_bitop(&frame.regs, a, b, |x, y| x << (y & 63))?);
    Ok(())
}

pub(super) fn shr(frame: &mut Frame, dst: u32, a: u32, b: u32) -> Result<()> {
    frame.set(dst, int_bitop(&frame.regs, a, b, |x, y| x >> (y & 63))?);
    Ok(())
}

// ── String formation ─────────────────────────────────────────────────────

pub(super) fn str_concat(ctx: &VmContext, frame: &mut Frame, dst: u32, a: u32, b: u32) -> Result<()> {
    // fuse-str-concat-alloc: borrow both operands as `&str` and allocate the
    // concatenation as a single fused GC block — no per-operand `str_val` String
    // clone, no intermediate `format!` String (was 4 allocations, now 1).
    let s = match (frame.get(a)?, frame.get(b)?) {
        (Value::Str(sa), Value::Str(sb)) => ctx.heap().alloc_str_concat2(sa, sb),
        (va, vb) => bail!("StrConcat: expected two strings, got {:?} and {:?}", va, vb),
    };
    frame.set(dst, Value::Str(s));
    Ok(())
}

pub(super) fn to_str(
    ctx: &VmContext, module: &Module, frame: &mut Frame, dst: u32, src: u32,
) -> Result<()> {
    let s = obj_to_string(ctx, module, frame.get(src)?)?;
    frame.set(dst, Value::Str(s.into()));
    Ok(())
}

// ── Numeric cast (spec fix-numeric-cast-lowering, 2026-05-13) ────────────
//
// converge-vm-arith-semantics (H3): the cast dispatch table + tag constants
// moved to `crate::semantics::convert_value` (shared with the JIT
// `jit_convert` helper); this handler is now just the frame read/write wrapper.

pub(super) fn convert(frame: &mut Frame, dst: u32, src: u32, to_tag: u8) -> Result<()> {
    let v = frame.get(src)?.clone();
    let result = crate::semantics::convert_value(v, to_tag)?;
    frame.set(dst, result);
    Ok(())
}
