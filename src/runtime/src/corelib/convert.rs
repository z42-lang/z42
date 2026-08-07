use crate::metadata::Value;
use crate::metadata::types::BoxedPrim;
use crate::vm_context::VmContext;
use anyhow::{bail, Result};

/// add-primitive-value-boxing: 把裸基元装箱成 `Value::Boxed`，携带其精确基元 struct 类名。
/// 编译器在 prim→object/接口 转换点发 `builtin __box_prim(%value, %classStr)`。
/// arg0 = 裸基元值；arg1 = FQ 基元 struct 名（`Std.Int64`/…）。已是 Boxed 则原样返（幂等）。
pub fn builtin_box_prim(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let inner = match args.first() {
        Some(v) => v.clone(),
        None => bail!("__box_prim: missing value arg"),
    };
    if matches!(inner, Value::Boxed(_)) {
        return Ok(inner);
    }
    let class = match args.get(1) {
        Some(Value::Str(s)) => s.clone(),
        _ => bail!("__box_prim: missing/invalid class-name arg"),
    };
    Ok(Value::Boxed(Box::new(BoxedPrim { class, inner })))
}

// ── Typed argument extractors ────────────────────────────────────────────────
//
// refactor-corelib-typed-extractors (2026-05-17): direct-ABI 优化的第一阶段。
// 每个 builtin 在 dispatch 边界拿到 `&[Value]` 后会 extract typed args；旧的
// `require_str` 每次都 `s.clone()` 一份 `String`，对 `__str_length` /
// `__str_equals` 这种纯只读 ops 是显著开销。
//
// `arg_*` 系列：
//   * 返回 `&str` / `i64` / `bool` / `char` / `f64` / `usize` — 全部 borrow 或 Copy
//   * `#[inline]` 让编译器把 match 内联到 caller，消除函数调用开销
//   * 错误格式与旧 `require_*` 一致
//
// 所有 corelib 已 migrate 完，旧 `require_*` 已删（pre-1.0 不留兼容包袱）。

#[inline]
pub fn arg_str<'a>(args: &'a [Value], idx: usize, ctx: &str) -> Result<&'a str> {
    match args.get(idx) {
        Some(Value::Str(s)) => Ok(&s),
        Some(other) => bail!("{}: arg {} expected string, got {:?}", ctx, idx, other),
        None => bail!("{}: missing arg {}", ctx, idx),
    }
}

#[inline]
pub fn arg_i64(args: &[Value], idx: usize, ctx: &str) -> Result<i64> {
    match args.get(idx) {
        Some(Value::I64(n)) => Ok(*n),
        // add-primitive-value-boxing: 装箱整数实参透明拆箱。call-arg 装箱把整数实参装箱成
        // object（如 Assert.Equal(object,object)），而基元 struct 方法的 native（Equals/CompareTo/
        // 算术）按裸 long 签名读参 → 装箱值须在此拆回内层 I64。Boxed 恒包整数（_intPrimFQ 只装箱整数）。
        Some(Value::Boxed(b)) => match &b.inner {
            Value::I64(n) => Ok(*n),
            other => bail!("{}: arg {} expected int, got Boxed({:?})", ctx, idx, other),
        },
        Some(other) => bail!("{}: arg {} expected int, got {:?}", ctx, idx, other),
        None => bail!("{}: missing arg {}", ctx, idx),
    }
}

#[inline]
pub fn arg_f64(args: &[Value], idx: usize, ctx: &str) -> Result<f64> {
    match args.get(idx) {
        Some(Value::F64(f)) => Ok(*f),
        Some(Value::I64(n)) => Ok(*n as f64),
        Some(other) => bail!("{}: arg {} expected double, got {:?}", ctx, idx, other),
        None => bail!("{}: missing arg {}", ctx, idx),
    }
}

