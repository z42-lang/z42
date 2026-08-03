use crate::metadata::Value;
use crate::metadata::types::ArrayObj;
use crate::gc::GcRef;
use crate::vm_context::VmContext;
use anyhow::{anyhow, bail, Result};
use super::convert::{arg_str, arg_usize};

/// string.ToCharArray() — bulk materialise the whole `char[]` in ONE native
/// call (vs the per-char `CharAt` loop, which pays a builtin dispatch per
/// character). args: [this: str]
///
/// add-native-str-indexof (script-first experiment): exposes the backing
/// characters as a single primitive so string algorithms can be written in
/// SCRIPT over `arr[i]` (the `ArrayGet` opcode) instead of `CharAt` (a builtin
/// call) — the C# "string ops in managed code over a char buffer" model.
pub fn builtin_str_to_chars(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let s = arg_str(args, 0, "__str_to_chars")?;
    let elems: Vec<Value> = s.chars().map(Value::Char).collect();
    Ok(Value::Array(GcRef::new(ArrayObj::typed("char", elems))))
}

/// Returns the number of Unicode scalar values (characters) in the string.
/// O(n) — walks the UTF-8 bytes to count chars. For an O(1) byte count
/// see [`builtin_str_byte_length`] / `Std.String.ByteLength`.
/// args: [this: str]
pub fn builtin_str_length(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    // O(1) amortised via the per-string metadata cache (perf-str-char-index):
    // the z42c lexer queries `src.Length` once per character, which would be
    // O(n²) with a fresh `chars().count()` each time.
    match args.first() {
        Some(Value::Str(s)) => Ok(Value::I64(super::str_meta::char_len(s) as i64)),
        _ => {
            let s = arg_str(args, 0, "__str_length")?;
            Ok(Value::I64(s.chars().count() as i64))
        }
    }
}

/// Returns the number of UTF-8 bytes in the string. O(1).
///
/// review.md C11.1 (2026-05-27, option 4): non-breaking sibling to
/// `Length`. `Length` keeps the existing char-count (Unicode scalar)
/// semantics — `"你好".Length == 2`. `ByteLength` reports the underlying
/// UTF-8 byte storage — `"你好".ByteLength == 6` — for hot paths that
/// need O(1) size queries (allocation sizing, network framing, hashing).
/// args: [this: str]
pub fn builtin_str_byte_length(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let s = arg_str(args, 0, "__str_byte_length")?;
    Ok(Value::I64(s.len() as i64))
}

/// Returns the char at the given scalar index.
/// args: [this: str, index: i64]
///
/// docs/review.md Part 2 C11.2 (2026-05-25): single-pass implementation —
/// fast path returns at iteration `i` (O(i+1)); error path knows the
/// actual char count from the same iteration. Previous version did two
/// full scans (`chars().nth(i)` + `chars().count()` in the error branch)
/// — O(2n) on failure, wasteful on long strings.
pub fn builtin_str_char_at(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let i = arg_usize(args, 1, "__str_char_at")?;
    // O(1) amortised via the per-string metadata cache (perf-str-char-index):
    // ASCII strings index bytes directly; non-ASCII use a cached char→byte
    // offset table. Avoids the O(i) `chars().nth(i)` walk the lexer triggers
    // for every character (→ O(n²) over a source file).
    if let Some(Value::Str(s)) = args.first() {
        return match super::str_meta::char_at(s, i) {
            Some(c) => Ok(Value::Char(c)),
            None => Err(anyhow!(
                "__str_char_at: index {} out of range (length {})",
                i, super::str_meta::char_len(s))),
        };
    }
    let s = arg_str(args, 0, "__str_char_at")?;
    let mut last_seen = 0usize;
    for (idx, c) in s.chars().enumerate() {
        if idx == i {
            return Ok(Value::Char(c));
        }
        last_seen = idx + 1;
    }
    Err(anyhow!("__str_char_at: index {} out of range (length {})", i, last_seen))
}

/// Builds a string from a char[] array.
/// args: [chars: Array<Char>]
/// New in simplify-string-stdlib (2026-04-24): enables script-side string
/// construction (Substring / Replace / ToLower / ToUpper etc.).
pub fn builtin_str_from_chars(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let arr = match args.first() {
        Some(Value::Array(a)) => a.clone(),
        Some(other) => bail!("__str_from_chars: expected char[], got {:?}", other),
        None => bail!("__str_from_chars: missing arg 0"),
    };
    let out: String = arr.borrow().iter()
        .map(|v| match v {
            Value::Char(c) => Ok(*c),
            other => Err(anyhow!("__str_from_chars: array element must be char, got {:?}", other)),
        })
        .collect::<Result<String>>()?;
    Ok(Value::Str(out.into()))
}

// 2026-04-27 wave1-string-script: builtin_str_split + builtin_str_join removed.
// `Std.String.Split` / `Join` 现在是 z42 脚本，基于 CharAt + Substring。

// 2026-04-27 wave3a-str-concat-script: builtin_str_concat removed.
// `Std.String.Concat` 现在是 z42 脚本（用 `+` 即 IR StrConcatInstr）。

// ── Object protocol overrides for string ─────────────────────────────────────

/// string.ToString() — returns the string itself.
/// args: [this: str]
///
/// refactor-corelib-typed-extractors (2026-05-17): `arg_str` returns `&str`,
/// here we own a fresh `String` to fit `Value::Str(String)`. Only ONE clone
/// at the return boundary, vs the old code's double-clone（require_str clone
/// 一次 + Value::Str(s) 再持有一次）。
pub fn builtin_str_to_string(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let s = arg_str(args, 0, "__str_to_string")?;
    Ok(Value::Str(s.to_owned().into()))
}

/// string.Equals(other) — value equality.
/// args: [this: str, other: str | null]
pub fn builtin_str_equals(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let a = arg_str(args, 0, "__str_equals")?;
    let result = match args.get(1) {
        Some(Value::Str(b)) => a == &**b,
        Some(Value::Null) | None => false,
        _ => false,
    };
    Ok(Value::Bool(result))
}

/// string.GetHashCode() — FNV-1a hash of the UTF-8 bytes.
/// args: [this: str]
pub fn builtin_str_hash_code(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let s = arg_str(args, 0, "__str_hash_code")?;
    let mut hash: u32 = 2_166_136_261;
    for byte in s.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16_777_619);
    }
    Ok(Value::I64((hash & 0x7fff_ffff) as i64))
}

// 2026-04-27 wave3b-str-format-script: builtin_str_format removed.
// `Std.String.Format` 现在是 z42 脚本（用 string.Replace + Convert.ToString）。

#[cfg(test)]
#[path = "string_tests.rs"]
mod string_tests;
