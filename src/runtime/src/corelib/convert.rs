use crate::metadata::Value;
use crate::metadata::types::NativeData;
use crate::metadata::well_known_names::int_wrapper_scalar_spec;
use crate::vm_context::VmContext;
use anyhow::{bail, Result};
use super::abi;

/// unify Phase 2 R3：把整数标量装箱进堆 `ScriptObject`（D1-B：标量 LE 字节存 `struct_bytes`，
/// 尺寸 = wrapper 标量宽度，与 struct 装箱完全同构）→ `Value::BoxedStruct`。`class` = FQ wrapper
/// 名，`raw` = 裸 i64。宽度未知（不该发生）→ 全 8 字节 signed 兜底（仍能 round-trip i64）。
/// 无引用叶子（整数）→ `struct_refs`/`slots` 空。
pub(crate) fn box_prim_to_heap(ctx: &VmContext, class: &str, raw: i64) -> Result<Value> {
    let td = ctx.try_lookup_type(class).ok_or_else(|| {
        anyhow::anyhow!("__box_prim: unknown prim wrapper type `{class}`")
    })?;
    // make-enum-distinct-type 1.5: enums box through this same path, but with the
    // *enum's own* TypeDesc instead of a `Std.*` wrapper — so `object o = Color.Blue;
    // o.GetType()` yields `Color`, matching the already-correct `typeof(Color)` /
    // `Color.Blue.GetType()` folds. Width is 8/signed because z42 backs every enum
    // with i64 (`z42.core/src/Type.z42` — `GetEnumUnderlyingType()` is always `long`);
    // stated explicitly rather than leaning on the "shouldn't happen" fallback below.
    let is_enum = td.class_flags & crate::metadata::bytecode::CLASS_FLAG_ENUM != 0;
    let (width, _signed) = if is_enum {
        (8, true)
    } else {
        int_wrapper_scalar_spec(&td.name).unwrap_or((8, true))
    };
    let struct_bytes: Box<[u8]> = raw.to_le_bytes()[..width].to_vec().into_boxed_slice();
    match ctx.heap().alloc_boxed_prim(td, struct_bytes) {
        Value::Object(gc) => Ok(Value::BoxedStruct(gc)),
        other => Ok(other), // Null under strict-OOM refusal
    }
}

/// add-boxed-struct-identity (P4b, 路 B2): box a value-struct blob into a **shared**
/// `ScriptObject` (`type_desc` = the struct type, `is_struct()` true), storing the
/// blob in the object's payload — primitive leaves plus 8 B reference words at the struct
/// layout's reference offsets (`struct_blob`; `alloc_object` sizes the payload from the
/// struct's own `struct_layout`). Returns a
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
                let o = gc.borrow();
                let size = o.payload_len();
                // A fresh box (young, unreachable until returned): no barrier owed.
                let offs = o.type_desc.struct_layout().map(|sl| sl.ref_offsets.clone()).unwrap_or_default();
                o.write_blob(0, size, bytes, refs, &offs, &mut |_| {})?;
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
pub(crate) fn struct_hash_code(gc: &crate::gc::ObjRef) -> i64 {
    // add-boxed-struct-identity (P4b): read the blob out of the shared box object.
    // The blob's reference slots hold reference words — hash the bytes with them zeroed.
    let (bytes, refs) = gc.borrow().boxed_blob();
    let mut h: u32 = 2_166_136_261;
    for &byte in bytes.iter() { h ^= byte as u32; h = h.wrapping_mul(16_777_619); }
    for r in refs.iter() {
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
    (h & 0x7fff_ffff) as i64
}

// ── Typed argument extractors ────────────────────────────────────────────────
//
// refactor-corelib-typed-extractors : direct-ABI 优化的第一阶段。
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

//
// null 是**用户能触发**的（null 字符串传进 / 调到 builtin），走 objops 的 `NullReferenceException`，
// 不是内部错误：`arg_str` 报「第 N 个实参是 null」，`this_str` 报「在 null 上调用某成员」
// （接收者，消息与 `VCall` 的 null 接收者同一条）。只在既有的 tag match 失配臂里构造，命中路径不变。

#[inline]
pub fn arg_str<'a>(args: &'a [Value], idx: usize, ctx: &str) -> Result<&'a str> {
    match args.get(idx) {
        Some(Value::Str(s)) => Ok(&s),
        Some(Value::Null) => Err(crate::objops::OpError::null_arg(ctx, idx).into_builtin_error()),
        Some(other) => bail!("{}: arg {} expected string, got {:?}", ctx, idx, other),
        None => bail!("{}: missing arg {}", ctx, idx),
    }
}