#[inline]
pub fn arg_bool(args: &[Value], idx: usize, ctx: &str) -> Result<bool> {
    match args.get(idx) {
        Some(Value::Bool(b)) => Ok(*b),
        Some(other) => bail!("{}: arg {} expected bool, got {:?}", ctx, idx, other),
        None => bail!("{}: missing arg {}", ctx, idx),
    }
}

#[inline]
pub fn arg_char(args: &[Value], idx: usize, ctx: &str) -> Result<char> {
    match args.get(idx) {
        Some(Value::Char(c)) => Ok(*c),
        Some(other) => bail!("{}: arg {} expected char, got {:?}", ctx, idx, other),
        None => bail!("{}: missing arg {}", ctx, idx),
    }
}

#[inline]
pub fn arg_usize(args: &[Value], idx: usize, ctx: &str) -> Result<usize> {
    match args.get(idx) {
        Some(Value::I64(n)) if *n >= 0 => Ok(*n as usize),
        Some(other) => bail!("{}: arg {} expected non-negative integer, got {:?}", ctx, idx, other),
        None => bail!("{}: missing arg {}", ctx, idx),
    }
}

/// Convert a Value to its string representation.
///
/// Exhaustive match: 加新 `Value` variant 时编译期强制覆盖（防止再次出现
/// 像 `Value::Map` 那样"variant 加进 enum 但消费侧忘记更新"的死代码）。
pub fn value_to_str(v: &Value) -> String {
    match v {
        Value::I64(n)  => n.to_string(),
        Value::F64(f)  => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Char(c) => c.to_string(),
        Value::Str(s)  => s.to_string(),
        Value::Null    => "null".to_string(),
        Value::Array(rc) => {
            let inner: Vec<String> = rc.borrow().iter_boxed().map(|v| value_to_str(&v)).collect();
            format!("[{}]", inner.join(", "))
        }
        Value::Object(rc) => format!("{}{{...}}", rc.type_desc().name),
        Value::PinnedView(pv) => {
            format!("PinnedView{{ptr=0x{:x}, len={}, kind={:?}}}", pv.ptr, pv.len, pv.kind)
        }
        Value::FuncRef(name) => format!("<fn {name}>"),
        Value::Closure(c)                   => format!("<closure {}>", c.fn_name),
        Value::StackClosure(sc) => format!("<closure {}>", sc.fn_name),
        // Spec impl-ref-out-in-runtime: Refs 应该在 frame.get/set 阶段透明
        // deref，不应到达 user-visible 字符串化路径。如果出现，说明代码漏了
        // 一处 deref —— 用占位字串避免 panic，但调试时容易识别。
        Value::Ref(_) => "<ref>".to_string(),
        // add-primitive-value-boxing: 装箱基元字符串化 = inner 的字符串。
        Value::Boxed(b) => value_to_str(&b.inner),
        // add-escape-analysis-stack-alloc: stack objects/arrays never reach the
        // user-visible stringify path (ToStr is an escape sink → such objects are
        // heap-allocated, not stack). This arm is defensive: a placeholder that's
        // easy to spot in debugging if an analysis bug ever lets one through
        // (`value_to_str` has no `ctx` to resolve the arena, by design).
        Value::StackObject { .. } => "<stack object>".to_string(),
        Value::StackArray { .. }  => "<stack array>".to_string(),
        // add-struct-value-semantics: struct blob lives in the arena; value_to_str
        // has no ctx to resolve it. ToString on a value struct dispatches through
        // its type's method (VCall), not this raw path — defensive placeholder.
        Value::StructRef { .. } => "<struct value>".to_string(),
    }
}

// refactor-corelib-typed-extractors (2026-05-17): 旧的 `require_str` /
// `require_usize` / `to_usize` / `require_i64` / `require_f64` / `require_char`
// 全部删除 —— 全 corelib 已 migrated 到 `arg_*` 系列（零 clone / Copy / #[inline]）。
// pre-1.0 不留兼容包袱。

