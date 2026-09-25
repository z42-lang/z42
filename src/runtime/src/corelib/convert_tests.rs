use super::*;
use crate::metadata::Value;
use crate::vm_context::VmContext;

fn ctx() -> std::pin::Pin<Box<VmContext>> { VmContext::new() }

fn parse(builtin: fn(&VmContext, &[Value]) -> anyhow::Result<Value>, s: &str)
    -> anyhow::Result<i64>
{
    match builtin(&ctx(), &[Value::Str(s.to_string().into())])? {
        Value::I64(n) => Ok(n),
        other => panic!("expected I64, got {:?}", other),
    }
}

#[test]
fn int_parse_accepts_i32_range() {
    assert_eq!(parse(builtin_int32_parse, "0").unwrap(), 0);
    assert_eq!(parse(builtin_int32_parse, "-2147483648").unwrap(), i32::MIN as i64);
    assert_eq!(parse(builtin_int32_parse, "2147483647").unwrap(), i32::MAX as i64);
}

#[test]
fn int_parse_rejects_out_of_range() {
    assert!(parse(builtin_int32_parse, "2147483648").is_err());
    assert!(parse(builtin_int32_parse, "-2147483649").is_err());
}

#[test]
fn i8_parse_range() {
    assert_eq!(parse(builtin_sbyte_parse, "-128").unwrap(), -128);
    assert_eq!(parse(builtin_sbyte_parse, "127").unwrap(), 127);
    assert!(parse(builtin_sbyte_parse, "128").is_err());
    assert!(parse(builtin_sbyte_parse, "-129").is_err());
}

#[test]
fn i16_parse_range() {
    assert_eq!(parse(builtin_int16_parse, "-32768").unwrap(), -32768);
    assert_eq!(parse(builtin_int16_parse, "32767").unwrap(), 32767);
    assert!(parse(builtin_int16_parse, "32768").is_err());
}

#[test]
fn u8_parse_range() {
    assert_eq!(parse(builtin_byte_parse, "0").unwrap(), 0);
    assert_eq!(parse(builtin_byte_parse, "255").unwrap(), 255);
    assert!(parse(builtin_byte_parse, "256").is_err());
    assert!(parse(builtin_byte_parse, "-1").is_err());
}

#[test]
fn u16_parse_range() {
    assert_eq!(parse(builtin_uint16_parse, "65535").unwrap(), 65535);
    assert!(parse(builtin_uint16_parse, "65536").is_err());
    assert!(parse(builtin_uint16_parse, "-1").is_err());
}

#[test]
fn u32_parse_range() {
    assert_eq!(parse(builtin_uint32_parse, "4294967295").unwrap(), u32::MAX as i64);
    assert!(parse(builtin_uint32_parse, "4294967296").is_err());
    assert!(parse(builtin_uint32_parse, "-1").is_err());
}

#[test]
fn u64_parse_preserves_bits_above_i64_max() {
    // u64::MAX = 0xFFFF_FFFF_FFFF_FFFF — bit-cast to i64 → -1
    assert_eq!(parse(builtin_uint64_parse, "18446744073709551615").unwrap(), -1);
    // i64::MAX + 1 → bit-cast to i64 → i64::MIN
    assert_eq!(parse(builtin_uint64_parse, "9223372036854775808").unwrap(), i64::MIN);
    // values within i64::MAX round-trip unchanged
    assert_eq!(parse(builtin_uint64_parse, "12345").unwrap(), 12345);
}

#[test]
fn u64_parse_rejects_negative_and_overflow() {
    assert!(parse(builtin_uint64_parse, "-1").is_err());
    assert!(parse(builtin_uint64_parse, "18446744073709551616").is_err());
}

#[test]
fn parse_rejects_non_numeric() {
    assert!(parse(builtin_int32_parse, "abc").is_err());
    assert!(parse(builtin_byte_parse, "12x").is_err());
    assert!(parse(builtin_uint64_parse, "").is_err());
}

#[test]
fn parse_trims_whitespace() {
    assert_eq!(parse(builtin_int32_parse, "  42 ").unwrap(), 42);
    assert_eq!(parse(builtin_uint16_parse, "\t100\n").unwrap(), 100);
}

// ── __box_prim 的 Null 分支（值类型槽的不变式）──────────────────────────────
//
// 「值类型的存储槽永不含 `Value::Null`」（enforce-value-type-non-null）。装箱点收到 Null
// 只可能是 VM / 编译器缺陷，所以 debug 构建报错、release 放行。
//
// ⚠️ 这两条是这道门的**正面对照**：全量摸底对它是零命中（现有语料没有一处踩到），
// 而「零命中」既不能证明规则对、也不能证明这门真的会响 —— 只有直接喂一个 Null 才能分辨。
// 少了它，这里就可能交付一道**恒不响的门**。

#[test]
#[cfg(debug_assertions)]
fn box_prim_null_is_an_error_in_debug() {
    let e = builtin_box_prim(&ctx(), &[Value::Null, Value::Str("Std.Int32".into())])
        .expect_err("debug 构建必须把「装箱 Null」报成错误");
    let msg = format!("{e}");
    // 消息要把责任方说清楚：这是 VM 缺陷，不是调用方的 z42 代码写错了。
    assert!(msg.contains("__box_prim received Null"), "msg = {msg}");
    assert!(msg.contains("不变式"), "msg = {msg}");
}

#[test]
#[cfg(not(debug_assertions))]
fn box_prim_null_passes_through_in_release() {
    // release 行为与 #717 之后一字不变：不拿用户的崩溃换我们的诊断能力。
    let v = builtin_box_prim(&ctx(), &[Value::Null, Value::Str("Std.Int32".into())]).unwrap();
    assert!(matches!(v, Value::Null));
}

#[test]
fn box_prim_does_not_intercept_a_real_integer() {
    // 🔒 真整数不能被上面那道 Null 门拦住 —— 否则「加一道诊断」会顺手废掉好路径。
    //
    // 这个裸 `VmContext` 没有类型注册表，所以 `Std.Int32` 这个 wrapper 查不到，装箱最终仍会失败
    // ——但**失败在后面那一步**，错误文本证明它已经走过了 Null 分支。真正的装箱成功路径由整个
    // golden 语料端到端覆盖（`object o = 42` 到处都是），不在这个脚手架里重造。
    let e = builtin_box_prim(&ctx(), &[Value::I64(42), Value::Str("Std.Int32".into())])
        .expect_err("裸 VmContext 没有 wrapper 类型，这里预期失败在 wrapper 查找上");
    let msg = format!("{e}");
    assert!(msg.contains("unknown prim wrapper type"), "msg = {msg}");
    assert!(!msg.contains("received Null"), "真整数被 Null 门拦住了：msg = {msg}");
}