/// 字符串**接收者**（`[Native]` 实例成员的 `this`，即 `args[0]`）。`member` 是该 builtin 所实现的
/// `Std.String` 成员在 IR 里的名字（属性访问器写 `get_X`）：null 接收者 →
/// `OpError::null_call(member)`，与 `s.Member` 经 `VCall` 撞上 null 时同一条消息。
#[inline]
pub fn this_str<'a>(args: &'a [Value], member: &str) -> Result<&'a str> {
    match args.first() {
        Some(Value::Str(s)) => Ok(&s),
        Some(Value::Null) => Err(crate::objops::OpError::null_call(member).into_builtin_error()),
        Some(other) => bail!("String.{}: expected a string receiver, got {:?}", member, other),
        None => bail!("String.{}: missing receiver", member),
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

/// perf-str-concat-direct: a stack-first `fmt::Write` sink. Up to 64 bytes stay in the
/// inline buffer (every `i64` / `bool` / `char` and the common `f64` texts); longer
/// output (e.g. `1e300` prints 301 digits) spills to a `String`.
pub struct FmtBuf {
    buf: [u8; 64],
    len: usize,
    spill: String,
}

impl FmtBuf {
    #[inline]
    pub fn new() -> Self {
        Self { buf: [0; 64], len: 0, spill: String::new() }
    }
    #[inline]
    pub fn as_str(&self) -> &str {
        if self.spill.is_empty() {
            // SAFETY: only whole `&str` fragments are ever copied into `buf[..len]`.
            unsafe { std::str::from_utf8_unchecked(&self.buf[..self.len]) }
        } else {
            &self.spill
        }
    }
}

impl Default for FmtBuf {
    fn default() -> Self { Self::new() }
}

impl std::fmt::Write for FmtBuf {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        if self.spill.is_empty() && self.len + s.len() <= self.buf.len() {
            self.buf[self.len..self.len + s.len()].copy_from_slice(s.as_bytes());
            self.len += s.len();
        } else {
            if self.spill.is_empty() {
                // SAFETY: as in `as_str`.
                self.spill.push_str(unsafe { std::str::from_utf8_unchecked(&self.buf[..self.len]) });
            }
            self.spill.push_str(s);
        }
        Ok(())
    }
}

