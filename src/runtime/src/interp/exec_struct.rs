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

use crate::metadata::types::StructTypeLayout;
use super::Frame;

// unify-object-byte-layout (PR-2): the primitive byte<->Value codec moved to
// `metadata::types` (both object byte-storage and struct blobs consume it). Re-export
// so existing call sites here + in `corelib::reflection` keep the same path.
pub(crate) use crate::metadata::types::{decode_prim, encode_prim, prim_width, is_ref_tag};

/// `StructAlloc dst, type_name, size` — allocate a zero-initialized blob in the
/// per-context struct arena; `dst` = `Value::StructRef` handle. The blob's byte +
/// reference layout comes from the type's TYPE-section struct block (via
/// [`resolve_layout`]); pure-primitive types fall back to a `size`-only layout.
pub(super) fn struct_alloc(
    ctx: &VmContext, frame: &mut Frame, dst: u32, type_name: &str, size: u32,
) -> Result<()> {
    let v = struct_alloc_val(ctx, frame.frame_id, type_name, size);
    frame.set(dst, v);
    Ok(())
}

/// Frame-agnostic core of `StructAlloc` — allocate a zero-initialized blob in the
/// per-context struct arena stamped with `frame_id`; returns the `StructRef` handle.
/// Shared by interp ([`struct_alloc`]) and the JIT struct helpers
/// (`jit::helpers::struct_ops`) which read `frame_id` off `JitFrame`.
pub(crate) fn struct_alloc_val(
    ctx: &VmContext, frame_id: u32, type_name: &str, size: u32,
) -> Value {
    let layout = resolve_layout(ctx, type_name, size);
    let idx = ctx.struct_alloc(frame_id, Arc::from(type_name), layout);
    Value::StructRef { idx, frame_id }
}

