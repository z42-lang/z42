//! Tests for `semantics::convert_value` — numeric cast dispatch table.
//! Spec fix-numeric-cast-lowering (2026-05-13); moved here from
//! `interp/exec_value_tests.rs` by converge-vm-arith-semantics (H3).

use super::*;
use crate::metadata::Value;

// ── f64 → integer ───────────────────────────────────────────────────────────

#[test]
fn f64_to_i64_truncates_positive() {
    let r = convert_value(Value::F64(3.7), T_I64).unwrap();
    assert_eq!(r, Value::I64(3));
}

#[test]
fn f64_to_i64_truncates_negative() {
    let r = convert_value(Value::F64(-3.7), T_I64).unwrap();
    assert_eq!(r, Value::I64(-3));
}

#[test]
fn f64_to_i64_nan_yields_zero() {
    let r = convert_value(Value::F64(f64::NAN), T_I64).unwrap();
    assert_eq!(r, Value::I64(0));
}

#[test]
fn f64_to_i64_pos_inf_saturates() {
    let r = convert_value(Value::F64(f64::INFINITY), T_I64).unwrap();
    assert_eq!(r, Value::I64(i64::MAX));
}

#[test]
fn f64_to_i64_neg_inf_saturates() {
    let r = convert_value(Value::F64(f64::NEG_INFINITY), T_I64).unwrap();
    assert_eq!(r, Value::I64(i64::MIN));
}

#[test]
fn f64_to_i32_truncates_then_narrows() {
    let r = convert_value(Value::F64(3.9), T_I32).unwrap();
    assert_eq!(r, Value::I64(3));
}

#[test]
fn f64_to_i8_narrowing_overflow_saturates_then_truncates() {
    // 300 as i8 = 44 (Rust saturating semantics on float→i8 = 127; but our
    // implementation does f as i8 directly which gives saturated i8 then cast
    // back to i64). Rust f64 `as i8` for 300.0 → 127 (saturating).
    let r = convert_value(Value::F64(300.0), T_I8).unwrap();
    assert_eq!(r, Value::I64(127));
}

// ── i64 narrowing ───────────────────────────────────────────────────────────

#[test]
fn i64_to_i32_high_bits_truncated() {
    // 100_000_000_000 as i32 = -1486618624 (low 32 bits sign-extended)
    // 0x174876E800 → low 32 bits = 0x4876E800 = 1215752192 (positive — high bit 0)
    let r = convert_value(Value::I64(100_000_000_000), T_I32).unwrap();
    assert_eq!(r, Value::I64(1_215_752_192));
}

#[test]
fn i64_to_i16_truncates() {
    let r = convert_value(Value::I64(70_000), T_I16).unwrap();
    assert_eq!(r, Value::I64(4464));
}

#[test]
fn i64_to_u8_truncates() {
    let r = convert_value(Value::I64(300), T_U8).unwrap();
    assert_eq!(r, Value::I64(44));
}

#[test]
fn i64_to_u32_passes_unchanged_in_range() {
    let r = convert_value(Value::I64(42), T_U32).unwrap();
    assert_eq!(r, Value::I64(42));
}

#[test]
fn i64_to_i64_identity() {
    let r = convert_value(Value::I64(-42), T_I64).unwrap();
    assert_eq!(r, Value::I64(-42));
}

// ── int → float ─────────────────────────────────────────────────────────────

#[test]
fn i64_to_f64_widens() {
    let r = convert_value(Value::I64(5), T_F64).unwrap();
    assert_eq!(r, Value::F64(5.0));
}

#[test]
fn i64_to_f64_large_int_precision_loss_is_silent() {
    // 2^53 + 1 — beyond f64's mantissa precision; cast still succeeds
    let r = convert_value(Value::I64(9_007_199_254_740_993), T_F64).unwrap();
    if let Value::F64(d) = r {
        // f64 mantissa rounds to 9007199254740992
        assert_eq!(d, 9_007_199_254_740_992.0);
    } else {
        panic!("expected F64, got {:?}", r);
    }
}

// ── char ↔ int ──────────────────────────────────────────────────────────────

#[test]
fn char_to_i64() {
    let r = convert_value(Value::Char('A'), T_I64).unwrap();
    assert_eq!(r, Value::I64(65));
}

#[test]
fn i64_to_char_basic() {
    let r = convert_value(Value::I64(65), T_CHAR).unwrap();
    assert_eq!(r, Value::Char('A'));
}