/// perf-str-concat-direct: [`value_to_str`] as a borrowed view — the text is handed to `f`
/// instead of being returned as an owned `String`. Strings pass through, scalars
/// (`I64` / `F64` / `Bool` / `Char` / `Null`) are formatted into a stack [`FmtBuf`]
/// (no heap allocation); every other variant falls back to `value_to_str`. The text is
/// byte-identical to `value_to_str(v)` (same `Display` impls).
#[inline]
pub fn with_value_str<R>(v: &Value, f: impl FnOnce(&str) -> R) -> R {
    use std::fmt::Write;
    let mut b = FmtBuf::new();
    match v {
        Value::Str(s) => return f(s),
        Value::I64(n) => { let _ = write!(b, "{n}"); }
        Value::F64(x) => { let _ = write!(b, "{x}"); }
        Value::Bool(x) => return f(if *x { "true" } else { "false" }),
        Value::Char(c) => { let mut t = [0u8; 4]; return f(c.encode_utf8(&mut t)); }
        Value::Null => return f("null"),
        other => return f(&value_to_str(other)),
    }
    f(b.as_str())
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
            let a = rc.borrow();
            // 值 struct 数组的元素没有 ctx 装不了箱：与装箱 struct 同一个类型名占位。
            let inner: Vec<String> = match a.struct_layout() {
                Some(_) => vec![format!("{}{{...}}", &*a.element_type()); a.len()],
                None => a.iter_boxed().map(|v| value_to_str(&v)).collect(),
            };
            format!("[{}]", inner.join(", "))
        }
        Value::Object(rc) => format!("{}{{...}}", rc.type_desc().name),
        // P1-2 PR 8: callables hold a `FnId`; without a `ctx` the name is not reachable, so
        // the id is shown. The ctx-aware display (`interp::dispatch::with_obj_str`, which
        // string interpolation / `Console.WriteLine` / `ToStr` use) prints the name.
        Value::FuncRef(id) => format!("<fn #{}>", id.0),
        Value::Closure(c)                   => format!("<closure #{}>", crate::metadata::types::closure_data_of(c).fn_id.0),
        // make-value-copy: PinnedView / Ref are transient-arena handles
        // and `value_to_str` has no `ctx` to resolve the arena payload — placeholders
        // (same as StackObject / StructRef below). These never reach the user-visible
        // stringify path (ToString is an escape sink → the heap form is what materializes).
        Value::PinnedView { .. } => "<pinned view>".to_string(),
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
        // make-enum-distinct-type 1.5: an enum box stringifies as its member name
        // (C# `Enum.ToString`) — checked before the scalar arm, which would otherwise
        // print the raw i64. Keeps `Console.WriteLine(Color.Blue)` and
        // `((object)Color.Blue).ToString()` (resolve_vcall) saying the same thing.
        // NB: one `borrow()` for both queries — re-borrowing under the first guard
        // deadlocks (the entry `Mutex` is not reentrant).
        Value::BoxedStruct(gc) => {
            let o = gc.borrow();
            match o.boxed_enum_name() {
                Some(name) => name,
                None => match o.boxed_prim_i64() {
                    Some(n) => n.to_string(),
                    None => format!("{}{{...}}", gc.type_desc().name),
                },
            }
        }
        // make-value-copy: a struct[] element handle — arena-resident, no ctx here;
        // placeholder (ToString on the element dispatches via VCall, not this raw path).
        Value::StructRefHeap { .. } => "<struct value>".to_string(),
    }
}

// refactor-corelib-typed-extractors : 旧的 `require_str` /
// `require_usize` / `to_usize` / `require_i64` / `require_f64` / `require_char`
// 全部删除 —— 全 corelib 已 migrated 到 `arg_*` 系列（零 clone / Copy / #[inline]）。
// pre-1.0 不留兼容包袱。

// ── Parse / format builtins (builtin ABI v2, runtime-audit P1-8) ─────────────
//
// Errors follow .NET: malformed text → `FormatException`, a number outside the target
// type → `OverflowException`, null → `ArgumentNullException`. `TryParse` never throws:
// it writes the value (0 on failure) through its `ref` argument and returns success.

/// Parse `s` as an integer of the type `ty` with range `min..=max` (an unsigned 64-bit
/// value is stored bit-preserving in the `i64`, as `Value::I64` carries every integer).
/// Parsed through `i128`, so "-1" for an unsigned type is an overflow (as in .NET),
/// not a format error.
pub(crate) fn parse_int(s: &str, ty: &str, min: i128, max: i128) -> std::result::Result<i64, abi::BuiltinError> {
    use std::num::IntErrorKind::{NegOverflow, PosOverflow};
    match s.trim().parse::<i128>() {
        Ok(v) if v >= min && v <= max => Ok(v as i64),
        Ok(_) => Err(int_overflow(ty)),
        Err(e) if matches!(e.kind(), PosOverflow | NegOverflow) => Err(int_overflow(ty)),
        Err(_) => Err(not_a_number(ty, s)),
    }
}

#[cold]
fn int_overflow(ty: &str) -> abi::BuiltinError {
    abi::BuiltinError::Overflow(format!("{ty}.Parse: value was either too large or too small for {ty}"))
}

#[cold]
fn not_a_number(ty: &str, s: &str) -> abi::BuiltinError {
    abi::BuiltinError::Format(format!("{ty}.Parse: the input string '{s}' was not in a correct format"))
}

/// Parse `s` as a `Double` (surrounding whitespace allowed; `1e400` is infinity, as in .NET).
pub(crate) fn parse_double(s: &str) -> std::result::Result<f64, abi::BuiltinError> {
    s.trim().parse::<f64>().map_err(|_| not_a_number("Double", s))
}