// ── Parse / convert builtins ─────────────────────────────────────────────────
//
// rename-primitives-to-pascal-case (2026-05-24): builtin functions + error
// messages now use BCL PascalCase (`Int32.Parse` instead of `int.Parse`).
// Source keyword (`int / long / i8 / u8 / ...`) remains valid in user code
// via C# TypeChecker alias; this Rust layer is the underlying implementation.

pub fn builtin_int64_parse(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let s = arg_str(args, 0, "Int64.Parse")?;
    s.trim().parse::<i64>().map(Value::I64)
        .map_err(|_| anyhow::anyhow!("Int64.Parse: could not parse {:?} as Int64", s))
}

/// add-narrow-int-primitives (2026-05-15): parse the input as an i64, then
/// validate that the value fits in the target's range. Out-of-range values
/// throw OverflowException-style error (anyhow string surfaced as VM bail).
/// Pre-2026-05-15 `int.Parse("99999999999999")` silently succeeded with a
/// value larger than i32 could hold; this build now rejects it.
pub fn builtin_int32_parse(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    parse_narrow_int(args, "Int32.Parse", i32::MIN as i64, i32::MAX as i64)
}
pub fn builtin_sbyte_parse(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    parse_narrow_int(args, "SByte.Parse", i8::MIN as i64, i8::MAX as i64)
}
pub fn builtin_int16_parse(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    parse_narrow_int(args, "Int16.Parse", i16::MIN as i64, i16::MAX as i64)
}
pub fn builtin_byte_parse(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    parse_narrow_int(args, "Byte.Parse", 0, u8::MAX as i64)
}
pub fn builtin_uint16_parse(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    parse_narrow_int(args, "UInt16.Parse", 0, u16::MAX as i64)
}
pub fn builtin_uint32_parse(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    parse_narrow_int(args, "UInt32.Parse", 0, u32::MAX as i64)
}
/// u64 can hold values > i64::MAX. We parse as u64 then bit-cast to i64
/// (i.e. values above i64::MAX appear as negative under Int32.ToString — same
/// bit-preserving semantics as `convert_from_i64` U64 cast).
pub fn builtin_uint64_parse(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let s = arg_str(args, 0, "UInt64.Parse")?;
    s.trim().parse::<u64>().map(|v| Value::I64(v as i64))
        .map_err(|_| anyhow::anyhow!(
            "UInt64.Parse: could not parse {:?} as UInt64 (range: 0..={})", s, u64::MAX))
}

fn parse_narrow_int(args: &[Value], ctx: &str, min: i64, max: i64) -> Result<Value> {
    let s = arg_str(args, 0, ctx)?;
    let v = s.trim().parse::<i64>()
        .map_err(|_| anyhow::anyhow!("{}: could not parse {:?} as integer", ctx, s))?;
    if v < min || v > max {
        bail!("{}: value {} out of range (expected {}..={})", ctx, v, min, max);
    }
    Ok(Value::I64(v))
}
pub fn builtin_double_parse(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let s = arg_str(args, 0, "Double.Parse")?;
    s.trim().parse::<f64>().map(Value::F64)
        .map_err(|_| anyhow::anyhow!("Double.Parse: could not parse {:?} as Double", s))
}
pub fn builtin_to_str(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    Ok(Value::Str(args.first().map(value_to_str).unwrap_or_default().into()))
}

// ── L3-G4b primitive interface implementations ──────────────────────────────
// Backing native functions for IComparable<T> / IEquatable<T> on primitive
// receivers (Int32/Double/Boolean/Char). Dispatched by VCall when the receiver
// is Value::I64/F64/Bool/Char and the method matches CompareTo/Equals/GetHashCode.
// 旧 file-local require_* 已删 —— 用顶部 pub `arg_i64` / `arg_f64` / `arg_char`。

// 2026-04-27 wave2-compare-to-script: builtin_int_compare_to removed.
// `Std.Int32.CompareTo` / `Std.Int64.CompareTo` 现在是脚本（用 IR `<`/`>`）。

