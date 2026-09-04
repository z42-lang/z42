use crate::metadata::Value;
use crate::metadata::types::NativeData;
use crate::metadata::well_known_names::int_wrapper_scalar_spec;
use crate::vm_context::VmContext;
use anyhow::{bail, Result};

/// add-primitive-value-boxing → unify Phase 2 R3（装箱统一）：把裸整数基元装箱成**堆
/// `ScriptObject` + 引用身份**（对齐 C#），返 `Value::BoxedStruct`——与 struct 装箱同一模型。
/// 编译器在 prim→object/接口 转换点发 `builtin __box_prim(%value, %classStr)`；arg0 = 裸整数值
/// （`__box_prim` 只装整数，bool/char/double/string 各留 `Value` variant），arg1 = FQ wrapper 名
/// （`Std.Int32`/`Std.Byte`/…）。已是 BoxedStruct 则原样返（幂等）。每次装箱 alloc 新盒
/// → `object o=5; object p=5; ReferenceEquals(o,p)==false`（C# 语义）。
pub fn builtin_box_prim(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let inner = match args.first() {
        Some(v) => v,
        None => bail!("__box_prim: missing value arg"),
    };
    // 幂等：已装箱（基元盒是 BoxedStruct）→ 原样返回，不重复 alloc。
    if matches!(inner, Value::BoxedStruct(_)) {
        return Ok(inner.clone());
    }
    let raw = match inner {
        Value::I64(n) => *n,
        other => bail!("__box_prim: expected integer value, got {:?}", other),
    };
    let class = match args.get(1) {
        Some(Value::Str(s)) => s,
        _ => bail!("__box_prim: missing/invalid class-name arg"),
    };
    box_prim_to_heap(ctx, class, raw)
}

/// unify Phase 2 R3：把整数标量装箱进堆 `ScriptObject`（D1-B：标量 LE 字节存 `struct_bytes`，
/// 尺寸 = wrapper 标量宽度，与 struct 装箱完全同构）→ `Value::BoxedStruct`。`class` = FQ wrapper
/// 名，`raw` = 裸 i64。宽度未知（不该发生）→ 全 8 字节 signed 兜底（仍能 round-trip i64）。
/// 无引用叶子（整数）→ `struct_refs`/`slots` 空。
pub(crate) fn box_prim_to_heap(ctx: &VmContext, class: &str, raw: i64) -> Result<Value> {
    let td = ctx.try_lookup_type(class).ok_or_else(|| {
        anyhow::anyhow!("__box_prim: unknown prim wrapper type `{class}`")
    })?;
    let (width, _signed) = int_wrapper_scalar_spec(&td.name).unwrap_or((8, true));
    let struct_bytes: Box<[u8]> = raw.to_le_bytes()[..width].to_vec().into_boxed_slice();
    match ctx.heap().alloc_boxed_prim(td, struct_bytes) {
        Value::Object(gc) => Ok(Value::BoxedStruct(gc)),
        other => Ok(other), // Null under strict-OOM refusal
    }
}

/// add-struct-object-boxing (PR2a): 把 blob 值 struct 装箱成堆 `Value::BoxedStruct`——从 arena slot 拷出
/// `bytes` 快照 + clone `refs` + 类型名（脱离帧生命周期，修裸拷 StructRef 逃逸帧的 use-after-free）。
/// 编译器在 struct→object/接口 转换点发 `builtin __box_struct(%structHandle)`（arg0=StructRef，装箱点
/// struct 活、slot 有效）。类型名从 slot 直接取（FQ，`StructAlloc` 时写入），无需额外 class 参数。
/// 已是 BoxedStruct 幂等返回；非 struct 值原样返回（保守——`BoxIfNeeded` 只对 blob struct 发本 builtin）。
pub fn builtin_box_struct(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let v = match args.first() {
        Some(v) => v,
        None => bail!("__box_struct: missing value arg"),
    };
    match v {
        // add-boxed-struct-identity (P4b): already a shared box → return the SAME box
        // (reference identity preserved; re-boxing must not clone the heap object).
        Value::BoxedStruct(_) => Ok(v.clone()),
        Value::StructRef { idx, frame_id } => {
            let (type_name, bytes, refs) = ctx.struct_arena.lock()
                .with(*idx, *frame_id, |s| (s.type_name.clone(), s.bytes.clone(), s.refs.clone()))?;
            box_struct_blob(ctx, &type_name, &bytes, &refs)
        }
        other => Ok(other.clone()),
    }
}

