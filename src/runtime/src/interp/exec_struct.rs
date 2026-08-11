//! add-struct-value-semantics Phase A: interp execution of blob value-type
//! instructions (StructAlloc / StructCopy / StructFieldGetPrim / StructFieldSetPrim).
//!
//! Operates on the per-context byte arena ([`super::struct_arena`]); registers
//! hold `Value::StructRef { idx, frame_id }` handles. The primitive byte<->Value
//! codec is `kind`-driven — `kind` is a `TypeTag` (`TAG_I32` / `TAG_F64` / …)
//! giving the leaf's byte width and how to decode/encode it.

use crate::metadata::types as ty;
use crate::metadata::types::{ArrayBacking, Value};
use crate::vm_context::VmContext;
use anyhow::{bail, Result};
use std::sync::Arc;

use crate::metadata::types::StructTypeLayout;
use super::Frame;

/// `StructAlloc dst, type_name, size` — allocate a zero-initialized blob in the
/// per-context struct arena; `dst` = `Value::StructRef` handle. The blob's byte +
/// reference layout comes from the type's TYPE-section struct block (via
/// [`resolve_layout`]); pure-primitive types fall back to a `size`-only layout.
pub(super) fn struct_alloc(
    ctx: &VmContext, frame: &mut Frame, dst: u32, type_name: &str, size: u32,
) -> Result<()> {
    let frame_id = frame.frame_id;
    let layout = resolve_layout(ctx, type_name, size);
    let idx = ctx.struct_arena.lock().alloc(frame_id, Arc::from(type_name), layout);
    frame.set(dst, Value::StructRef { idx, frame_id });
    Ok(())
}

/// Resolve a value-struct type's runtime layout (byte size + reference bitmap).
/// A-use delivers it via the loaded `TypeDesc`; a type without a delivered layout
/// (or before the TYPE-section block reaches the runtime) falls back to a
/// `size`-only pure-primitive layout — byte-for-byte the pre-A-use behavior.
fn resolve_layout(ctx: &VmContext, type_name: &str, size: u32) -> Arc<StructTypeLayout> {
    if let Some(td) = ctx.try_lookup_type(type_name) {
        if let Some(layout) = td.struct_layout() {
            return layout;
        }
    }
    Arc::new(StructTypeLayout {
        size: size as usize,
        ref_offsets: Box::new([]),
        ref_kinds: Box::new([]),
    })
}

/// add-struct-object-boxing (PR2a): 拆箱——把堆 `BoxedStruct` 的 blob 拷回**当前帧** struct arena，
/// 返回值 struct `StructRef` 句柄（`(P)o` / `o as P` 用）。alloc 用类型布局（size 兜底自 `bytes.len()`），
/// 再 memcpy bytes + clone refs。拆出的 struct 是独立副本（改它不影响 boxed 或再次拆箱）。
pub(super) fn unbox_struct(ctx: &VmContext, frame: &Frame, b: &ty::BoxedStructData) -> Result<Value> {
    let frame_id = frame.frame_id;
    let layout = resolve_layout(ctx, &b.type_name, b.bytes.len() as u32);
    let idx = ctx.struct_arena.lock().alloc(frame_id, b.type_name.clone(), layout);
    ctx.struct_arena.lock().with_mut(idx, frame_id, |s| {
        let n = b.bytes.len().min(s.bytes.len());
        s.bytes[..n].copy_from_slice(&b.bytes[..n]);
        let rn = b.refs.len().min(s.refs.len());
        s.refs[..rn].clone_from_slice(&b.refs[..rn]);
    })?;
    Ok(Value::StructRef { idx, frame_id })
}

