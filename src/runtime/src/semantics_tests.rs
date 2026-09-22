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

/// fix-mixed-numeric-equality：`Eq`/`Ne` 的加宽必须与 `numeric_lt` 一致。
///
/// 回归前这里每一条 `Eq` 都是 `false`、每一条 `Ne` 都是 `true`——因为 `eval_cmp` 把
/// `Eq`/`Ne` 直接委给 `Value: PartialEq`，而后者按变体配对、没有混合数值臂。
/// 四种跨类顺序（F64/I64、I64/F64、Char/I64、I64/Char）各测一遍，缺一条就漏一个方向。
#[test]
fn eval_cmp_eq_widens_mixed_numerics() {
    let pairs = [
        (Value::I64(5), Value::F64(5.0)),
        (Value::F64(5.0), Value::I64(5)),
        (Value::Char('A'), Value::I64(65)),
        (Value::I64(65), Value::Char('A')),
    ];
    for (a, b) in &pairs {
        assert!(eval_cmp(CmpOp::Eq, a, b).unwrap(), "Eq {a:?} {b:?} 应为 true");
        assert!(!eval_cmp(CmpOp::Ne, a, b).unwrap(), "Ne {a:?} {b:?} 应为 false");
    }
}

/// 加宽不能把「不相等」也判成相等——否则上面的测试用一句 `=> true` 就能骗过。
#[test]
fn eval_cmp_eq_mixed_numerics_unequal_stays_false() {
    let pairs = [
        (Value::I64(5), Value::F64(6.0)),
        (Value::F64(5.5), Value::I64(5)),
        (Value::Char('A'), Value::I64(66)),
        (Value::I64(66), Value::Char('A')),
    ];
    for (a, b) in &pairs {
        assert!(!eval_cmp(CmpOp::Eq, a, b).unwrap(), "Eq {a:?} {b:?} 应为 false");
        assert!(eval_cmp(CmpOp::Ne, a, b).unwrap(), "Ne {a:?} {b:?} 应为 true");
    }
}

/// 非数值相等仍走 `Value: PartialEq`，不受加宽影响（`numeric_eq` 的 `_` 臂）。
#[test]
fn eval_cmp_eq_non_numeric_unchanged() {
    assert!(eval_cmp(CmpOp::Eq, &Value::Str("hi".into()), &Value::Str("hi".into())).unwrap());
    assert!(!eval_cmp(CmpOp::Eq, &Value::Str("hi".into()), &Value::Str("ho".into())).unwrap());
    assert!(eval_cmp(CmpOp::Eq, &Value::Null, &Value::Null).unwrap());
    assert!(eval_cmp(CmpOp::Eq, &Value::Bool(true), &Value::Bool(true)).unwrap());
    // 跨类别（数值 vs 非数值）仍是 false，不得因加宽而误判
    assert!(!eval_cmp(CmpOp::Eq, &Value::I64(0), &Value::Null).unwrap());
    assert!(!eval_cmp(CmpOp::Eq, &Value::I64(1), &Value::Bool(true)).unwrap());
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

// ── make-hard-cast-fail-properly：hard_cast_failure ─────────────────────────

mod hard_cast {
    use super::super::*;
    use crate::metadata::types::{TAG_I32, TAG_I64, TAG_F64, TAG_BOOL, TAG_CHAR, TAG_STR, TAG_OBJECT, TAG_ARRAY};

    /// 🔒 **本组最要紧的一条**：`hard_cast_failure` 与 `convert_value` 的放行条件必须严格互补。
    ///
    /// 若某个组合两边都放行，`convert_value` 的防御性 `bail!` 会以**内部错误**泄漏给用户
    /// （不可 catch、Rust Debug 格式的消息）——那正是本变更要消灭的形态。
    /// 若某个组合两边都拒，合法转换会被误抛。
    #[test]
    fn failure_predicate_is_complementary_to_convert_value() {
        let values = [
            Value::I64(7), Value::F64(1.5), Value::Bool(true), Value::Char('x'),
            Value::Str("s".into()), Value::Null,
        ];
        let tags = [TAG_I32, TAG_I64, TAG_F64, TAG_BOOL, TAG_CHAR, TAG_STR, TAG_OBJECT, TAG_ARRAY];
        for v in &values {
            for &t in &tags {
                let verdict = hard_cast_failure(v, t);
                if verdict.is_none() {
                    // 说明「这是合法硬转换」⇒ convert_value 不得走到防御性 bail。
                    // （数值越界 / 非法 Unicode 标量等**值域**错误仍可能 Err，那是另一类，
                    // 消息不含 "internal:"。）
                    if let Err(e) = convert_value(v.clone(), t) {
                        let msg = format!("{:#}", e);
                        assert!(!msg.contains("internal:"),
                            "hard_cast_failure 放行了 {:?} → tag 0x{:02X}，但 convert_value 走到防御性 bail：{}",
                            v, t, msg);
                    }
                }
            }
        }
    }

    #[test]
    fn null_to_value_type_is_null_reference_not_invalid_cast() {
        // 「没有对象」与「对象类型不对」是两种不同的错，不能合成一条。
        let (exc, msg) = hard_cast_failure(&Value::Null, TAG_I32).expect("null→int 必须失败");
        assert_eq!(exc, NULL_REF_EXC);
        assert!(msg.contains("int"), "消息要点出目标类型：{msg}");
    }

    #[test]
    fn null_to_reference_type_is_allowed() {
        // C# 同：null 可以转成任何引用类型。误抛这条会打断 `(string)someNullObj`。
        assert!(hard_cast_failure(&Value::Null, TAG_STR).is_none());
        assert!(hard_cast_failure(&Value::Null, TAG_OBJECT).is_none());
    }

    #[test]
    fn wrong_kind_to_value_type_is_invalid_cast_with_readable_message() {
        let (exc, msg) = hard_cast_failure(&Value::Str("hello".into()), TAG_I32)
            .expect("string→int 必须失败");
        assert_eq!(exc, INVALID_CAST_EXC);
        // 🔒 消息不得是 Rust Debug 格式（此前是 `Str("hello")` / `type tag 0x04`）。
        assert!(!msg.contains("Str("), "消息漏了 Rust Debug 格式：{msg}");
        assert!(!msg.contains("tag 0x"), "消息漏了 tag 数字：{msg}");
        assert!(msg.contains("int"), "消息要点出目标类型：{msg}");
    }

    #[test]
    fn legit_numeric_and_identity_casts_are_allowed() {
        assert!(hard_cast_failure(&Value::I64(1), TAG_F64).is_none());
        assert!(hard_cast_failure(&Value::F64(3.7), TAG_I32).is_none());
        assert!(hard_cast_failure(&Value::Bool(true), TAG_BOOL).is_none());
        assert!(hard_cast_failure(&Value::Str("s".into()), TAG_STR).is_none());
    }
}