#[test]
fn i64_to_char_surrogate_errors() {
    let err = convert_value(Value::I64(0xD800), T_CHAR);
    assert!(err.is_err(), "0xD800 is a surrogate — must fail");
    let msg = format!("{}", err.unwrap_err());
    assert!(msg.contains("InvalidCastException"),
            "expected InvalidCastException in message; got: {msg}");
}

#[test]
fn i64_to_char_above_max_errors() {
    let err = convert_value(Value::I64(0x110000), T_CHAR);
    assert!(err.is_err(), "0x110000 > U+10FFFF — must fail");
}

// ── Rejected source types ───────────────────────────────────────────────────

#[test]
fn bool_source_rejected() {
    let err = convert_value(Value::Bool(true), T_I64);
    assert!(err.is_err());
}

#[test]
fn str_source_rejected() {
    let err = convert_value(Value::Str("5".into()), T_I64);
    assert!(err.is_err());
}

#[test]
fn null_source_rejected() {
    let err = convert_value(Value::Null, T_I64);
    assert!(err.is_err());
}

// ── Rejected target tags ────────────────────────────────────────────────────

#[test]
fn target_bool_rejected_for_int() {
    let err = convert_value(Value::I64(1), T_BOOL);
    assert!(err.is_err());
}

#[test]
fn target_bool_rejected_for_f64() {
    let err = convert_value(Value::F64(1.0), T_BOOL);
    assert!(err.is_err());
}

// ── Boxing: unbox of boxed primitives (add-boxing-conversions) ──────────────

#[test]
fn unbox_bool_identity() {
    // `(bool)o` where o boxes a bool: bool has no numeric arm (bool↔numeric is
    // rejected), so an identity match is the only valid unbox path. Previously
    // fell through to the defensive InvalidCastException bail (bug).
    let r = convert_value(Value::Bool(true), T_BOOL).unwrap();
    assert!(matches!(r, Value::Bool(true)));
}

#[test]
fn unbox_int_via_convert_path() {
    // `(int)o` where o boxes an int → convert path (all z42 ints are I64).
    let r = convert_value(Value::I64(5), T_I32).unwrap();
    assert!(matches!(r, Value::I64(5)));
}

#[test]
fn unbox_char_identity() {
    let r = convert_value(Value::Char('x'), T_CHAR).unwrap();
    assert!(matches!(r, Value::Char('x')));
}

#[test]
fn unbox_mismatch_str_to_bool_throws() {
    // `(bool)o` where o boxes a string → InvalidCastException.
    assert!(convert_value(Value::Str("hi".into()), T_BOOL).is_err());
}

// ── 标量算术 / 比较 / 除零判定（converge-vm-arith-semantics 新增）─────────────

#[test]
fn int_binop_wrapping_add_overflow() {
    let r = int_binop(&Value::I64(i64::MAX), &Value::I64(1), i64::wrapping_add, |x, y| x + y).unwrap();
    assert_eq!(r, Value::I64(i64::MIN));
}

#[test]
fn int_binop_widens_mixed_i64_f64() {
    let r = int_binop(&Value::I64(3), &Value::F64(0.5), i64::wrapping_add, |x, y| x + y).unwrap();
    assert_eq!(r, Value::F64(3.5));
}

#[test]
fn int_binop_type_mismatch_bails() {
    assert!(int_binop(&Value::Bool(true), &Value::I64(1), i64::wrapping_add, |x, y| x + y).is_err());
}

#[test]
fn int_bitop_shift_masks_low_six_bits() {
    // shift amount 64 masks to 0 → identity (matches interp shl/shr `& SHIFT_MASK`).
    let r = int_bitop(&Value::I64(1), &Value::I64(SHIFT_MASK + 1), |x, y| x << (y & SHIFT_MASK)).unwrap();
    assert_eq!(r, Value::I64(1));
}

#[test]
fn numeric_lt_char_i64_widening() {
    assert!(numeric_lt(&Value::Char('0'), &Value::I64(100)).unwrap());
}

#[test]
fn eval_cmp_ne_nan_is_true() {
    // Ne on NaN vs NaN → true (unordered), matching JIT inline FloatCC::NotEqual.
    assert!(eval_cmp(CmpOp::Ne, &Value::F64(f64::NAN), &Value::F64(f64::NAN)).unwrap());
    assert!(!eval_cmp(CmpOp::Eq, &Value::F64(f64::NAN), &Value::F64(f64::NAN)).unwrap());
}

#[test]
fn is_int_div_by_zero_only_integer_zero() {
    assert!(is_int_div_by_zero(&Value::I64(0)));
    assert!(!is_int_div_by_zero(&Value::F64(0.0)));   // float /0 → IEEE Infinity, not throw
    assert!(!is_int_div_by_zero(&Value::I64(1)));
}
