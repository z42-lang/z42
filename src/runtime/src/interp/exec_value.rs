/// Value-shuffling instructions: constants, copy, arithmetic, comparison,
/// logical, unary, bitwise, string formation. Pure register operations —
/// none of these can throw user exceptions; all errors are VM-internal.

use crate::metadata::{Module, Value};
use crate::vm_context::VmContext;
use anyhow::{bail, Result};

use super::dispatch::{obj_to_string, value_to_str};
use super::ops::{bool_val, int_binop, int_bitop, str_val};
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

pub(super) fn add(frame: &mut Frame, dst: u32, a: u32, b: u32) -> Result<()> {
    let result = match (frame.get(a)?, frame.get(b)?) {
        (Value::Str(sa), Value::Str(sb)) => Value::Str(format!("{}{}", sa, sb).into()),
        (Value::Str(sa), vb)             => Value::Str(format!("{}{}", sa, value_to_str(vb)).into()),
        (va, Value::Str(sb))             => Value::Str(format!("{}{}", value_to_str(va), sb).into()),
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
fn check_int_div_by_zero(
    ctx: &VmContext, module: &Module, regs: &[Value], b: u32, op: &str,
) -> Result<Option<Value>> {
    if matches!(regs.get(b as usize), Some(Value::I64(0))) {
        return Ok(Some(crate::exception::make_stdlib_exception(
            ctx, module, "Std.DivideByZeroException",
            format!("integer {} by zero", op),
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

pub(super) fn str_concat(frame: &mut Frame, dst: u32, a: u32, b: u32) -> Result<()> {
    let sa = str_val(&frame.regs, a)?;
    let sb = str_val(&frame.regs, b)?;
    frame.set(dst, Value::Str(format!("{}{}", sa, sb).into()));
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

/// Target type tag constants — mirror `compiler/z42.IR/BinaryFormat/Opcodes.cs::TypeTags`.
/// Kept inline here (rather than reading from `metadata::tokens`) to keep
/// `convert` self-contained; canonical authority is the C# side.
const T_BOOL: u8 = 0x01;
const T_I8:   u8 = 0x02;
const T_I16:  u8 = 0x03;
const T_I32:  u8 = 0x04;
const T_I64:  u8 = 0x05;
const T_U8:   u8 = 0x06;
const T_U16:  u8 = 0x07;
const T_U32:  u8 = 0x08;
const T_U64:  u8 = 0x09;
const T_F32:  u8 = 0x0A;
const T_F64:  u8 = 0x0B;
const T_CHAR:   u8 = 0x0C;
const T_STR:    u8 = 0x0D;
const T_OBJECT: u8 = 0x20;
const T_ARRAY:  u8 = 0x21;

pub(super) fn convert(frame: &mut Frame, dst: u32, src: u32, to_tag: u8) -> Result<()> {
    let v = frame.get(src)?.clone();
    let result = convert_value(v, to_tag)?;
    frame.set(dst, result);
    Ok(())
}

/// Pure numeric conversion — same dispatch as `convert` but without
/// frame-register read/write side effects, so JIT helpers can reuse it.
///
/// Reference-type identity casts pass through unchanged. The compiler
/// emits a single `Convert` IR for every cast (including `(string)obj`,
/// `(byte[])obj`, `(SomeClass)obj`), so the runtime is responsible for
/// recognising "narrow static type, value already matches" as a no-op
/// rather than a numeric conversion. `Null` is universally castable to
/// any reference target. (add-std-process, 2026-05-13.)
pub fn convert_value(v: Value, to_tag: u8) -> Result<Value> {
    // add-primitive-value-boxing → unify Phase 2 R3: `(T)o` on a boxed primitive → unbox then
    // convert the scalar (`(long)o` boxed-Int64 → I64 identity；`(int)o` → 数值窄化)。基元盒现是
    // `BoxedStruct`（标量存 struct_bytes）；非整数盒（struct 装箱）boxed_prim_i64 返 None → 不拦截。
    if let Value::BoxedStruct(gc) = &v {
        if let Some(n) = gc.borrow().boxed_prim_i64() {
            return convert_value(Value::I64(n), to_tag);
        }
    }
    // Reference-type identity casts — value's dynamic kind already matches the
    // narrowed static target.
    match (&v, to_tag) {
        (Value::Str(_),    T_STR)    => return Ok(v),
        (Value::Array(_),  T_ARRAY)  => return Ok(v),
        (Value::Object(_), T_OBJECT) => return Ok(v),
        // Unbox of a boxed bool: `(bool)o` where o holds a Bool. Bool has no
        // numeric conversion arm (bool↔numeric is rejected at TypeCheck), so an
        // identity match here is the only valid bool unbox path. (add-boxing-conversions)
        (Value::Bool(_),   T_BOOL)   => return Ok(v),
        // Object-to-Array cast (e.g. `(byte[])obj_elem`): tag is T_ARRAY
        // when destination is an array reference type. Already handled above.
        // Null → any reference target.
        (Value::Null,      T_STR | T_OBJECT | T_ARRAY) => return Ok(v),
        _ => {}
    }
    match v {
        Value::F64(f)  => convert_from_f64(f, to_tag),
        Value::I64(x)  => convert_from_i64(x, to_tag),
        Value::Char(c) => convert_from_char(c, to_tag),
        // bool / str / object etc. — TypeChecker should reject; defensive bail
        other => bail!("InvalidCastException: cannot convert {:?} to type tag 0x{:02X}", other, to_tag),
    }
}

fn convert_from_f64(f: f64, to_tag: u8) -> Result<Value> {
    Ok(match to_tag {
        T_F32 | T_F64 => Value::F64(f),
        T_I8  => Value::I64((f as i8) as i64),
        T_I16 => Value::I64((f as i16) as i64),
        T_I32 => Value::I64((f as i32) as i64),
        T_I64 => Value::I64(f as i64),
        T_U8  => Value::I64((f as u8) as i64),
        T_U16 => Value::I64((f as u16) as i64),
        T_U32 => Value::I64((f as u32) as i64),
        T_U64 => Value::I64(f as i64),  // saturating same as f → i64
        T_CHAR => {
            let u = f as u32;
            char::from_u32(u)
                .map(Value::Char)
                .ok_or_else(|| anyhow::anyhow!(
                    "InvalidCastException: 0x{:X} not a valid Unicode scalar", u))?
        }
        T_BOOL => bail!("InvalidCastException: cannot cast f64 to bool"),
        _ => bail!("InvalidCastException: unknown target tag 0x{:02X} for f64 source", to_tag),
    })
}

fn convert_from_i64(x: i64, to_tag: u8) -> Result<Value> {
    Ok(match to_tag {
        T_I8  => Value::I64((x as i8) as i64),
        T_I16 => Value::I64((x as i16) as i64),
        T_I32 => Value::I64((x as i32) as i64),
        T_I64 => Value::I64(x),
        T_U8  => Value::I64((x as u8) as i64),
        T_U16 => Value::I64((x as u16) as i64),
        T_U32 => Value::I64((x as u32) as i64),
        T_U64 => Value::I64(x),
        T_F32 | T_F64 => Value::F64(x as f64),
        T_CHAR => {
            let u = x as u32;
            char::from_u32(u)
                .map(Value::Char)
                .ok_or_else(|| anyhow::anyhow!(
                    "InvalidCastException: 0x{:X} not a valid Unicode scalar", u))?
        }
        T_BOOL => bail!("InvalidCastException: cannot cast int to bool"),
        _ => bail!("InvalidCastException: unknown target tag 0x{:02X} for i64 source", to_tag),
    })
}

fn convert_from_char(c: char, to_tag: u8) -> Result<Value> {
    let u = c as u32;
    Ok(match to_tag {
        T_I8  => Value::I64((u as i8) as i64),
        T_I16 => Value::I64((u as i16) as i64),
        T_I32 => Value::I64(u as i32 as i64),
        T_I64 => Value::I64(u as i64),
        T_U8  => Value::I64((u as u8) as i64),
        T_U16 => Value::I64((u as u16) as i64),
        T_U32 => Value::I64(u as i64),
        T_U64 => Value::I64(u as i64),
        T_F32 | T_F64 => Value::F64(u as f64),
        T_CHAR => Value::Char(c),
        T_BOOL => bail!("InvalidCastException: cannot cast char to bool"),
        _ => bail!("InvalidCastException: unknown target tag 0x{:02X} for char source", to_tag),
    })
}

#[cfg(test)]
#[path = "exec_value_tests.rs"]
mod exec_value_tests;
