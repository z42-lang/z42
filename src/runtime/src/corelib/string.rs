use crate::metadata::Value;
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
pub fn builtin_str_to_chars(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let s = arg_str(args, 0, "__str_to_chars")?;
    let elems: Vec<Value> = s.chars().map(Value::Char).collect();
    // unify-gc-heap PR-3: region-alloc the `char[]` (packed `Chars` block in the GC heap)
    // via the heap, not the leaking `GcRef::new` path.
    Ok(ctx.heap().alloc_array_typed("char", elems))
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

/// `Std.String.Substring(start, length)` bulk path (perf-stdlib-hot-paths): character
/// range → byte range via the per-string metadata cache, one slice copy into a new heap
/// string. Bails on an out-of-range request (the script side checks first and reports
/// its own message; this is the defensive floor).
/// args: [this: str, start: int, length: int]
pub fn builtin_str_substring(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let start = arg_usize(args, 1, "__str_substring")?;
    let len = arg_usize(args, 2, "__str_substring")?;
    match args.first() {
        Some(Value::Str(s)) => match super::str_meta::byte_range(s, start, len) {
            Some((b0, b1)) => Ok(Value::Str(ctx.heap().alloc_str(&s[b0..b1]))),
            None => bail!("__str_substring: range [{}, {}) out of bounds (length {})",
                          start, start + len, super::str_meta::char_len(s)),
        },
        Some(other) => bail!("__str_substring: expected string receiver, got {:?}", other),
        None => bail!("__str_substring: missing receiver"),
    }
}

/// `Std.String.ConcatParts(parts, count)` (perf-stdlib-hot-paths): concatenate the first
/// `count` elements of a `string[]` with a single allocation — the `StringBuilder.ToString`
/// / `Join` floor. Elements must be strings (null is rejected: a StringBuilder never
/// stores null parts, so a null here is a caller bug worth surfacing).
/// args: [parts: string[], count: int]
pub fn builtin_str_concat_parts(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let count = arg_usize(args, 1, "__str_concat_parts")?;
    let arr = match args.first() {
        Some(Value::Array(a)) => a.clone(),
        Some(other) => bail!("__str_concat_parts: expected string[], got {:?}", other),
        None => bail!("__str_concat_parts: missing arg 0"),
    };
    let b = arr.borrow();
    if count > b.len() {
        bail!("__str_concat_parts: count {} exceeds array length {}", count, b.len());
    }
    let mut total = 0usize;
    let mut parts: Vec<Value> = Vec::with_capacity(count);
    for v in b.iter_boxed().take(count) {
        match &v {
            Value::Str(s) => total += s.len(),
            other => bail!("__str_concat_parts: element must be string, got {:?}", other),
        }
        parts.push(v);
    }
    let mut out = String::with_capacity(total);
    for v in &parts {
        if let Value::Str(s) = v { out.push_str(s); }
    }
    Ok(Value::Str(ctx.heap().alloc_str(&out)))
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
    let out: String = arr.borrow().iter_boxed()
        .map(|v| match v {
            Value::Char(c) => Ok(c),
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

// shrink-primitive-native-interop (2026-08-27): builtin_str_to_string removed —
// Std.String.ToString 现在是脚本 `return this;`（旧 builtin 只是原样返回自身）。

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
