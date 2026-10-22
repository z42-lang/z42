use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::{anyhow, bail, Result};
use crate::corelib::abi::BuiltinError;
use super::convert::{arg_str, arg_usize, this_str};

/// string.ToCharArray() — bulk materialise the whole `char[]` in ONE native
/// call (vs the per-char `CharAt` loop, which pays a builtin dispatch per
/// character). args: [this: str]
///
/// add-native-str-indexof (script-first experiment): exposes the backing
/// characters as a single primitive so string algorithms can be written in
/// SCRIPT over `arr[i]` (the `ArrayGet` opcode) instead of `CharAt` (a builtin
/// call) — the C# "string ops in managed code over a char buffer" model.
pub fn str_to_chars(ctx: &VmContext, s: &crate::metadata::vstr::Str) -> Value {
    let s: &str = s;
    // One PRIM `char[]` block, decoded straight into it (perf-array-alloc-direct: no staging
    // `Vec<Value>`).
    let n = s.chars().count();
    let heap = ctx.heap();
    crate::metadata::types::ArrayObj::alloc_typed_iter(heap, crate::metadata::types::ElemType::char(), n, s.chars().map(Value::Char))
}

/// Returns the number of UTF-8 bytes in the string. O(1).
///
/// review.md C11.1 (2026-05-27, option 4): non-breaking sibling to
/// `Length`. `Length` keeps the existing char-count (Unicode scalar)
/// semantics — `"你好".Length == 2`. `ByteLength` reports the underlying
/// UTF-8 byte storage — `"你好".ByteLength == 6` — for hot paths that
/// need O(1) size queries (allocation sizing, network framing, hashing).
/// args: [this: str]
pub fn str_byte_length(s: &crate::metadata::vstr::Str) -> i64 {
    s.len() as i64
}

/// The string access builtins on ABI v2 (`string.Length`, `s[i]` / `CharAt`): typed, no
/// frame when folded — the hottest `[Native]` calls of string-processing code.
#[z42_macros::builtins]
pub mod typed {
    use crate::corelib::abi::BuiltinError;
    use crate::vm_context::VmContext;
    use crate::metadata::vstr::Str;

    /// Number of Unicode scalar values. O(1) amortised via the per-string metadata
    /// cache (`str_meta`, perf-str-char-index): the z42c lexer queries `src.Length` once
    /// per character, which would be O(n²) with a fresh count each time.
    #[builtin(name = "__str_length", this = "get_Length", pure)]
    pub fn str_length(this: &Str) -> i64 {
        crate::corelib::str_meta::char_len(this) as i64
    }

    /// The char at scalar index `i`. O(1) amortised: ASCII strings index bytes directly,
    /// others use a cached char → byte offset table (`str_meta`). Out of range (negative
    /// included) → `IndexOutOfRangeException`, as .NET's `s[i]`.
    #[builtin(name = "__str_char_at", this = "CharAt", pure)]
    pub fn str_char_at(this: &Str, i: i64) -> Result<char, BuiltinError> {
        usize::try_from(i).ok()
            .and_then(|i| crate::corelib::str_meta::char_at(this, i))
            .ok_or_else(|| BuiltinError::IndexOutOfRange(format!(
                "index {i} is out of range for a string of length {}", crate::corelib::str_meta::char_len(this))))
    }
    /// `ToCharArray()` — 一个 PRIM char[] 块,解码直写。
    #[builtin(name = "__str_to_chars", this = "ToCharArray")]
    pub fn to_chars(ctx: &VmContext, s: &crate::metadata::vstr::Str) -> crate::metadata::Value {
        super::str_to_chars(ctx, s)
    }

    /// UTF-8 字节数,O(1)(`"你好".ByteLength == 6`)。
    #[builtin(name = "__str_byte_length", this = "get_ByteLength", pure)]
    pub fn byte_length(s: &crate::metadata::vstr::Str) -> i64 { super::str_byte_length(s) }

    #[builtin(name = "__str_substring", this = "Substring")]
    pub fn substring(
        s: &crate::metadata::vstr::Str, start: i64, len: i64,
    ) -> Result<String, BuiltinError> { super::str_substring(s, start, len) }

    /// `ConcatParts(parts, count)` — 单分配拼接(StringBuilder.ToString / Join 的地板)。
    #[builtin(name = "__str_concat_parts")]
    pub fn concat_parts(ctx: &VmContext, parts: &crate::metadata::Value, count: i64) -> Result<String, BuiltinError> {
        super::str_concat_parts(ctx, parts, count)
    }

