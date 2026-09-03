//! inline ref 与基元字节编解码（read/write_inline_ref、decode/encode_prim）。refactor-split-metadata-types（2026-09-03）：从 2436 行的 `types.rs` 按职责拆出，
//! 对外路径不变（`metadata::types::*` 经 hub 的 `pub use` 全量再导出）。

#![allow(unused_imports)]
use super::*;
use std::sync::Arc;
use crate::metadata::vstr::Str;
use crate::gc::GcRef;
use crate::gc::var_region::{BlockType, VarGcRef};
use crate::gc::heap::MagrGC;

/// unify-object-byte-layout (PR-3 chunk 2b): read a byte-inlined direct object/array
/// reference — the 8B tagged `GcRef` pointer at `off` in an object's `bytes` — back into
/// a `Value`. `0` (the zero-initialized default / an explicit `Null` store) → `Value::Null`.
/// `is_array` picks the variant (`Value::Array` vs `Value::Object`); the raw pointer has
/// no object-vs-array discriminant, so the kind must come from the layout's `field_kinds`.
#[inline]
pub(crate) fn read_inline_ref(bytes: &[u8], off: usize, is_array: bool) -> Value {
    if off + 8 > bytes.len() {
        return Value::Null;
    }
    let mut b = [0u8; 8];
    b.copy_from_slice(&bytes[off..off + 8]);
    let bits = u64::from_le_bytes(b);
    if is_array {
        // SAFETY: `bits` was written by `write_inline_ref` from a live `GcRef<ArrayObj>`
        // whose backing Region outlives this object (a field reference is a strong root
        // kept alive by GC tracing of `inline_refs`); `0` → `None` → `Null`.
        match unsafe { GcRef::<ArrayObj>::from_tagged_bits(bits) } {
            Some(r) => Value::Array(r),
            None => Value::Null,
        }
    } else {
        // SAFETY: as above, `bits` came from a live `GcRef<ScriptObject>`.
        match unsafe { GcRef::<ScriptObject>::from_tagged_bits(bits) } {
            Some(r) => Value::Object(r),
            None => Value::Null,
        }
    }
}

/// unify-object-byte-layout (PR-3 chunk 2b): write a `Value` into a byte-inlined direct
/// object/array reference slot (`off` in `bytes`). Heap `Object`/`Array` → their 8B tagged
/// pointer; `Null` → `0`. Any other value (including a stack-escaped `StackObject`/
/// `StackArray`, which must have been heap-promoted before reaching a field — see
/// `exec_object::field_set`'s debug_assert) defensively stores `0` rather than a bogus
/// pointer. The write barrier is fired separately by the caller.
#[inline]
pub(crate) fn write_inline_ref(bytes: &mut [u8], off: usize, v: &Value) {
    let bits: u64 = match v {
        Value::Object(r) => r.to_tagged_bits(),
        Value::Array(r) => r.to_tagged_bits(),
        _ => {
            debug_assert!(
                matches!(v, Value::Null),
                "inlined object/array field only holds a heap Object/Array or Null, got {v:?}"
            );
            0
        }
    };
    if off + 8 <= bytes.len() {
        bytes[off..off + 8].copy_from_slice(&bits.to_le_bytes());
    }
}

// ── Value ↔ byte codec (unify-object-byte-layout PR-2) ───────────────────────
//
// Serialization of a primitive `Value` to/from a byte window, keyed by `ty::TAG_*`.
// Lives in `metadata` (not `interp`) because both object byte-storage
// (`ScriptObject::field_value`) and value-struct blobs (`interp::exec_struct`) consume
// it, and `metadata` is the lower layer both depend on. Moved here from
// `interp::exec_struct` by PR-2 (was `add-struct-heap-inline`).

/// Whether a leaf `ty::TAG_*` denotes a reference leaf (`string` / object / array),
/// which lives in the blob's / object's `refs` side-slice rather than byte-packed.
#[inline]
pub fn is_ref_tag(tag: u8) -> bool {
    matches!(tag, TAG_STR | TAG_OBJECT | TAG_ARRAY)
}

/// Byte width of a primitive leaf by its `ty::TAG_*`.
pub fn prim_width(kind: u8) -> anyhow::Result<usize> {
    Ok(match kind {
        TAG_BOOL | TAG_I8 | TAG_U8 => 1,
        TAG_I16 | TAG_U16 => 2,
        TAG_I32 | TAG_U32 | TAG_F32 | TAG_CHAR => 4,
        TAG_I64 | TAG_U64 | TAG_F64 => 8,
        other => anyhow::bail!("struct field: unsupported primitive tag {other:#x}"),
    })
}