/// add-boxed-struct-identity (P4b, 路 B2): box a value-struct blob into a **shared**
/// `ScriptObject` (`type_desc` = the struct type, `is_struct()` true), storing the
/// primitive leaves in the object's `struct_bytes` and the reference leaves in
/// `struct_refs` (`alloc_object` pre-sizes both via `inline_region_sizes`, which reads
/// the struct's own `struct_layout` for struct-typed objects). Returns a
/// `Value::BoxedStruct(gc)` — a shared, GC-managed, reference-identity box (C# semantics:
/// `object b = a` aliases the same box, reflective `SetValue` writes through).
///
/// Reused by `__box_struct` (top-level boxing) and reflection's nested-field GetValue
/// (a nested struct field materializes as a fresh boxed snapshot).
pub(crate) fn box_struct_blob(
    ctx: &VmContext, type_name: &str, bytes: &[u8], refs: &[Value],
) -> Result<Value> {
    let td = ctx.try_lookup_type(type_name).ok_or_else(|| {
        anyhow::anyhow!("__box_struct: unknown struct type `{type_name}`")
    })?;
    let obj_val = ctx.heap().alloc_object(td, Vec::new(), NativeData::None);
    match obj_val {
        Value::Object(gc) => {
            {
                let mut o = gc.borrow_mut();
                let n = bytes.len().min(o.bytes().len());
                o.bytes_mut()[..n].copy_from_slice(&bytes[..n]);
                let rn = refs.len().min(o.refs().len());
                o.refs_mut()[..rn].clone_from_slice(&refs[..rn]);
            }
            Ok(Value::BoxedStruct(gc))
        }
        // alloc_object returns Null under strict-OOM refusal.
        other => Ok(other),
    }
}