    #[builtin(name = "__str_split", this = "Split")]
    pub fn split(ctx: &VmContext, s: &crate::metadata::vstr::Str, sep: &str) -> Result<crate::metadata::Value, BuiltinError> {
        super::str_split(ctx, s, sep)
    }

    #[builtin(name = "__str_join")]
    pub fn join(sep: &str, values: &crate::metadata::Value) -> Result<String, BuiltinError> {
        super::str_join(sep, values)
    }

    #[builtin(name = "__str_from_chars")]
    pub fn from_chars(chars: &crate::metadata::Value) -> Result<String, BuiltinError> {
        super::str_from_chars(chars)
    }

    #[builtin(name = "__str_equals", this = "Equals", pure)]
    pub fn equals(
        s: &crate::metadata::vstr::Str, other: Option<&crate::metadata::vstr::Str>,
    ) -> bool { super::str_equals(s, other) }

    /// FNV-1a 哈希(UTF-8 字节),31 位截断。
    #[builtin(name = "__str_hash_code", this = "GetHashCode", pure)]
    pub fn hash_code(s: &crate::metadata::vstr::Str) -> i64 { super::str_hash_code(s) }

}

/// `Std.String.Substring(start, length)` bulk path (perf-stdlib-hot-paths): character
/// range → byte range via the per-string metadata cache, one slice copy into a new heap
/// string. Bails on an out-of-range request (the script side checks first and reports
/// its own message; this is the defensive floor).
/// args: [this: str, start: int, length: int]
pub fn str_substring(
    s: &crate::metadata::vstr::Str, start: i64, len: i64,
) -> Result<String, BuiltinError> {
    let (start, len) = (start as usize, len as usize);
    match super::str_meta::byte_range(s, start, len) {
        Some((b0, b1)) => Ok(s[b0..b1].to_string()),
        // 与旧表同语义:越界 → ArgumentOutOfRangeException(此前 catchable
        // Std.Exception,文案不变)。
        None => Err(BuiltinError::ArgumentOutOfRange(format!(
            "__str_substring: range [{}, {}) out of bounds (length {})",
            start, start + len, super::str_meta::char_len(s)))),
    }
}

/// `Std.String.ConcatParts(parts, count)` (perf-stdlib-hot-paths): concatenate the first
/// `count` elements of a `string[]` with a single allocation — the `StringBuilder.ToString`
/// / `Join` floor. Elements must be strings (null is rejected: a StringBuilder never
/// stores null parts, so a null here is a caller bug worth surfacing).
/// args: [parts: string[], count: int]
pub fn str_concat_parts(
    ctx: &VmContext, parts: &Value, count: i64,
) -> Result<String, BuiltinError> {
    let count = count as usize;
    let Value::Array(arr) = parts else {
        return Err(BuiltinError::Internal("__str_concat_parts: expected string[]".into()));
    };
    let b = arr.borrow();
    if count > b.len() {
        return Err(BuiltinError::Argument(format!(
            "__str_concat_parts: count {} exceeds array length {}", count, b.len())));
    }
    // 单趟 iter_boxed(与 str_join 同因:两趟对每元素多一次 acquire+decode)。
    let mut out = String::with_capacity(count * 16);
    for v in b.iter_boxed().take(count) {
        match &v {
            Value::Str(s) => out.push_str(s),
            other => return Err(BuiltinError::Argument(format!(
                "__str_concat_parts: element must be string, got {:?}", other))),
        }
    }
    out.shrink_to_fit();
    Ok(out)
}