/// Resolve a value-struct type's runtime layout (byte size + reference bitmap).
/// A-use delivers it via the loaded `TypeDesc`; a type without a delivered layout
/// (or before the TYPE-section block reaches the runtime) falls back to a
/// `size`-only pure-primitive layout — byte-for-byte the pre-A-use behavior.
pub(crate) fn resolve_layout(ctx: &VmContext, type_name: &str, size: u32) -> Arc<StructTypeLayout> {
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
pub(crate) fn unbox_struct(
    ctx: &VmContext, frame_id: u32, gc: &crate::gc::GcRef<ty::ScriptObject>,
) -> Result<Value> {
    // add-boxed-struct-identity (P4b, 路 B2): the box is a shared struct-typed
    // `ScriptObject` — snapshot its blob (`struct_bytes`/`struct_refs`) into a fresh
    // current-frame arena `StructRef` (value-semantics unbox: the arena copy is
    // independent of the box).
    let (type_name, bytes, refs): (Arc<str>, Vec<u8>, Vec<Value>) = {
        let o = gc.borrow();
        (Arc::from(&*o.type_desc.name), o.bytes().to_vec(), o.refs().to_vec())
    };
    let layout = resolve_layout(ctx, &type_name, bytes.len() as u32);
    let idx = ctx.struct_alloc(frame_id, type_name, layout);
    ctx.struct_arena.lock().with_mut(idx, frame_id, |s| {
        let n = bytes.len().min(s.bytes.len());
        s.bytes[..n].copy_from_slice(&bytes[..n]);
        let rn = refs.len().min(s.refs.len());
        s.refs[..rn].clone_from_slice(&refs[..rn]);
    })?;
    Ok(Value::StructRef { idx, frame_id })
}

/// add-struct-foreach (P3b follow-up): copy a `StructBytes`-array element out to a fresh
/// **current-frame** arena `StructRef` (a value-semantics snapshot). Used by `as_cast` when
/// a `foreach (P p in arr)` loop var (or any value-context read) receives a `StructRefHeap`
/// element handle — the loop var must be an independent copy, not an alias into the array.
/// Mirrors [`unbox_struct`] but the source is a byte-backed array element, not a boxed blob.
pub(crate) fn copy_array_elem_out(ctx: &VmContext, frame_id: u32, e: &ty::StructArrayElem) -> Result<Value> {
    let i = e.index as usize;
    let (src_bytes, src_refs, layout, tname): (Vec<u8>, Vec<Value>, Arc<StructTypeLayout>, Arc<str>) = {
        let arr = e.arr.borrow();
        // unify-gc-heap PR-3: struct[] element bytes + refs live in GC blocks — read via accessors.
        let layout = arr.struct_layout().ok_or_else(|| anyhow::anyhow!("as-cast on a non-value-struct array element"))?;
        let elem_size = layout.size;
        let rc = layout.ref_count();
        let bstart = i * elem_size;
        let bytes = arr.struct_bytes().expect("StructBytes backing");
        let refs = arr.gc_refs();
        (bytes[bstart..bstart + elem_size].to_vec(),
         refs[i * rc..i * rc + rc].to_vec(),
         layout,
         arr.element_type.clone())
    };
    let idx = ctx.struct_alloc(frame_id, tname, layout);
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
    let dst_val = frame.get(dst)?.clone();
    let src_val = frame.get(src)?.clone();
    struct_copy_val(ctx, &dst_val, &src_val, size)
}

/// Frame-agnostic core of `StructCopy` — copy the `src` blob into the `dst` blob
/// (both already arena-allocated). Shared by interp and the JIT struct helpers.
pub(crate) fn struct_copy_val(
    ctx: &VmContext, dst_val: &Value, src_val: &Value, size: u32,
) -> Result<()> {
    let (d_idx, d_fid) = as_struct_ref(dst_val, "StructCopy dst")?;
    let (s_idx, s_fid) = as_struct_ref(src_val, "StructCopy src")?;
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
    let val = struct_field_get_val(ctx, &base_val, byte_off, kind)?;
    frame.set(dst, val);
    Ok(())
}

/// Frame-agnostic core of `StructFieldGetPrim` — read the leaf at `byte_off` of the
/// `base_val` struct (base = arena `StructRef` / heap `Object` inline field /
/// `StructRefHeap` array element). Shared by interp and the JIT struct helpers.
pub(crate) fn struct_field_get_val(
    ctx: &VmContext, base_val: &Value, byte_off: u32, kind: u8,
) -> Result<Value> {
    let val = match base_val {
        // unify-object-byte-layout (PR-2): inline struct leaf of a heap object — read
        // from the object's `bytes`/`refs` via the composed object layout. `byte_off`
        // is the compiler-baked **composed** object-relative offset (task 2.6).
        Value::Object(gc) => {
            let obj = gc.borrow();
            if is_ref_tag(kind) {
                let col = obj.type_desc.composed_object_layout().ok_or_else(|| {
                    anyhow::anyhow!("StructFieldGetPrim: object `{}` has no object layout", obj.type_desc.name)
                })?;
                let ri = col.ref_index(byte_off).ok_or_else(|| {
                    anyhow::anyhow!("inline struct ref leaf at byte offset {byte_off} not in object layout")
                })?;
                obj.refs()[ri].clone()
            } else {
                let off = byte_off as usize;
                let w = prim_width(kind)?;
                decode_prim(&obj.bytes(), off, w, kind)?
            }
        }
        // add-static-struct-bytecization (PR-2 S): leaf of a **boxed** value struct (a
        // static struct field is stored as a `BoxedStruct` for process-lifetime +
        // reference identity — `Holder.P.X` mutates it in place). The box's `bytes`/`refs`
        // ARE the struct blob (**struct** layout, not composed object layout); `byte_off`
        // is the struct-relative leaf offset (`FieldByteOffset`).
        Value::BoxedStruct(gc) => {
            let obj = gc.borrow();
            if is_ref_tag(kind) {
                let sl = obj.type_desc.struct_layout().ok_or_else(|| {
                    anyhow::anyhow!("StructFieldGetPrim: boxed struct `{}` has no struct layout", obj.type_desc.name)
                })?;
                let ri = sl.ref_index(byte_off).ok_or_else(|| {
                    anyhow::anyhow!("boxed struct ref leaf at byte offset {byte_off} not in struct layout")
                })?;
                obj.refs()[ri].clone()
            } else {
                let off = byte_off as usize;
                let w = prim_width(kind)?;
                decode_prim(&obj.bytes(), off, w, kind)?
            }
        }
        // add-struct-heap-inline (P3b, D1-a): leaf of a struct[] element `arr[index]`.
        // make-value-copy: resolve the StructRefHeap handle → StructArrayElem via the arena.
        Value::StructRefHeap { idx, frame_id } => {
            let e = ctx.transient_arena.lock().struct_elem(*idx, *frame_id)?;
            let arr = e.arr.borrow();
            // unify-gc-heap PR-3: struct[] element bytes/refs live in GC blocks — read via accessors.
            let layout = arr.struct_layout()
                .ok_or_else(|| anyhow::anyhow!("StructFieldGetPrim: StructRefHeap base is not a value-struct array"))?;
            let i = e.index as usize;
            if is_ref_tag(kind) {
                let rc = layout.ref_count();
                let ri = layout.ref_index(byte_off).ok_or_else(|| {
                    anyhow::anyhow!("struct[] ref leaf at byte offset {byte_off} not in element layout")
                })?;
                arr.gc_refs()[i * rc + ri].clone()
            } else {
                let off = i * layout.size + byte_off as usize;
                let w = prim_width(kind)?;
                decode_prim(arr.struct_bytes().expect("StructBytes backing"), off, w, kind)?
            }
        }
        _ => {
            let (idx, fid) = as_struct_ref(base_val, "StructFieldGetPrim base")?;
            if is_ref_tag(kind) {
                ctx.struct_arena.lock().get_ref(idx, fid, byte_off)?
            } else {
                let off = byte_off as usize;
                let w = prim_width(kind)?;
                ctx.struct_arena.lock().with(idx, fid, |s| decode_prim(&s.bytes, off, w, kind))??
            }
        }
    };
    Ok(val)
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
    struct_field_set_val(ctx, &base_val, byte_off, kind, &v)
}

/// Frame-agnostic core of `StructFieldSetPrim` — write `v` into the `base_val`
/// struct's leaf at `byte_off` in place (base = arena `StructRef` / heap `Object`
/// inline field / `StructRefHeap` array element). Heap bases route reference-leaf
/// writes through a write barrier. Shared by interp and the JIT struct helpers.
pub(crate) fn struct_field_set_val(
    ctx: &VmContext, base_val: &Value, byte_off: u32, kind: u8, v: &Value,
) -> Result<()> {
    match base_val {
        // unify-object-byte-layout (PR-2): inline struct leaf of a heap object — write
        // into the object's `bytes`/`refs` via the composed object layout. `byte_off`
        // is the compiler-baked composed object-relative offset (task 2.6).
        Value::Object(gc) => {
            if is_ref_tag(kind) {
                let ri = {
                    let mut obj = gc.borrow_mut();
                    let col = obj.type_desc.composed_object_layout().ok_or_else(|| {
                        anyhow::anyhow!("StructFieldSetPrim: object `{}` has no object layout", obj.type_desc.name)
                    })?;
                    let ri = col.ref_index(byte_off).ok_or_else(|| {
                        anyhow::anyhow!("inline struct ref leaf at byte offset {byte_off} not in object layout")
                    })?;
                    obj.refs_mut()[ri] = v.clone();
                    ri
                };
                // Write barrier: reference stored into a heap object. The `slot`
                // argument is informational (card/diagnostics); the ref index is a
                // stable per-object identifier. STW mode = no-op.
                if v.is_heap_ref() {
                    ctx.heap().write_barrier_field(base_val, ri, v);
                }
                Ok(())
            } else {
                let off = byte_off as usize;
                let w = prim_width(kind)?;
                let mut obj = gc.borrow_mut();
                encode_prim(&mut obj.bytes_mut(), off, w, kind, v)
            }
        }
        // add-static-struct-bytecization (PR-2 S): leaf write into a **boxed** value
        // struct in place (static struct field `Holder.P.X = 5`; the box has reference
        // identity so the mutation persists). Struct layout + struct-relative `byte_off`.
        Value::BoxedStruct(gc) => {
            if is_ref_tag(kind) {
                let ri = {
                    let mut obj = gc.borrow_mut();
                    let sl = obj.type_desc.struct_layout().ok_or_else(|| {
                        anyhow::anyhow!("StructFieldSetPrim: boxed struct `{}` has no struct layout", obj.type_desc.name)
                    })?;
                    let ri = sl.ref_index(byte_off).ok_or_else(|| {
                        anyhow::anyhow!("boxed struct ref leaf at byte offset {byte_off} not in struct layout")
                    })?;
                    obj.refs_mut()[ri] = v.clone();
                    ri
                };
                if v.is_heap_ref() {
                    ctx.heap().write_barrier_field(base_val, ri, v);
                }
                Ok(())
            } else {
                let off = byte_off as usize;
                let w = prim_width(kind)?;
                let mut obj = gc.borrow_mut();
                encode_prim(&mut obj.bytes_mut(), off, w, kind, v)
            }
        }
        // add-struct-heap-inline (P3b, D1-a): leaf write into a struct[] element.
        // make-value-copy: resolve the StructRefHeap handle → StructArrayElem via the arena.
        Value::StructRefHeap { idx, frame_id } => {
            let e = ctx.transient_arena.lock().struct_elem(*idx, *frame_id)?;
            if is_ref_tag(kind) {
                {
                    // unify-gc-heap PR-3: write the ref leaf into the struct[] refs block.
                    let mut arr = e.arr.borrow_mut();
                    let layout = arr.struct_layout()
                        .ok_or_else(|| anyhow::anyhow!("StructFieldSetPrim: StructRefHeap base is not a value-struct array"))?;
                    let rc = layout.ref_count();
                    let ri = layout.ref_index(byte_off).ok_or_else(|| {
                        anyhow::anyhow!("struct[] ref leaf at byte offset {byte_off} not in element layout")
                    })?;
                    arr.struct_refs_mut().expect("StructBytes backing")[e.index as usize * rc + ri] = v.clone();
                }
                // Write barrier: reference stored into a heap array element (P3b).
                if v.is_heap_ref() {
                    let owner = Value::Array(e.arr.clone());
                    ctx.heap().write_barrier_array_elem(&owner, e.index as usize, v);
                }
                Ok(())
            } else {
                // unify-gc-heap PR-3: encode the prim leaf into the struct[] bytes block.
                let mut arr = e.arr.borrow_mut();
                let layout = arr.struct_layout()
                    .ok_or_else(|| anyhow::anyhow!("StructFieldSetPrim: StructRefHeap base is not a value-struct array"))?;
                let off = e.index as usize * layout.size + byte_off as usize;
                let w = prim_width(kind)?;
                encode_prim(arr.struct_bytes_mut().expect("StructBytes backing"), off, w, kind, v)
            }
        }
        _ => {
            let (idx, fid) = as_struct_ref(base_val, "StructFieldSetPrim base")?;
            if is_ref_tag(kind) {
                return ctx.struct_arena.lock().set_ref(idx, fid, byte_off, v.clone());
            }
            let off = byte_off as usize;
            let w = prim_width(kind)?;
            ctx.struct_arena.lock().with_mut(idx, fid, |s| encode_prim(&mut s.bytes, off, w, kind, v))?
        }
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn as_struct_ref(v: &Value, what: &str) -> Result<(u32, u32)> {
    match v {
        Value::StructRef { idx, frame_id } => Ok((*idx, *frame_id)),
        other => bail!("{what}: expected a struct value (StructRef), got {other:?}"),
    }
}

#[cfg(test)]
#[path = "exec_struct_tests.rs"]
mod exec_struct_tests;
