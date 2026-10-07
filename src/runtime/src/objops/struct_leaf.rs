//! 值 struct 的叶子读写：`StructFieldGetPrim` / `StructFieldSetPrim` 的引擎无关核心，以及
//! struct 数组元素 / 装箱 struct 的整块快照。interp（`exec_struct`）与 JIT（`helpers/struct_ops`）
//! 都调这里，不碰对象 / 数组的存储细节。
//!
//! 叶子所在的 base 有五种形态，偏移的编号空间由 base 决定：
//!
//! | base | 布局 | 写引用叶子的屏障 |
//! |---|---|---|
//! | `Object`（类实例里内联的 struct 字段） | composed 对象布局 | `write_barrier_field` |
//! | `StackObject` | composed 对象布局 | 无（arena 是根） |
//! | `BoxedStruct`（装箱 struct，静态 struct 字段也是它） | struct 布局 | `write_barrier_field` |
//! | `StructRefHeap`（`struct[]` 元素） | struct 布局 × 元素下标 | `write_barrier_array_elem` |
//! | `StructRef`（帧内 arena blob） | struct 布局 | 无（arena 是根） |

use crate::metadata::types::{
    decode_prim, encode_prim, is_ref_tag, prim_width, ScriptObject, StructArrayElem, StructTypeLayout, Value,
};
use crate::gc::GcRef;
use crate::vm_context::VmContext;
use anyhow::{bail, Result};
use std::sync::Arc;

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
                obj.storage.load_prim(off, w, kind)?
            }
        }
        // fix-stackobj-inline-struct-leaf: same as `Value::Object` above, but the object
        // lives in the **stack arena** (escape analysis stack-allocated it). Identical
        // layout and `byte_off` semantics — a stack object is the same type descriptor,
        // just a different home. Without this arm the read fell through to
        // `as_struct_ref` and bailed `expected a struct value (StructRef), got StackObject`.
        Value::StackObject { idx, frame_id } => {
            let (idx, frame_id) = (*idx, *frame_id);
            ctx.stack_arena.lock().with_obj(idx, frame_id, |obj| {
                if is_ref_tag(kind) {
                    let col = obj.type_desc.composed_object_layout().ok_or_else(|| {
                        anyhow::anyhow!("StructFieldGetPrim: stack object `{}` has no object layout", obj.type_desc.name)
                    })?;
                    let ri = col.ref_index(byte_off).ok_or_else(|| {
                        anyhow::anyhow!("inline struct ref leaf at byte offset {byte_off} not in object layout")
                    })?;
                    Ok(obj.refs()[ri].clone())
                } else {
                    let w = prim_width(kind)?;
                    obj.storage.load_prim(byte_off as usize, w, kind)
                }
            })??
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
                obj.storage.load_prim(off, w, kind)?
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
                    obj.set_ref_slot(ri, v);
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
                obj.storage.store_prim(off, w, kind, v)
            }
        }
        // fix-stackobj-inline-struct-leaf: same as `Value::Object` above, but the object
        // lives in the **stack arena** (escape analysis stack-allocated it).
        //
        // 🔴 **No write barrier**, unlike the heap-object arm — and that asymmetry is
        // deliberate, mirroring `exec_object.rs::field_set`: a stack object is not a heap
        // slot, and its heap-ref fields are kept live by **root-scanning the arena** every
        // cycle. Adding a barrier here would be harmless-but-wrong (it would card-mark a
        // non-heap address).
        Value::StackObject { idx, frame_id } => {
            let (idx, frame_id) = (*idx, *frame_id);
            ctx.stack_arena.lock().with_obj_mut(idx, frame_id, |obj| {
                if is_ref_tag(kind) {
                    let ri = {
                        let col = obj.type_desc.composed_object_layout().ok_or_else(|| {
                            anyhow::anyhow!("StructFieldSetPrim: stack object `{}` has no object layout", obj.type_desc.name)
                        })?;
                        col.ref_index(byte_off).ok_or_else(|| {
                            anyhow::anyhow!("inline struct ref leaf at byte offset {byte_off} not in object layout")
                        })?
                    };
                    obj.set_ref_slot(ri, v);
                    Ok(())
                } else {
                    let w = prim_width(kind)?;
                    obj.storage.store_prim(byte_off as usize, w, kind, v)
                }
            })?
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
                    obj.set_ref_slot(ri, v);
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
                obj.storage.store_prim(off, w, kind, v)
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
                    if !arr.set_struct_ref(e.index as usize * rc + ri, v) {
                        anyhow::bail!("StructFieldSetPrim: struct[] ref leaf {ri} of element {} out of range", e.index);
                    }
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

/// 装箱 struct 的整块快照（类型名、字节、引用叶子）——拆箱时拷进帧 arena。
pub(crate) fn snapshot_box(gc: &GcRef<ScriptObject>) -> (Arc<str>, Vec<u8>, Vec<Value>) {
    let o = gc.borrow();
    (Arc::from(&*o.type_desc.name), o.bytes().to_vec(), o.refs().to_vec())
}

/// `struct[]` 元素的整块快照（字节、引用叶子、元素布局、元素类型名）——值语义拷出时用。
pub(crate) fn snapshot_elem(e: &StructArrayElem) -> Result<(Vec<u8>, Vec<Value>, Arc<StructTypeLayout>, Arc<str>)> {
    let i = e.index as usize;
    let arr = e.arr.borrow();
    let layout = arr.struct_layout()
        .ok_or_else(|| anyhow::anyhow!("as-cast on a non-value-struct array element"))?;
    let (size, rc) = (layout.size, layout.ref_count());
    let bytes = arr.struct_bytes()
        .ok_or_else(|| anyhow::anyhow!("struct array element without a StructBytes backing"))?;
    let refs = arr.gc_refs();
    Ok((bytes[i * size..(i + 1) * size].to_vec(), refs[i * rc..(i + 1) * rc].to_vec(), layout, arr.element_type.arc()))
}

pub(crate) fn as_struct_ref(v: &Value, what: &str) -> Result<(u32, u32)> {
    match v {
        Value::StructRef { idx, frame_id } => Ok((*idx, *frame_id)),
        other => bail!("{what}: expected a struct value (StructRef), got {other:?}"),
    }
}