/// add-struct-object-methods (PR2b): boxed 值 struct 的 GetHashCode——FNV-1a over 基元字节 blob + 混入
/// 引用叶子哈希（string 走内容 FNV；object/array 叶子暂贡献常量 = 弱但合法，因 Equals 对引用叶子按引用比较，
/// 不同引用→不 Equals→哈希可碰撞）。结果 `& 0x7fffffff` 非负（`__str_hash_code` 同款，Dictionary 契约）。
/// 值相等的 struct → 字节+refs 相同 → 同哈希（契约满足）。⚠️边角：float ±0.0 字节不同→哈希不同，而 Equals
/// 的浮点 == 判 +0==-0 → 极少数含 ±0 float 的 struct 违反契约（pre-1.0 文档标注，与 C# 历史一致）。
pub fn builtin_struct_hash_code(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let gc = match args.first() {
        Some(Value::BoxedStruct(gc)) => gc,
        _ => bail!("__struct_hash_code: expected a boxed struct"),
    };
    // add-boxed-struct-identity (P4b): read the blob out of the shared box object.
    let b = gc.borrow();
    let mut h: u32 = 2_166_136_261;
    for &byte in b.bytes().iter() { h ^= byte as u32; h = h.wrapping_mul(16_777_619); }
    for r in b.refs().iter() {
        let rh: u32 = match r {
            Value::Str(s) => {
                let mut sh: u32 = 2_166_136_261;
                for &x in s.as_bytes() { sh ^= x as u32; sh = sh.wrapping_mul(16_777_619); }
                sh
            }
            _ => 0,   // object/array/null 叶子：弱贡献（引用相等语义下 collisions 合法）
        };
        h ^= rh; h = h.wrapping_mul(16_777_619);
    }
    Ok(Value::I64((h & 0x7fff_ffff) as i64))
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
        // add-primitive-value-boxing → unify Phase 2 R3: 装箱整数实参透明拆箱。call-arg 装箱把整数
        // 实参装箱成 object（如 Assert.Equal(object,object)），而基元 struct 方法的 native（Equals/
        // CompareTo/算术）按裸 long 签名读参 → 装箱值须在此拆回 i64。装箱后整数盒是 `BoxedStruct`
        // （堆 ScriptObject，标量存 struct_bytes），非整数盒（多字段 struct）→ unbox_prim_i64 返 None。
        Some(Value::BoxedStruct(gc)) => match gc.borrow().boxed_prim_i64() {
            Some(n) => Ok(n),
            None => bail!("{}: arg {} expected int, got a non-prim boxed struct", ctx, idx),
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
        Value::FuncRef(name) => format!("<fn {name}>"),
        Value::Closure(c)                   => format!("<closure {}>", crate::metadata::types::closure_data_of(c).fn_name),
        // make-value-copy: PinnedView / StackClosure / Ref are transient-arena handles
        // and `value_to_str` has no `ctx` to resolve the arena payload — placeholders
        // (same as StackObject / StructRef below). These never reach the user-visible
        // stringify path (ToString is an escape sink → the heap form is what materializes).
        Value::PinnedView { .. } => "<pinned view>".to_string(),
        Value::StackClosure { .. } => "<closure>".to_string(),
        Value::Ref { .. } => "<ref>".to_string(),
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
        // add-struct-object-boxing → unify Phase 2 R3: 基元装箱盒（int→Std.Int32…）→ 标量字符串
        // （恢复 add-primitive-value-boxing 的 `value_to_str(inner)` 语义——`WriteLine(object)` 把
        // int 实参装箱后须打印 "5" 而非 "Std.Int32{...}"）。boxed struct 的完整 ToString（值格式）由
        // PR2b 合成方法经 VCall 提供；此原始路径给类型名占位（比 StructRef 占位更具体）。
        Value::BoxedStruct(gc) => match gc.borrow().boxed_prim_i64() {
            Some(n) => n.to_string(),
            None => format!("{}{{...}}", gc.type_desc().name),
        },
        // make-value-copy: a struct[] element handle — arena-resident, no ctx here;
        // placeholder (ToString on the element dispatches via VCall, not this raw path).
        Value::StructRefHeap { .. } => "<struct value>".to_string(),
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

// shrink-primitive-native-interop (2026-08-27): builtin_int32_equals /
// builtin_int32_hash_code removed — Std.{Int32,Int16,SByte,Byte,UInt16,UInt32,
// Int64,UInt64}.Equals/GetHashCode 现在是脚本（`this == other` / `(int)this`；
// 64 位类型折叠高低字 `(int)(v ^ (v >> 32))`）。
pub fn builtin_int32_to_string(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let a = arg_i64(args, 0, "Int32.ToString")?;
    Ok(Value::Str(a.to_string().into()))
}

// 2026-04-27 wave2-compare-to-script: builtin_double_compare_to removed.
// `Std.Double.CompareTo` / `Std.Single.CompareTo` 现在是脚本（NaN → 0 由 `<`/`>` 自然返回 false 实现）。

// shrink-primitive-native-interop (2026-08-27): builtin_double_equals /
// builtin_double_hash_code removed — Std.{Double,Single}.Equals/GetHashCode 现在
// 是脚本（`this == other`；hash 经 BitConverter 折叠 IEEE-754 位模式）。
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

// shrink-primitive-native-interop (2026-08-27): builtin_char_equals /
// builtin_char_hash_code removed — Std.Char.Equals/GetHashCode 现在是脚本。
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