/// `TryParse` tail: store the value (0 when `s` is null or does not parse) and report success.
fn try_store(ctx: &VmContext, out: abi::RefArg<'_>, parsed: Option<Value>, zero: Value)
    -> std::result::Result<bool, abi::BuiltinError>
{
    let ok = parsed.is_some();
    out.store(ctx, parsed.unwrap_or(zero))?;
    Ok(ok)
}

fn try_int(ctx: &VmContext, s: Option<&str>, out: abi::RefArg<'_>, ty: &str, min: i128, max: i128)
    -> std::result::Result<bool, abi::BuiltinError>
{
    let parsed = s.and_then(|s| parse_int(s, ty, min, max).ok()).map(Value::I64);
    try_store(ctx, out, parsed, Value::I64(0))
}

/// The typed convert builtins. The integer parsers differ only in range; each `[Native]`
/// name maps to one function here.
/// native-hash：MurmurHash3_x86_128、seed 0——逐字面镜像 z42.package 的
/// `Murmur3.z42`（smhasher 参考实现；z42 侧有对拍金向量，两侧哈希值必须一致，等值单测双向盯：
/// 编译器侧 `murmur3.z42` 测试 + 本文件 `mmh3_x86_128_tests`）。供编译器源文本哈希
/// （`ZpkgBuilder.SourceHashHex`）省掉 z42 侧 GetBytes 物化 + 解释执行的哈希内环。
const MMH3_C1: u32 = 0x239B_961B;
const MMH3_C2: u32 = 0xAB0E_9789;
const MMH3_C3: u32 = 0x38B3_4AE5;
const MMH3_C4: u32 = 0xA1E3_8B93;

fn mmh3_blocks(data: &[u8], nblocks: usize, h: &mut [u32; 4]) {
    let rd32 = |o: usize| u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
    for i in 0..nblocks {
        let o = i * 16;
        let k1 = rd32(o).wrapping_mul(MMH3_C1).rotate_left(15).wrapping_mul(MMH3_C2);
        h[0] ^= k1;
        h[0] = h[0].rotate_left(19).wrapping_add(h[1]).wrapping_mul(5).wrapping_add(0x561C_CD1B);
        let k2 = rd32(o + 4).wrapping_mul(MMH3_C2).rotate_left(16).wrapping_mul(MMH3_C3);
        h[1] ^= k2;
        h[1] = h[1].rotate_left(17).wrapping_add(h[2]).wrapping_mul(5).wrapping_add(0x0BCA_A747);
        let k3 = rd32(o + 8).wrapping_mul(MMH3_C3).rotate_left(17).wrapping_mul(MMH3_C4);
        h[2] ^= k3;
        h[2] = h[2].rotate_left(15).wrapping_add(h[3]).wrapping_mul(5).wrapping_add(0x96CD_1C35);
        let k4 = rd32(o + 12).wrapping_mul(MMH3_C4).rotate_left(18).wrapping_mul(MMH3_C1);
        h[3] ^= k4;
        h[3] = h[3].rotate_left(13).wrapping_add(h[0]).wrapping_mul(5).wrapping_add(0x32AC_3B17);
    }
}

/// 尾部（< 16 字节）——smhasher 参考实现的 switch fallthrough 次序。
fn mmh3_tail(data: &[u8], t: usize, tl: usize, h: &mut [u32; 4]) {
    let byte = |i: usize| data[t + i];
    let (mut q1, mut q2, mut q3, mut q4) = (0u32, 0u32, 0u32, 0u32);
    if tl >= 15 { q4 ^= (byte(14) as u32) << 16; }
    if tl >= 14 { q4 ^= (byte(13) as u32) << 8; }
    if tl >= 13 {
        q4 ^= byte(12) as u32;
        h[3] ^= q4.wrapping_mul(MMH3_C4).rotate_left(18).wrapping_mul(MMH3_C1);
    }
    if tl >= 12 { q3 ^= (byte(11) as u32) << 24; }
    if tl >= 11 { q3 ^= (byte(10) as u32) << 16; }
    if tl >= 10 { q3 ^= (byte(9) as u32) << 8; }
    if tl >= 9 {
        q3 ^= byte(8) as u32;
        h[2] ^= q3.wrapping_mul(MMH3_C3).rotate_left(17).wrapping_mul(MMH3_C4);
    }
    if tl >= 8 { q2 ^= (byte(7) as u32) << 24; }
    if tl >= 7 { q2 ^= (byte(6) as u32) << 16; }
    if tl >= 6 { q2 ^= (byte(5) as u32) << 8; }
    if tl >= 5 {
        q2 ^= byte(4) as u32;
        h[1] ^= q2.wrapping_mul(MMH3_C2).rotate_left(16).wrapping_mul(MMH3_C3);
    }
    if tl >= 4 { q1 ^= (byte(3) as u32) << 24; }
    if tl >= 3 { q1 ^= (byte(2) as u32) << 16; }
    if tl >= 2 { q1 ^= (byte(1) as u32) << 8; }
    if tl >= 1 {
        q1 ^= byte(0) as u32;
        h[0] ^= q1.wrapping_mul(MMH3_C1).rotate_left(15).wrapping_mul(MMH3_C2);
    }
}

fn mmh3_x86_128(data: &[u8]) -> [u8; 16] {
    let mut h = [0u32; 4];
    let n = data.len();
    let nblocks = n / 16;
    mmh3_blocks(data, nblocks, &mut h);
    mmh3_tail(data, nblocks * 16, n - nblocks * 16, &mut h);
    let (mut h1, mut h2, mut h3, mut h4) = (h[0], h[1], h[2], h[3]);
    let ln = n as u32;
    h1 ^= ln; h2 ^= ln; h3 ^= ln; h4 ^= ln;
    h1 = h1.wrapping_add(h2).wrapping_add(h3).wrapping_add(h4);
    h2 = h2.wrapping_add(h1); h3 = h3.wrapping_add(h1); h4 = h4.wrapping_add(h1);
    let fmix = |mut x: u32| {
        x ^= x >> 16; x = x.wrapping_mul(0x85EB_CA6B);
        x ^= x >> 13; x = x.wrapping_mul(0xC2B2_AE35);
        x ^= x >> 16;
        x
    };
    h1 = fmix(h1); h2 = fmix(h2); h3 = fmix(h3); h4 = fmix(h4);
    h1 = h1.wrapping_add(h2).wrapping_add(h3).wrapping_add(h4);
    h2 = h2.wrapping_add(h1); h3 = h3.wrapping_add(h1); h4 = h4.wrapping_add(h1);
    let mut out = [0u8; 16];
    out[0..4].copy_from_slice(&h1.to_le_bytes());
    out[4..8].copy_from_slice(&h2.to_le_bytes());
    out[8..12].copy_from_slice(&h3.to_le_bytes());
    out[12..16].copy_from_slice(&h4.to_le_bytes());
    out
}

fn hex_str(bytes: &[u8]) -> String {
    const D: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(D[(b >> 4) as usize] as char);
        s.push(D[(b & 0xF) as usize] as char);
    }
    s
}

#[z42_macros::builtins]
pub mod typed {
    use super::{parse_double, parse_int, try_int, try_store, value_to_str};
    use crate::corelib::abi::{BuiltinError, RefArg};
    use crate::metadata::Value;
    use crate::vm_context::VmContext;

    /// native-hash：字符串（按其 UTF-8 字节）→ MurmurHash3_x86_128 的 32 个小写 hex 字符。
    /// 等价于 z42 侧 `Murmur3.Hash128Hex(Utf8.GetBytes(s))`（对拍见上）。
    #[builtin(name = "__mmh3_hex_str", pure)]
    pub fn mmh3_hex_str(text: &str) -> Result<String, BuiltinError> {
        Ok(super::hex_str(&super::mmh3_x86_128(text.as_bytes())))
    }

    /// native-hash：字符串（按其 UTF-8 字节）→ 小写 hex 编码（每字节 2 字符）。
    /// 等价于 z42c.pipeline `CacheStore.HexEnc` 的输出形态。
    #[builtin(name = "__hex_str", pure)]
    pub fn hex_str_builtin(text: &str) -> Result<String, BuiltinError> {
        Ok(super::hex_str(text.as_bytes()))
    }

    /// native-hash（bytes 扩展）：byte[] → MurmurHash3_x86_128 的 32 个小写 hex。等价于 z42 侧
    /// `Murmur3.Hash128Hex(bytes)`——cache zbc 的内容哈希（写端 _writeCacheZbc / 读端配对校验）
    /// 与 DepIdentity 的字节依赖指纹共用；消费方全在 z42 侧（Rust 只读比较、从不重算）。
    #[builtin(name = "__mmh3_hex_bytes", pure)]
    pub fn mmh3_hex_bytes(data: Vec<u8>) -> Result<String, BuiltinError> {
        Ok(super::hex_str(&super::mmh3_x86_128(&data)))
    }

    #[builtin(name = "__int64_parse", pure)]
    pub fn int64_parse(s: &str) -> Result<i64, BuiltinError> { parse_int(s, "Int64", i64::MIN as i128, i64::MAX as i128) }
    #[builtin(name = "__int32_parse", pure)]
    pub fn int32_parse(s: &str) -> Result<i64, BuiltinError> { parse_int(s, "Int32", i32::MIN as i128, i32::MAX as i128) }
    #[builtin(name = "__int16_parse", pure)]
    pub fn int16_parse(s: &str) -> Result<i64, BuiltinError> { parse_int(s, "Int16", i16::MIN as i128, i16::MAX as i128) }
    #[builtin(name = "__sbyte_parse", pure)]
    pub fn sbyte_parse(s: &str) -> Result<i64, BuiltinError> { parse_int(s, "SByte", i8::MIN as i128, i8::MAX as i128) }
    #[builtin(name = "__byte_parse", pure)]
    pub fn byte_parse(s: &str) -> Result<i64, BuiltinError> { parse_int(s, "Byte", 0, u8::MAX as i128) }
    #[builtin(name = "__uint16_parse", pure)]
    pub fn uint16_parse(s: &str) -> Result<i64, BuiltinError> { parse_int(s, "UInt16", 0, u16::MAX as i128) }
    #[builtin(name = "__uint32_parse", pure)]
    pub fn uint32_parse(s: &str) -> Result<i64, BuiltinError> { parse_int(s, "UInt32", 0, u32::MAX as i128) }
    /// Values above `i64::MAX` are stored bit-preserving (they read back as negative
    /// `long`s; `UInt64.ToString` reinterprets them).
    #[builtin(name = "__uint64_parse", pure)]
    pub fn uint64_parse(s: &str) -> Result<i64, BuiltinError> { parse_int(s, "UInt64", 0, u64::MAX as i128) }
    #[builtin(name = "__double_parse", pure)]
    pub fn double_parse(s: &str) -> Result<f64, BuiltinError> { parse_double(s) }

    #[builtin(name = "__int64_try_parse")]
    pub fn int64_try_parse(ctx: &VmContext, s: Option<&str>, v: RefArg) -> Result<bool, BuiltinError> {
        try_int(ctx, s, v, "Int64", i64::MIN as i128, i64::MAX as i128)
    }
    #[builtin(name = "__int32_try_parse")]
    pub fn int32_try_parse(ctx: &VmContext, s: Option<&str>, v: RefArg) -> Result<bool, BuiltinError> {
        try_int(ctx, s, v, "Int32", i32::MIN as i128, i32::MAX as i128)
    }
    #[builtin(name = "__int16_try_parse")]
    pub fn int16_try_parse(ctx: &VmContext, s: Option<&str>, v: RefArg) -> Result<bool, BuiltinError> {
        try_int(ctx, s, v, "Int16", i16::MIN as i128, i16::MAX as i128)
    }
    #[builtin(name = "__sbyte_try_parse")]
    pub fn sbyte_try_parse(ctx: &VmContext, s: Option<&str>, v: RefArg) -> Result<bool, BuiltinError> {
        try_int(ctx, s, v, "SByte", i8::MIN as i128, i8::MAX as i128)
    }
    #[builtin(name = "__byte_try_parse")]
    pub fn byte_try_parse(ctx: &VmContext, s: Option<&str>, v: RefArg) -> Result<bool, BuiltinError> {
        try_int(ctx, s, v, "Byte", 0, u8::MAX as i128)
    }
    #[builtin(name = "__uint16_try_parse")]
    pub fn uint16_try_parse(ctx: &VmContext, s: Option<&str>, v: RefArg) -> Result<bool, BuiltinError> {
        try_int(ctx, s, v, "UInt16", 0, u16::MAX as i128)
    }
    #[builtin(name = "__uint32_try_parse")]
    pub fn uint32_try_parse(ctx: &VmContext, s: Option<&str>, v: RefArg) -> Result<bool, BuiltinError> {
        try_int(ctx, s, v, "UInt32", 0, u32::MAX as i128)
    }
    #[builtin(name = "__uint64_try_parse")]
    pub fn uint64_try_parse(ctx: &VmContext, s: Option<&str>, v: RefArg) -> Result<bool, BuiltinError> {
        try_int(ctx, s, v, "UInt64", 0, u64::MAX as i128)
    }
    #[builtin(name = "__double_try_parse")]
    pub fn double_try_parse(ctx: &VmContext, s: Option<&str>, v: RefArg) -> Result<bool, BuiltinError> {
        let parsed = s.and_then(|s| parse_double(s).ok()).map(Value::F64);
        try_store(ctx, v, parsed, Value::F64(0.0))
    }

    /// `Convert.ToString(object)` and the compiler's string conversions.
    #[builtin(name = "__to_str")]
    pub fn to_str(v: &Value) -> String { value_to_str(v) }

    #[builtin(name = "__int32_to_string", pure)]
    pub fn int32_to_string(v: i64) -> String { v.to_string() }
    /// `UInt64.ToString`: the payload is the bit pattern (see `__uint64_parse`), so it is
    /// reinterpreted as `u64` before formatting.
    #[builtin(name = "__uint64_to_string", pure)]
    pub fn uint64_to_string(v: i64) -> String { (v as u64).to_string() }
    #[builtin(name = "__double_to_string", pure)]
    pub fn double_to_string(v: f64) -> String { v.to_string() }
    #[builtin(name = "__char_to_string", pure)]
    pub fn char_to_string(c: char) -> String { c.to_string() }

    /// Ordinal comparison; a null argument sorts first (.NET: any string > null).
    #[builtin(name = "__str_compare_to", this = "CompareTo", pure)]
    pub fn str_compare_to(this: &str, other: Option<&str>) -> i64 {
        match other {
            None => 1,
            Some(b) => this.cmp(b) as i64,
        }
    }

    // IEEE-754 bit reinterpretation backing `BitConverter` (`Std.IO.BinaryReader/Writer`).
    // `Single` round-trips through `f32`; its 32-bit pattern rides zero-extended in the
    // `i64`'s low word (`as u32` on the way back recovers it regardless of sign extension).
    #[builtin(name = "__single_to_bits", pure)]
    pub fn single_to_bits(v: f64) -> i64 { (v as f32).to_bits() as i64 }
    #[builtin(name = "__single_from_bits", pure)]
    pub fn single_from_bits(bits: i64) -> f64 { f32::from_bits(bits as u32) as f64 }
    #[builtin(name = "__double_to_bits", pure)]
    pub fn double_to_bits(v: f64) -> i64 { v.to_bits() as i64 }
    #[builtin(name = "__double_from_bits", pure)]
    pub fn double_from_bits(bits: i64) -> f64 { f64::from_bits(bits as u64) }

    /// Box an integer primitive into a heap `ScriptObject` with reference identity (C#
    /// semantics: `object o = 5; object p = 5; ReferenceEquals(o, p) == false`). The
    /// compiler emits `__box_prim(%value, "<FQ wrapper>")` at prim → object / interface
    /// conversions; only integers come through here (bool / char / double / string keep
    /// their own `Value` variants). An already boxed value is returned as is.
    ///
    /// `Null` here breaks the invariant "a value-type slot never holds `Null`" (E0475 /
    /// E0476 / E0483 rule out every legal way to get one): a VM or compiler bug, so it
    /// is an internal (fatal) error in debug builds — the golden corpus runs on the debug
    /// VM and becomes a detector for it — and passes through in release, where a user
    /// must not pay for our diagnostics.
    #[builtin(name = "__box_prim")]
    pub fn box_prim(ctx: &VmContext, v: &Value, class: &Value) -> Result<Value, BuiltinError> {
        match v {
            Value::BoxedStruct(_) => Ok(*v),
            Value::Null if cfg!(debug_assertions) => Err(BuiltinError::Internal(format!(
                "__box_prim received Null — 「值类型的存储槽永不含 Value::Null」这条不变式被破。\
                 用户代码不可能合法地走到这里（E0475 / E0476 / E0483 已在编译期堵住），\
                 所以这是 VM 或编译器的缺陷：请查这个值是从哪个槽读出来的（字段 / 数组元素 / \
                 帧槽 / 泛型型参字段），而不是查调用方的 z42 代码。class arg = {class:?}"))),
            Value::Null => Ok(Value::Null),
            Value::I64(raw) => match class {
                Value::Str(c) => super::box_prim_to_heap(ctx, c, *raw)
                    .map_err(|e| BuiltinError::Internal(e.to_string())),
                other => Err(BuiltinError::Internal(format!("__box_prim: class argument is {other:?}, not a type name"))),
            },
            other => Err(BuiltinError::Internal(format!("__box_prim: expected an integer, got {other:?}"))),
        }
    }

    /// Box a blob value struct into a shared heap `Value::BoxedStruct`: snapshot the
    /// bytes, references and type name out of the arena slot (the struct is live at the
    /// boxing point), detaching it from the frame's lifetime. Emitted by the compiler at
    /// struct → object / interface conversions, and after an element read whose static type
    /// is a generic method's type parameter (generic method bodies keep `T` values boxed; a
    /// `struct[]` element read yields a frame-scoped view that must not leave the frame).
    /// Already boxed → the same box (reference identity); a non-struct value passes through.
    #[builtin(name = "__box_struct")]
    pub fn box_struct(ctx: &VmContext, v: &Value) -> Result<Value, BuiltinError> {
        match v {
            Value::BoxedStruct(_) => Ok(*v),
            Value::StructRef { idx, frame_id } => {
                let (type_name, bytes, refs) = ctx.struct_arena.lock()
                    .with(*idx, *frame_id, |s| (s.type_name.clone(), s.bytes.clone(), s.refs.clone()))
                    .map_err(|e| BuiltinError::Internal(e.to_string()))?;
                super::box_struct_blob(ctx, &type_name, &bytes, &refs).map_err(|e| BuiltinError::Internal(e.to_string()))
            }
            Value::StructRefHeap { idx, frame_id } => {
                let e = ctx.transient_arena.lock().struct_elem(*idx, *frame_id)
                    .map_err(|e| BuiltinError::Internal(e.to_string()))?;
                let (bytes, refs, _layout, type_name) = crate::objops::struct_leaf::snapshot_elem(&e)
                    .map_err(|e| BuiltinError::Internal(e.to_string()))?;
                super::box_struct_blob(ctx, &type_name, &bytes, &refs).map_err(|e| BuiltinError::Internal(e.to_string()))
            }
            other => Ok(*other),
        }
    }

    /// `GetHashCode` of a boxed value struct (see `struct_hash_code`).
    #[builtin(name = "__struct_hash_code", pure)]
    pub fn struct_hash_code(v: &Value) -> Result<i64, BuiltinError> {
        match v {
            Value::BoxedStruct(gc) => Ok(super::struct_hash_code(gc)),
            other => Err(BuiltinError::Internal(format!("__struct_hash_code: expected a boxed struct, got {other:?}"))),
        }
    }
}

#[cfg(test)]
#[path = "convert_tests.rs"]
mod convert_tests;

#[cfg(test)]
mod mmh3_x86_128_tests {
    use super::{hex_str, mmh3_x86_128};
    fn h(s: &str) -> String { hex_str(&mmh3_x86_128(s.as_bytes())) }
    // 向量与 z42.package/tests/murmur3.z42 同源（smhasher 参考实现，seed 0，h1..h4 小端拼接）：
    // 两侧必须逐字相等——这就是编译器源文本哈希换到 native 后缓存键仍可比的依据。
    #[test]
    fn matches_z42_murmur3_vectors() {
        assert_eq!(h(""), "00000000000000000000000000000000");
        assert_eq!(h("a"), "3c9394a71bb056551bb056551bb05655");
        assert_eq!(h("abcd"), "aaccb6962ec6af452ec6af452ec6af45");
        assert_eq!(h("abcdefghijklmnop"), "2776d29f5612b990218bcee445ba93d1");
        assert_eq!(h("abcdefghijklmnopq"), "d3a445046f5c51642adc6dd99d07111d");
        assert_eq!(h("abcdefghijklmnopqrst"), "19342546a8216db62873b49e545dcb1f");
    }
    #[test]
    fn hex_is_lowercase_two_chars_per_byte() {
        assert_eq!(hex_str(b"\x00\xff\x10"), "00ff10");
        assert_eq!(hex_str("é".as_bytes()), "c3a9");
    }
}
