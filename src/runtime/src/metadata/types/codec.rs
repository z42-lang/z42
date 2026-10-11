//! 基元字节编解码（decode/encode_prim、prim_to/from_bits）。refactor-split-metadata-types（2026-09-03）：从 2436 行的 `types.rs` 按职责拆出，
//! 对外路径不变（`metadata::types::*` 经 hub 的 `pub use` 全量再导出）。

#![allow(unused_imports)]
use super::*;
use std::sync::Arc;
use crate::metadata::vstr::Str;
use crate::gc::GcRef;
use crate::gc::var_region::{BlockType, VarGcRef};
use crate::gc::heap::MagrGC;

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

/// A primitive leaf's raw little-endian bits (low `prim_width(kind)` bytes significant) —
/// the value an object's same-width atomic cell stores (`ObjStorage::store_prim`). Same
/// conversions and rejections as [`encode_prim`].
#[inline]
pub fn prim_to_bits(kind: u8, val: &Value) -> anyhow::Result<u64> {
    Ok(match kind {
        TAG_BOOL => codec_as_bool(val)? as u64,
        TAG_I8 | TAG_U8 | TAG_I16 | TAG_U16 | TAG_I32 | TAG_U32 | TAG_I64 | TAG_U64 => codec_as_i64(val)? as u64,
        TAG_F32 => (codec_as_f64(val)? as f32).to_bits() as u64,
        TAG_F64 => codec_as_f64(val)?.to_bits(),
        TAG_CHAR => codec_as_char_u32(val)? as u64,
        other => anyhow::bail!("struct field: unsupported primitive tag {other:#x}"),
    })
}

/// Inverse of [`prim_to_bits`]: `bits` holds the cell's `prim_width(kind)` bytes, zero-extended.
/// Mirrors [`decode_prim`] (sign extension for signed narrow ints, f32 widened to f64).
#[inline]
pub fn prim_from_bits(kind: u8, bits: u64) -> Value {
    match kind {
        TAG_BOOL => Value::Bool(bits as u8 != 0),
        TAG_I8 => Value::I64(bits as u8 as i8 as i64),
        TAG_I16 => Value::I64(bits as u16 as i16 as i64),
        TAG_I32 => Value::I64(bits as u32 as i32 as i64),
        TAG_U8 | TAG_U16 | TAG_U32 | TAG_I64 | TAG_U64 => Value::I64(bits as i64),
        TAG_F32 => Value::F64(f32::from_bits(bits as u32) as f64),
        TAG_F64 => Value::F64(f64::from_bits(bits)),
        TAG_CHAR => Value::Char(char::from_u32(bits as u32).unwrap_or('\0')),
        _ => Value::Null,
    }
}

#[inline]
fn codec_as_i64(v: &Value) -> anyhow::Result<i64> {
    match v {
        Value::I64(n) => Ok(*n),
        Value::Bool(b) => Ok(*b as i64),
        Value::Char(c) => Ok(*c as i64),
        other => anyhow::bail!("struct field: expected an integer value, got {other:?}"),
    }
}

#[inline]
fn codec_as_f64(v: &Value) -> anyhow::Result<f64> {
    match v {
        Value::F64(f) => Ok(*f),
        Value::I64(n) => Ok(*n as f64),
        other => anyhow::bail!("struct field: expected a float value, got {other:?}"),
    }
}

#[inline]
fn codec_as_bool(v: &Value) -> anyhow::Result<bool> {
    match v {
        Value::Bool(b) => Ok(*b),
        Value::I64(n) => Ok(*n != 0),
        other => anyhow::bail!("struct field: expected a bool value, got {other:?}"),
    }
}

#[inline]
fn codec_as_char_u32(v: &Value) -> anyhow::Result<u32> {
    match v {
        Value::Char(c) => Ok(*c as u32),
        Value::I64(n) => Ok(*n as u32),
        other => anyhow::bail!("struct field: expected a char value, got {other:?}"),
    }
}
