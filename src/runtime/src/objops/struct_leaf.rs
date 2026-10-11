//! 值 struct 的叶子读写：`StructFieldGetPrim` / `StructFieldSetPrim` 的引擎无关核心，以及
//! struct 数组元素 / 装箱 struct 的整块快照。interp（`exec_struct`）与 JIT（`helpers/struct_ops`）
//! 都调这里，不碰对象 / 数组的存储细节。
//!
//! 叶子所在的 base 有五种形态，偏移的编号空间由 base 决定：
//!
//! 堆上的 base（前四种）里引用叶子是 8 B 引用字，帧 arena 里是 `Value`（`struct_blob`）。
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
/// `base_val` struct. Shared by interp and the JIT struct helpers.
///
/// On the heap (an object's inline struct, a boxed struct, a `struct[]` element) a reference
/// leaf is an 8 B reference word at its offset (`struct_blob`); in the frame arena it is a
/// `Value` in the blob's `refs`.
pub(crate) fn struct_field_get_val(
    ctx: &VmContext, base_val: &Value, byte_off: u32, kind: u8,
) -> Result<Value> {
    match base_val {
        // Inline struct leaf of a heap object (`byte_off` = composed object-relative offset), or
        // a boxed struct's own leaf (`byte_off` = blob-relative; the box's payload IS the blob).
        Value::Object(gc) | Value::BoxedStruct(gc) => leaf_get(&gc.borrow(), byte_off, kind),
        // The same, for an object escape analysis put in the stack arena.
        Value::StackObject { idx, frame_id } => {
            ctx.stack_arena.lock().with_obj(*idx, *frame_id, |obj| leaf_get(obj, byte_off, kind))?
        }
        // Leaf of a struct[] element `arr[index]`.
        Value::StructRefHeap { idx, frame_id } => {
            let e = ctx.transient_arena.lock().struct_elem(*idx, *frame_id)?;
            let arr = e.arr.borrow();
            let i = e.index as usize;
            if is_ref_tag(kind) {
                arr.struct_elem_ref(i, byte_off)
            } else {
                arr.struct_elem_prim(i, byte_off, prim_width(kind)?, kind)
            }
        }
        _ => {
            let (idx, fid) = as_struct_ref(base_val, "StructFieldGetPrim base")?;
            if is_ref_tag(kind) {
                ctx.struct_arena.lock().get_ref(idx, fid, byte_off)
            } else {
                let off = byte_off as usize;
                let w = prim_width(kind)?;
                ctx.struct_arena.lock().with(idx, fid, |s| decode_prim(&s.bytes, off, w, kind))?
            }
        }
    }
}

fn leaf_get(obj: &ScriptObject, byte_off: u32, kind: u8) -> Result<Value> {
    if is_ref_tag(kind) {
        obj.struct_ref_leaf(byte_off)
    } else {
        obj.prim_leaf(byte_off as usize, prim_width(kind)?, kind)
    }
}

/// Frame-agnostic core of `StructFieldSetPrim` — write `v` into the `base_val` struct's leaf
/// at `byte_off` in place. Heap bases route a reference store through the write barrier with
/// the reference that landed (a box, for a value that does not fit 8 B). Shared by interp and
/// the JIT struct helpers.
pub(crate) fn struct_field_set_val(
    ctx: &VmContext, base_val: &Value, byte_off: u32, kind: u8, v: &Value,
) -> Result<()> {
    match base_val {
        Value::Object(gc) | Value::BoxedStruct(gc) => {
            if is_ref_tag(kind) {
                let wrote = gc.borrow_mut().set_struct_ref_leaf(byte_off, v)?;
                // The slot argument is informational (card / diagnostics).
                wrote.with_barrier_value(v, |s| ctx.heap().write_barrier_field(base_val, byte_off as usize, s));
                Ok(())
            } else {
                gc.borrow_mut().set_prim_leaf(byte_off as usize, prim_width(kind)?, kind, v)
            }
        }
        // 🔴 **No write barrier** for a stack object, mirroring `field_set`: it is not a heap
        // slot, its references are kept live by root-scanning the arena every cycle.
        Value::StackObject { idx, frame_id } => {
            ctx.stack_arena.lock().with_obj_mut(*idx, *frame_id, |obj| {
                if is_ref_tag(kind) {
                    obj.set_struct_ref_leaf(byte_off, v).map(|_| ())
                } else {
                    obj.set_prim_leaf(byte_off as usize, prim_width(kind)?, kind, v)
                }
            })?
        }
        // Leaf write into a struct[] element.
        Value::StructRefHeap { idx, frame_id } => {
            let e = ctx.transient_arena.lock().struct_elem(*idx, *frame_id)?;
            let i = e.index as usize;
            if is_ref_tag(kind) {
                let wrote = e.arr.borrow_mut().set_struct_elem_ref(i, byte_off, v)?;
                let owner = Value::Array(e.arr);
                wrote.with_barrier_value(v, |s| ctx.heap().write_barrier_array_elem(&owner, i, s));
                Ok(())
            } else {
                e.arr.borrow_mut().set_struct_elem_prim(i, byte_off, prim_width(kind)?, kind, v)
            }
        }
        _ => {
            let (idx, fid) = as_struct_ref(base_val, "StructFieldSetPrim base")?;
            if is_ref_tag(kind) {
                return ctx.struct_arena.lock().set_ref(idx, fid, byte_off, *v);
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
    let (bytes, refs) = o.boxed_blob();
    (Arc::from(&*o.type_desc.name), bytes, refs)
}

/// `struct[]` 元素的整块快照（字节、引用叶子、元素布局、元素类型名）——值语义拷出时用。
pub(crate) fn snapshot_elem(e: &StructArrayElem) -> Result<(Vec<u8>, Vec<Value>, Arc<StructTypeLayout>, Arc<str>)> {
    let arr = e.arr.borrow();
    let layout = arr.struct_layout()
        .ok_or_else(|| anyhow::anyhow!("as-cast on a non-value-struct array element"))?;
    let (bytes, refs) = arr.struct_elem(e.index as usize)
        .ok_or_else(|| anyhow::anyhow!("struct array element without a StructBytes backing"))?;
    Ok((bytes, refs, layout, arr.element_type.arc()))
}

pub(crate) fn as_struct_ref(v: &Value, what: &str) -> Result<(u32, u32)> {
    match v {
        Value::StructRef { idx, frame_id } => Ok((*idx, *frame_id)),
        other => bail!("{what}: expected a struct value (StructRef), got {other:?}"),
    }
}