/// `Std.String.Split(string)` (perf-strings-json): split `this` on every non-overlapping
/// occurrence of `sep`, scanning left to right, into a fresh `string[]` — empty pieces
/// kept, `n` separators ⇒ `n + 1` pieces. One native scan + one allocation per piece,
/// instead of a two-pass per-character script loop (UTF-8 is self-synchronising, so a
/// byte-level match of a valid needle is exactly a character-level match). The
/// pieces live only in this Rust `Vec` until the array holds them; that is safe for
/// the same reason as every multi-allocation builtin: collections run at safepoints
/// (none inside a builtin) and blocks allocated during marking are born marked.
/// args: [this: str, sep: str (non-empty — the script side rejects "")]
pub fn str_split(
    ctx: &VmContext, s: &crate::metadata::vstr::Str, sep: &str,
) -> Result<crate::metadata::Value, BuiltinError> {
    if sep.is_empty() {
        return Err(BuiltinError::Argument("__str_split: separator must not be empty".into()));
    }
    // 片段直进 GC 串 + string[] 一次成型(镜像旧表形态)。经 Vec<String> 返回会
    // 每片多一次 String 分配 + 返回时再拷进 GC(bench 实测 ~29% 慢)。
    let heap = ctx.heap();
    let parts: Vec<Value> = s.split(sep).map(|p| Value::Str(heap.alloc_str(p))).collect();
    Ok(heap.alloc_array_typed("string", parts))
}

/// `Std.String.Join(sep, values)` (perf-strings-json): `values[0] + sep + values[1] + …`
/// with a single allocation — the interleaving `ConcatParts` already did, minus building
/// the `2n − 1` interleaved `string[]` in script first. Elements must be strings (same
/// rule and message shape as `__str_concat_parts`).
/// args: [sep: str, values: string[]]
pub fn str_join(sep: &str, values: &Value) -> Result<String, BuiltinError> {
    let Value::Array(arr) = values else {
        return Err(BuiltinError::Internal("__str_join: expected string[]".into()));
    };
    // 单趟 iter_boxed(镜像旧表形态):容量先按保守上界估,元素解码只做一次——
    // 两趟形态(bench 实测)对 REF 数组每元素多一次 acquire+decode,~2ns/元素。
    let b = arr.borrow();
    let mut out = String::with_capacity(b.len() * (4 + sep.len()));
    for (i, v) in b.iter_boxed().enumerate() {
        if i > 0 { out.push_str(sep); }
        match &v {
            Value::Str(s) => out.push_str(s),
            other => return Err(BuiltinError::Argument(format!(
                "__str_join: element must be string, got {:?}", other))),
        }
    }
    out.shrink_to_fit();
    Ok(out)
}

/// Builds a string from a char[] array.
/// args: [chars: Array<Char>]
/// New in simplify-string-stdlib (2026-04-24): enables script-side string
/// construction (Substring / Replace / ToLower / ToUpper etc.).
pub fn str_from_chars(chars: &Value) -> Result<String, BuiltinError> {
    let Value::Array(arr) = chars else {
        return Err(BuiltinError::Internal("__str_from_chars: expected char[]".into()));
    };
    let b = arr.borrow();
    let mut out = String::with_capacity(b.len());
    for v in b.iter_boxed() {
        match v {
            Value::Char(c) => out.push(c),
            other => return Err(BuiltinError::Argument(format!(
                "__str_from_chars: array element must be char, got {:?}", other))),
        }
    }
    Ok(out)
}

// 2026-04-27 wave1-string-script: builtin_str_split + builtin_str_join removed.
// `Std.String.Split` / `Join` 现在是 z42 脚本，基于 CharAt + Substring。

// 2026-04-27 wave3a-str-concat-script: builtin_str_concat removed.
// `Std.String.Concat` 现在是 z42 脚本（用 `+` 即 IR StrConcatInstr）。

// ── Object protocol overrides for string ─────────────────────────────────────

// shrink-primitive-native-interop (2026-08-27): builtin_str_to_string removed —
// Std.String.ToString 现在是脚本 `return this;`（旧 builtin 只是原样返回自身）。

/// string.Equals(other) — value equality.
/// args: [this: str, other: str | null]
pub fn str_equals(a: &crate::metadata::vstr::Str, other: Option<&crate::metadata::vstr::Str>) -> bool {
    match other {
        Some(b) => a == b,
        None => false,
    }
}

/// string.GetHashCode() — FNV-1a hash of the UTF-8 bytes.
/// args: [this: str]
pub fn str_hash_code(s: &crate::metadata::vstr::Str) -> i64 {
    let mut hash: u32 = 2_166_136_261;
    for byte in s.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16_777_619);
    }
    (hash & 0x7fff_ffff) as i64
}

// 2026-04-27 wave3b-str-format-script: builtin_str_format removed.
// `Std.String.Format` 现在是 z42 脚本（用 string.Replace + Convert.ToString）。

#[cfg(test)]
#[path = "string_tests.rs"]
mod string_tests;