/// Decode `w` bytes at `off` into a `Value` per `kind`. Integers → `Value::I64`,
/// f32/f64 → `Value::F64`, bool → `Value::Bool`, char → `Value::Char` (mirrors the
/// VM's scalar representation of primitives).
pub fn decode_prim(bytes: &[u8], off: usize, w: usize, kind: u8) -> anyhow::Result<Value> {
    if off + w > bytes.len() {
        anyhow::bail!("struct field read out of blob bounds (off={off}, w={w}, len={})", bytes.len());
    }
    let b = &bytes[off..off + w];
    let v = match kind {
        TAG_BOOL => Value::Bool(b[0] != 0),
        TAG_I8   => Value::I64(b[0] as i8 as i64),
        TAG_U8   => Value::I64(b[0] as i64),
        TAG_I16  => Value::I64(i16::from_le_bytes([b[0], b[1]]) as i64),
        TAG_U16  => Value::I64(u16::from_le_bytes([b[0], b[1]]) as i64),
        TAG_I32  => Value::I64(i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64),
        TAG_U32  => Value::I64(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64),
        TAG_I64 | TAG_U64 => {
            let mut a = [0u8; 8]; a.copy_from_slice(b); Value::I64(i64::from_le_bytes(a))
        }
        TAG_F32  => Value::F64(f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64),
        TAG_F64  => {
            let mut a = [0u8; 8]; a.copy_from_slice(b); Value::F64(f64::from_le_bytes(a))
        }
        TAG_CHAR => {
            let cp = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            Value::Char(char::from_u32(cp).unwrap_or('\0'))
        }
        other => anyhow::bail!("struct field: unsupported primitive tag {other:#x}"),
    };
    Ok(v)
}

/// Encode `val` into `w` bytes at `off` per `kind` (in place).
pub fn encode_prim(bytes: &mut [u8], off: usize, w: usize, kind: u8, val: &Value) -> anyhow::Result<()> {
    if off + w > bytes.len() {
        anyhow::bail!("struct field write out of blob bounds (off={off}, w={w}, len={})", bytes.len());
    }
    match kind {
        TAG_BOOL => bytes[off] = if codec_as_bool(val)? { 1 } else { 0 },
        TAG_I8 | TAG_U8 => bytes[off] = codec_as_i64(val)? as u8,
        TAG_I16 | TAG_U16 => bytes[off..off + 2].copy_from_slice(&(codec_as_i64(val)? as u16).to_le_bytes()),
        TAG_I32 | TAG_U32 => bytes[off..off + 4].copy_from_slice(&(codec_as_i64(val)? as u32).to_le_bytes()),
        TAG_I64 | TAG_U64 => bytes[off..off + 8].copy_from_slice(&codec_as_i64(val)?.to_le_bytes()),
        TAG_F32 => bytes[off..off + 4].copy_from_slice(&(codec_as_f64(val)? as f32).to_le_bytes()),
        TAG_F64 => bytes[off..off + 8].copy_from_slice(&codec_as_f64(val)?.to_le_bytes()),
        TAG_CHAR => bytes[off..off + 4].copy_from_slice(&codec_as_char_u32(val)?.to_le_bytes()),
        other => anyhow::bail!("struct field: unsupported primitive tag {other:#x}"),
    }
    Ok(())
}

fn codec_as_i64(v: &Value) -> anyhow::Result<i64> {
    match v {
        Value::I64(n) => Ok(*n),
        Value::Bool(b) => Ok(*b as i64),
        Value::Char(c) => Ok(*c as i64),
        other => anyhow::bail!("struct field: expected an integer value, got {other:?}"),
    }
}

fn codec_as_f64(v: &Value) -> anyhow::Result<f64> {
    match v {
        Value::F64(f) => Ok(*f),
        Value::I64(n) => Ok(*n as f64),
        other => anyhow::bail!("struct field: expected a float value, got {other:?}"),
    }
}

fn codec_as_bool(v: &Value) -> anyhow::Result<bool> {
    match v {
        Value::Bool(b) => Ok(*b),
        Value::I64(n) => Ok(*n != 0),
        other => anyhow::bail!("struct field: expected a bool value, got {other:?}"),
    }
}

fn codec_as_char_u32(v: &Value) -> anyhow::Result<u32> {
    match v {
        Value::Char(c) => Ok(*c as u32),
        Value::I64(n) => Ok(*n as u32),
        other => anyhow::bail!("struct field: expected a char value, got {other:?}"),
    }
}
