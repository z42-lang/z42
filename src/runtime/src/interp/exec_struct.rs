//! add-struct-value-semantics Phase A: interp execution of blob value-type
//! instructions (StructAlloc / StructCopy / StructFieldGetPrim / StructFieldSetPrim).
//!
//! Operates on the per-context byte arena ([`super::struct_arena`]); registers
//! hold `Value::StructRef { idx, frame_id }` handles. The primitive byte<->Value
//! codec is `kind`-driven — `kind` is a `TypeTag` (`TAG_I32` / `TAG_F64` / …)
//! giving the leaf's byte width and how to decode/encode it.

use crate::metadata::types as ty;
use crate::metadata::types::Value;
use crate::vm_context::VmContext;
use anyhow::{bail, Result};
use std::sync::Arc;

use super::Frame;

/// `StructAlloc dst, type_name, size` — allocate a zero-initialized blob in the
/// per-context struct arena; `dst` = `Value::StructRef` handle.
pub(super) fn struct_alloc(
    ctx: &VmContext, frame: &mut Frame, dst: u32, type_name: &str, size: u32,
) -> Result<()> {
    let frame_id = frame.frame_id;
    let idx = ctx.struct_arena.lock().alloc(frame_id, Arc::from(type_name), size as usize);
    frame.set(dst, Value::StructRef { idx, frame_id });
    Ok(())
}

/// `StructCopy dst, src, size` — copy the `src` blob into the `dst` blob (both
/// already allocated). This is the value-semantics copy point (assign/param/return).
pub(super) fn struct_copy(
    ctx: &VmContext, frame: &mut Frame, dst: u32, src: u32, size: u32,
) -> Result<()> {
    let (d_idx, d_fid) = as_struct_ref(frame.get(dst)?, "StructCopy dst")?;
    let (s_idx, s_fid) = as_struct_ref(frame.get(src)?, "StructCopy src")?;
    ctx.struct_arena.lock().copy_into(d_idx, d_fid, s_idx, s_fid, size as usize)
}

/// `StructFieldGetPrim dst, base, byte_off, kind` — read the primitive leaf at
/// `byte_off` of the `base` blob into `dst`, decoded per `kind`.
pub(super) fn struct_field_get_prim(
    ctx: &VmContext, frame: &mut Frame, dst: u32, base: u32, byte_off: u32, kind: u8,
) -> Result<()> {
    let (idx, fid) = as_struct_ref(frame.get(base)?, "StructFieldGetPrim base")?;
    let off = byte_off as usize;
    let w = prim_width(kind)?;
    let val = ctx.struct_arena.lock().with(idx, fid, |s| decode_prim(&s.bytes, off, w, kind))??;
    frame.set(dst, val);
    Ok(())
}