pub fn builtin_int32_equals(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let a = arg_i64(args, 0, "Int32.Equals")?;
    let b = arg_i64(args, 1, "Int32.Equals")?;
    Ok(Value::Bool(a == b))
}
pub fn builtin_int32_hash_code(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let a = arg_i64(args, 0, "Int32.GetHashCode")?;
    Ok(Value::I64(a))  // identity hash for integers
}
pub fn builtin_int32_to_string(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let a = arg_i64(args, 0, "Int32.ToString")?;
    Ok(Value::Str(a.to_string().into()))
}

// 2026-04-27 wave2-compare-to-script: builtin_double_compare_to removed.
// `Std.Double.CompareTo` / `Std.Single.CompareTo` 现在是脚本（NaN → 0 由 `<`/`>` 自然返回 false 实现）。

pub fn builtin_double_equals(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let a = arg_f64(args, 0, "Double.Equals")?;
    let b = arg_f64(args, 1, "Double.Equals")?;
    Ok(Value::Bool(a == b))
}
pub fn builtin_double_hash_code(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let a = arg_f64(args, 0, "Double.GetHashCode")?;
    Ok(Value::I64(a.to_bits() as i64))
}
pub fn builtin_double_to_string(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let a = arg_f64(args, 0, "Double.ToString")?;
    Ok(Value::Str(a.to_string().into()))
}

// add-binary-float (2026-06-09): IEEE-754 bit reinterpretation backing
// Std.IO.BinaryReader/Writer float serialization. z42 has no pure-script
// `reinterpret` (the BitConverter gap), so these expose f32/f64 ↔ raw bits.
// `__single_*` round-trips through f32 (4-byte IEEE-754) — the value rides the
// VM as F64, but the on-wire pattern is single precision. The 32-bit pattern is
// carried zero-extended in the i64 low word (BinaryWriter.WriteInt32* writes the
// low 4 bytes); `as u32` on the way back recovers it regardless of sign-extension.
pub fn builtin_single_to_bits(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let a = arg_f64(args, 0, "BitConverter.SingleToBits")?;
    Ok(Value::I64((a as f32).to_bits() as i64))
}
pub fn builtin_single_from_bits(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let bits = arg_i64(args, 0, "BitConverter.SingleFromBits")?;
    Ok(Value::F64(f32::from_bits(bits as u32) as f64))
}
pub fn builtin_double_to_bits(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let a = arg_f64(args, 0, "BitConverter.DoubleToBits")?;
    Ok(Value::I64(a.to_bits() as i64))
}
pub fn builtin_double_from_bits(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let bits = arg_i64(args, 0, "BitConverter.DoubleFromBits")?;
    Ok(Value::F64(f64::from_bits(bits as u64)))
}

// 2026-04-27 wave1-bool-script: 3 `builtin_bool_*` removed.
// `Std.Boolean.Equals` / `GetHashCode` / `ToString` 现在是 z42 脚本实现。

// 2026-04-27 wave2-compare-to-script: builtin_char_compare_to removed.
// `Std.Char.CompareTo` 现在是脚本（codepoint `<`/`>` 比较）。

pub fn builtin_char_equals(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let a = arg_char(args, 0, "Char.Equals")?;
    let b = arg_char(args, 1, "Char.Equals")?;
    Ok(Value::Bool(a == b))
}
pub fn builtin_char_hash_code(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let a = arg_char(args, 0, "Char.GetHashCode")?;
    Ok(Value::I64(a as i64))
}
pub fn builtin_char_to_string(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let a = arg_char(args, 0, "Char.ToString")?;
    Ok(Value::Str(a.to_string().into()))
}

pub fn builtin_str_compare_to(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let a = arg_str(args, 0, "String.CompareTo")?;
    let b = arg_str(args, 1, "String.CompareTo")?;
    Ok(Value::I64(a.cmp(&b) as i64))
}

#[cfg(test)]
#[path = "convert_tests.rs"]
mod convert_tests;