/// add-struct-foreach (P3b follow-up): copy a `StructBytes`-array element out to a fresh
/// **current-frame** arena `StructRef` (a value-semantics snapshot). Used by `as_cast` when
/// a `foreach (P p in arr)` loop var (or any value-context read) receives a `StructRefHeap`
/// element handle — the loop var must be an independent copy, not an alias into the array.
/// Mirrors [`unbox_struct`] but the source is a byte-backed array element, not a boxed blob.
pub(super) fn copy_array_elem_out(ctx: &VmContext, frame: &Frame, e: &ty::StructArrayElem) -> Result<Value> {
    let i = e.index as usize;
    let (src_bytes, src_refs, layout, tname): (Vec<u8>, Vec<Value>, Arc<StructTypeLayout>, Arc<str>) = {
        let arr = e.arr.borrow();
        match &arr.backing {
            ArrayBacking::StructBytes { elem_size, bytes, refs, layout } => {
                let rc = layout.ref_count();
                let bstart = i * elem_size;
                (bytes[bstart..bstart + elem_size].to_vec(),
                 refs[i * rc..i * rc + rc].to_vec(),
                 layout.clone(),
                 arr.element_type.clone())
            }
            _ => bail!("as-cast on a non-value-struct array element"),
        }
    };
    let frame_id = frame.frame_id;
    let idx = ctx.struct_arena.lock().alloc(frame_id, tname, layout);
    ctx.struct_arena.lock().with_mut(idx, frame_id, |s| {
        let n = src_bytes.len().min(s.bytes.len());
        s.bytes[..n].copy_from_slice(&src_bytes[..n]);
        let rn = src_refs.len().min(s.refs.len());
        s.refs[..rn].clone_from_slice(&src_refs[..rn]);
    })?;
    Ok(Value::StructRef { idx, frame_id })
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

/// `StructFieldGetPrim dst, base, byte_off, kind` — read the leaf at `byte_off` of
/// the `base` struct into `dst`. A primitive `kind` decodes bytes; a reference
/// `kind` (`string`/object/array) reads the `Value` from the reference side-slice.
///
/// `base` may be a frame-scoped **arena** `StructRef` (local/param/temp struct) or,
/// since add-struct-heap-inline (P3b, route α), a **heap `Value::Object`** whose
/// inline struct field lives in `ScriptObject::struct_bytes`/`struct_refs` — the
/// compiler bakes `byte_off` as the object-relative composite offset
/// (`field_byte_off + leaf_off`).
pub(super) fn struct_field_get_prim(
    ctx: &VmContext, frame: &mut Frame, dst: u32, base: u32, byte_off: u32, kind: u8,
) -> Result<()> {
    let base_val = frame.get(base)?.clone();
    let val = match &base_val {
        // add-struct-heap-inline (P3b, D1-a): inline struct field of a heap object.
        Value::Object(gc) => {
            let obj = gc.borrow();
            if is_ref_tag(kind) {
                let il = obj.type_desc.inline_layout().ok_or_else(|| {
                    anyhow::anyhow!("StructFieldGetPrim: object `{}` has no inline struct layout", obj.type_desc.name)
                })?;
                let ri = il.ref_index(byte_off).ok_or_else(|| {
                    anyhow::anyhow!("inline struct ref leaf at byte offset {byte_off} not in object layout")
                })?;
                obj.struct_refs[ri].clone()
            } else {
                let off = byte_off as usize;
                let w = prim_width(kind)?;
                decode_prim(&obj.struct_bytes, off, w, kind)?
            }
        }
        // add-struct-heap-inline (P3b, D1-a): leaf of a struct[] element `arr[index]`.
        Value::StructRefHeap(e) => {
            let arr = e.arr.borrow();
            match &arr.backing {
                ArrayBacking::StructBytes { elem_size, bytes, refs, layout } => {
                    let i = e.index as usize;
                    if is_ref_tag(kind) {
                        let rc = layout.ref_count();
                        let ri = layout.ref_index(byte_off).ok_or_else(|| {
                            anyhow::anyhow!("struct[] ref leaf at byte offset {byte_off} not in element layout")
                        })?;
                        refs[i * rc + ri].clone()
                    } else {
                        let off = i * elem_size + byte_off as usize;
                        let w = prim_width(kind)?;
                        decode_prim(bytes, off, w, kind)?
                    }
                }
                _ => bail!("StructFieldGetPrim: StructRefHeap base is not a value-struct array"),
            }
        }
        _ => {
            let (idx, fid) = as_struct_ref(&base_val, "StructFieldGetPrim base")?;
            if is_ref_tag(kind) {
                ctx.struct_arena.lock().get_ref(idx, fid, byte_off)?
            } else {
                let off = byte_off as usize;
                let w = prim_width(kind)?;
                ctx.struct_arena.lock().with(idx, fid, |s| decode_prim(&s.bytes, off, w, kind))??
            }
        }
    };
    frame.set(dst, val);
    Ok(())
}

/// `StructFieldSetPrim base, byte_off, kind, val` — write `val` into the `base`
/// struct at `byte_off` (in place; the value-struct lvalue write). A primitive
/// `kind` encodes bytes; a reference `kind` stores the `Value` into the reference
/// side-slice.
///
/// Arena base: no write barrier (the arena is a GC root, re-scanned every cycle).
/// Heap-object base (P3b): a reference-leaf write into `struct_refs` **does** need a
/// write barrier — the heap object is not re-scanned as a root, so a concurrent /
/// generational collector must observe the store (routed through `write_barrier_field`).
pub(super) fn struct_field_set_prim(
    ctx: &VmContext, frame: &mut Frame, base: u32, byte_off: u32, kind: u8, val: u32,
) -> Result<()> {
    let base_val = frame.get(base)?.clone();
    let v = frame.get(val)?.clone();
    match &base_val {
        // add-struct-heap-inline (P3b, D1-a): inline struct field of a heap object.
        Value::Object(gc) => {
            if is_ref_tag(kind) {
                let ri = {
                    let mut obj = gc.borrow_mut();
                    let il = obj.type_desc.inline_layout().ok_or_else(|| {
                        anyhow::anyhow!("StructFieldSetPrim: object `{}` has no inline struct layout", obj.type_desc.name)
                    })?;
                    let ri = il.ref_index(byte_off).ok_or_else(|| {
                        anyhow::anyhow!("inline struct ref leaf at byte offset {byte_off} not in object layout")
                    })?;
                    obj.struct_refs[ri] = v.clone();
                    ri
                };
                // Write barrier: reference stored into a heap object (P3b). The
                // `slot` argument is informational (card/diagnostics); the inline
                // ref index is a stable per-object identifier. STW mode = no-op.
                if v.is_heap_ref() {
                    ctx.heap().write_barrier_field(&base_val, ri, &v);
                }
                Ok(())
            } else {
                let off = byte_off as usize;
                let w = prim_width(kind)?;
                let mut obj = gc.borrow_mut();
                encode_prim(&mut obj.struct_bytes, off, w, kind, &v)
            }
        }
        // add-struct-heap-inline (P3b, D1-a): leaf write into a struct[] element.
        Value::StructRefHeap(e) => {
            if is_ref_tag(kind) {
                {
                    let mut arr = e.arr.borrow_mut();
                    match &mut arr.backing {
                        ArrayBacking::StructBytes { refs, layout, .. } => {
                            let rc = layout.ref_count();
                            let ri = layout.ref_index(byte_off).ok_or_else(|| {
                                anyhow::anyhow!("struct[] ref leaf at byte offset {byte_off} not in element layout")
                            })?;
                            refs[e.index as usize * rc + ri] = v.clone();
                        }
                        _ => bail!("StructFieldSetPrim: StructRefHeap base is not a value-struct array"),
                    }
                }
                // Write barrier: reference stored into a heap array element (P3b).
                if v.is_heap_ref() {
                    let owner = Value::Array(e.arr.clone());
                    ctx.heap().write_barrier_array_elem(&owner, e.index as usize, &v);
                }
                Ok(())
            } else {
                let mut arr = e.arr.borrow_mut();
                match &mut arr.backing {
                    ArrayBacking::StructBytes { elem_size, bytes, .. } => {
                        let off = e.index as usize * *elem_size + byte_off as usize;
                        let w = prim_width(kind)?;
                        encode_prim(bytes, off, w, kind, &v)
                    }
                    _ => bail!("StructFieldSetPrim: StructRefHeap base is not a value-struct array"),
                }
            }
        }
        _ => {
            let (idx, fid) = as_struct_ref(&base_val, "StructFieldSetPrim base")?;
            if is_ref_tag(kind) {
                return ctx.struct_arena.lock().set_ref(idx, fid, byte_off, v);
            }
            let off = byte_off as usize;
            let w = prim_width(kind)?;
            ctx.struct_arena.lock().with_mut(idx, fid, |s| encode_prim(&mut s.bytes, off, w, kind, &v))?
        }
    }
}

/// Whether a leaf `TypeTag` denotes a reference leaf (`string` / object / array),
/// which lives in the blob's `refs` side-slice rather than byte-packed in `bytes`.
#[inline]
fn is_ref_tag(kind: u8) -> bool {
    matches!(kind, ty::TAG_STR | ty::TAG_OBJECT | ty::TAG_ARRAY)
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