/// `StructFieldSetPrim base, byte_off, kind, val` — write primitive `val` into the
/// `base` blob at `byte_off` (in-place; the value-struct lvalue write).
pub(super) fn struct_field_set_prim(
    ctx: &VmContext, frame: &mut Frame, base: u32, byte_off: u32, kind: u8, val: u32,
) -> Result<()> {
    let (idx, fid) = as_struct_ref(frame.get(base)?, "StructFieldSetPrim base")?;
    let v = frame.get(val)?.clone();
    let off = byte_off as usize;
    let w = prim_width(kind)?;
    ctx.struct_arena.lock().with_mut(idx, fid, |s| encode_prim(&mut s.bytes, off, w, kind, &v))?
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn as_struct_ref(v: &Value, what: &str) -> Result<(u32, u32)> {
    match v {
        Value::StructRef { idx, frame_id } => Ok((*idx, *frame_id)),
        other => bail!("{what}: expected a struct value (StructRef), got {other:?}"),
    }
}

/// Byte width of a primitive leaf by its `TypeTag`.
fn prim_width(kind: u8) -> Result<usize> {
    Ok(match kind {
        ty::TAG_BOOL | ty::TAG_I8 | ty::TAG_U8 => 1,
        ty::TAG_I16 | ty::TAG_U16 => 2,
        ty::TAG_I32 | ty::TAG_U32 | ty::TAG_F32 | ty::TAG_CHAR => 4,
        ty::TAG_I64 | ty::TAG_U64 | ty::TAG_F64 => 8,
        other => bail!("struct field: unsupported primitive tag {other:#x}"),
    })
}

/// Decode `w` bytes at `off` into a `Value` per `kind`. Integers → `Value::I64`,
/// f32/f64 → `Value::F64`, bool → `Value::Bool`, char → `Value::Char` (mirrors the
/// VM's scalar representation of primitives).
fn decode_prim(bytes: &[u8], off: usize, w: usize, kind: u8) -> Result<Value> {
    if off + w > bytes.len() {
        bail!("struct field read out of blob bounds (off={off}, w={w}, len={})", bytes.len());
    }
    let b = &bytes[off..off + w];
    let v = match kind {
        ty::TAG_BOOL => Value::Bool(b[0] != 0),
        ty::TAG_I8   => Value::I64(b[0] as i8 as i64),
        ty::TAG_U8   => Value::I64(b[0] as i64),
        ty::TAG_I16  => Value::I64(i16::from_le_bytes([b[0], b[1]]) as i64),
        ty::TAG_U16  => Value::I64(u16::from_le_bytes([b[0], b[1]]) as i64),
        ty::TAG_I32  => Value::I64(i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64),
        ty::TAG_U32  => Value::I64(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64),
        ty::TAG_I64 | ty::TAG_U64 => {
            let mut a = [0u8; 8]; a.copy_from_slice(b); Value::I64(i64::from_le_bytes(a))
        }
        ty::TAG_F32  => Value::F64(f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64),
        ty::TAG_F64  => {
            let mut a = [0u8; 8]; a.copy_from_slice(b); Value::F64(f64::from_le_bytes(a))
        }
        ty::TAG_CHAR => {
            let cp = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            Value::Char(char::from_u32(cp).unwrap_or('\0'))
        }
        other => bail!("struct field: unsupported primitive tag {other:#x}"),
    };
    Ok(v)
}

/// Encode `val` into `w` bytes at `off` per `kind` (in place).
fn encode_prim(bytes: &mut [u8], off: usize, w: usize, kind: u8, val: &Value) -> Result<()> {
    if off + w > bytes.len() {
        bail!("struct field write out of blob bounds (off={off}, w={w}, len={})", bytes.len());
    }
    match kind {
        ty::TAG_BOOL => bytes[off] = if as_bool(val)? { 1 } else { 0 },
        ty::TAG_I8 | ty::TAG_U8 => bytes[off] = as_i64(val)? as u8,
        ty::TAG_I16 | ty::TAG_U16 => bytes[off..off + 2].copy_from_slice(&(as_i64(val)? as u16).to_le_bytes()),
        ty::TAG_I32 | ty::TAG_U32 => bytes[off..off + 4].copy_from_slice(&(as_i64(val)? as u32).to_le_bytes()),
        ty::TAG_I64 | ty::TAG_U64 => bytes[off..off + 8].copy_from_slice(&as_i64(val)?.to_le_bytes()),
        ty::TAG_F32 => bytes[off..off + 4].copy_from_slice(&(as_f64(val)? as f32).to_le_bytes()),
        ty::TAG_F64 => bytes[off..off + 8].copy_from_slice(&as_f64(val)?.to_le_bytes()),
        ty::TAG_CHAR => bytes[off..off + 4].copy_from_slice(&as_char_u32(val)?.to_le_bytes()),
        other => bail!("struct field: unsupported primitive tag {other:#x}"),
    }
    Ok(())
}

fn as_i64(v: &Value) -> Result<i64> {
    match v {
        Value::I64(n) => Ok(*n),
        Value::Bool(b) => Ok(*b as i64),
        Value::Char(c) => Ok(*c as i64),
        other => bail!("struct field: expected an integer value, got {other:?}"),
    }
}

fn as_f64(v: &Value) -> Result<f64> {
    match v {
        Value::F64(f) => Ok(*f),
        Value::I64(n) => Ok(*n as f64),
        other => bail!("struct field: expected a float value, got {other:?}"),
    }
}

fn as_bool(v: &Value) -> Result<bool> {
    match v {
        Value::Bool(b) => Ok(*b),
        Value::I64(n) => Ok(*n != 0),
        other => bail!("struct field: expected a bool value, got {other:?}"),
    }
}

fn as_char_u32(v: &Value) -> Result<u32> {
    match v {
        Value::Char(c) => Ok(*c as u32),
        Value::I64(n) => Ok(*n as u32),
        other => bail!("struct field: expected a char value, got {other:?}"),
    }
}

#[cfg(test)]
#[path = "exec_struct_tests.rs"]
mod exec_struct_tests;
