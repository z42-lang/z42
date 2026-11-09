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

use crate::object::types::named_struct_layout;
use crate::object::{
    decode_prim, encode_prim, is_ref_tag, prim_width, NativeData, ScriptObject, StructArrayElem,
    StructTypeLayout, Value,
};
use crate::context::VmContext;
use anyhow::{bail, Result};
use std::sync::Arc;

/// Copy a struct blob into a fresh heap object with boxed identity.
/// A fresh box is unreachable until returned, so its initial writes need no barrier.
pub fn box_struct_blob(
    ctx: &VmContext, type_name: &str, bytes: &[u8], refs: &[Value],
) -> Result<Value> {
    let td = ctx.try_lookup_type(type_name).ok_or_else(|| {
        anyhow::anyhow!("__box_struct: unknown struct type `{type_name}`")
    })?;
    let obj_val = ctx.heap().alloc_object(td, Vec::new(), NativeData::None);
    match obj_val {
        Value::Object(gc) => {
            {
                let o = gc.borrow();
                let size = o.payload_len();
                // A fresh box (young, unreachable until returned): no barrier owed.
                let offs = o.type_desc.struct_layout().map(|sl| sl.ref_offsets.clone()).unwrap_or_default();
                o.write_blob(0, size, bytes, refs, &offs, &mut |_| {})?;
            }
            Ok(Value::BoxedStruct(gc))
        }
        // alloc_object returns Null under strict-OOM refusal.
        other => Ok(other),
    }
}

/// Read a nested field into its own value snapshot.
fn snapshot_struct_leaf(
    ctx: &VmContext,
    o: &crate::object::ScriptObject,
    parent: &named_struct_layout::ComputedLayout,
    leaf: &named_struct_layout::FieldLeaf,
) -> Result<Value> {
    let resolve = |n: &str| ctx.try_lookup_type(n);
    let nested = named_struct_layout::compute(&resolve, &leaf.type_name)?;
    if nested.ref_offsets.iter().any(|&nro| parent.ref_index(leaf.byte_off + nro).is_none()) {
        bail!("struct field reflection: nested ref leaf offset not in parent bitmap");
    }
    let (bytes, refs) = o.read_blob(leaf.byte_off as usize, leaf.size as usize, &nested.ref_offsets);
    box_struct_blob(ctx, &leaf.type_name, &bytes, &refs)
}

/// Read a named field from an erased struct box. Reflection and FieldGet share
/// layout validation and copy nested struct values into a fresh box.
pub fn boxed_struct_field_get(
    ctx: &VmContext, gc: &crate::object::ObjRef, name: &str,
) -> Result<Value> {
    use crate::object::prim_width;
    let type_name = gc.type_desc().name.to_string();
    let resolve = |n: &str| ctx.try_lookup_type(n);
    let comp = named_struct_layout::compute(&resolve, &type_name)?;
    if let Some(sl) = gc.type_desc().struct_layout() {
        comp.validate_against(&sl, &type_name)?;
    } else {
        bail!("FieldInfo.GetValue: type `{type_name}` has no delivered struct layout");
    }
    let leaf = comp.field(name).ok_or_else(|| {
        anyhow::anyhow!("FieldInfo.GetValue: field `{name}` not present on `{type_name}`")
    })?;
    if leaf.is_struct {
        let o = gc.borrow();
        snapshot_struct_leaf(ctx, &o, &comp, leaf)
    } else if leaf.is_ref {
        gc.borrow().struct_ref_leaf(leaf.byte_off)
    } else {
        let w = prim_width(leaf.tag)?;
        gc.borrow().prim_leaf(leaf.byte_off as usize, w, leaf.tag)
    }
}

/// Frame-agnostic core of `StructFieldGetPrim` — read the leaf at `byte_off` of the
/// `base_val` struct. Shared by interp and the JIT struct helpers.
///
/// On the heap (an object's inline struct, a boxed struct, a `struct[]` element) a reference
/// leaf is an 8 B reference word at its offset (`struct_blob`); in the frame arena it is a
/// `Value` in the blob's `refs`.
pub fn struct_field_get_val(
    ctx: &VmContext, base_val: &Value, byte_off: u32, kind: u8,
) -> Result<Value> {
    match base_val {
        // Inline struct leaf of a heap object (`byte_off` = composed object-relative offset), or
        // a boxed struct's own leaf (`byte_off` = blob-relative; the box's payload IS the blob).
        Value::Object(gc) | Value::BoxedStruct(gc) => leaf_get(&gc.borrow(), byte_off, kind),
        // The same, for an object escape analysis put in the stack arena.
        Value::StackObject { idx, frame_id } => {
            ctx.exec.stack_arena.lock().with_obj(*idx, *frame_id, |obj| leaf_get(obj, byte_off, kind))?
        }
        // Leaf of a struct[] element `arr[index]`.
        Value::StructRefHeap { idx, frame_id } => {
            let e = ctx.exec.transient_arena.lock().struct_elem(*idx, *frame_id)?;
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
                ctx.exec.struct_arena.lock().get_ref(idx, fid, byte_off)
            } else {
                let off = byte_off as usize;
                let w = prim_width(kind)?;
                ctx.exec.struct_arena.lock().with(idx, fid, |s| decode_prim(&s.bytes, off, w, kind))?
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
pub fn struct_field_set_val(
    ctx: &VmContext, base_val: &Value, byte_off: u32, kind: u8, v: &Value,
) -> Result<()> {
    match base_val {
        Value::Object(gc) | Value::BoxedStruct(gc) => {
            if is_ref_tag(kind) {
                let wrote = gc.borrow().set_struct_ref_leaf(byte_off, v)?;
                // The slot argument is informational (card / diagnostics).
                wrote.with_barrier_value(v, |s| ctx.heap().write_barrier_field(base_val, byte_off as usize, s));
                Ok(())
            } else {
                gc.borrow().set_prim_leaf(byte_off as usize, prim_width(kind)?, kind, v)
            }
        }
        // 🔴 **No write barrier** for a stack object, mirroring `field_set`: it is not a heap
        // slot, its references are kept live by root-scanning the arena every cycle.
        Value::StackObject { idx, frame_id } => {
            ctx.exec.stack_arena.lock().with_obj_mut(*idx, *frame_id, |obj| {
                if is_ref_tag(kind) {
                    obj.set_struct_ref_leaf(byte_off, v).map(|_| ())
                } else {
                    obj.set_prim_leaf(byte_off as usize, prim_width(kind)?, kind, v)
                }
            })?
        }
        // Leaf write into a struct[] element.
        Value::StructRefHeap { idx, frame_id } => {
            let e = ctx.exec.transient_arena.lock().struct_elem(*idx, *frame_id)?;
            let i = e.index as usize;
            if is_ref_tag(kind) {
                let wrote = e.arr.borrow().set_struct_elem_ref(i, byte_off, v)?;
                let owner = Value::Array(e.arr);
                wrote.with_barrier_value(v, |s| ctx.heap().write_barrier_array_elem(&owner, i, s));
                Ok(())
            } else {
                e.arr.borrow().set_struct_elem_prim(i, byte_off, prim_width(kind)?, kind, v)
            }
        }
        _ => {
            let (idx, fid) = as_struct_ref(base_val, "StructFieldSetPrim base")?;
            if is_ref_tag(kind) {
                return ctx.exec.struct_arena.lock().set_ref(idx, fid, byte_off, *v);
            }
            let off = byte_off as usize;
            let w = prim_width(kind)?;
            ctx.exec.struct_arena.lock().with_mut(idx, fid, |s| encode_prim(&mut s.bytes, off, w, kind, v))?
        }
    }
}

/// 装箱 struct 的整块快照（类型名、字节、引用叶子）——拆箱时拷进帧 arena。
pub(crate) fn snapshot_box(gc: &crate::object::ObjRef) -> (Arc<str>, Vec<u8>, Vec<Value>) {
    let o = gc.borrow();
    let (bytes, refs) = o.boxed_blob();
    (Arc::from(&*o.type_desc.name), bytes, refs)
}

/// `struct[]` 元素的整块快照（字节、引用叶子、元素布局、元素类型名）——值语义拷出时用。
pub fn snapshot_elem(e: &StructArrayElem) -> Result<(Vec<u8>, Vec<Value>, Arc<StructTypeLayout>, Arc<str>)> {
    let arr = e.arr.borrow();
    let layout = arr.struct_layout()
        .ok_or_else(|| anyhow::anyhow!("as-cast on a non-value-struct array element"))?;
    let (bytes, refs) = arr.struct_elem(e.index as usize)
        .ok_or_else(|| anyhow::anyhow!("struct array element on a non-value-struct array"))?;
    Ok((bytes, refs, layout, arr.element_type().arc()))
}

pub fn as_struct_ref(v: &Value, what: &str) -> Result<(u32, u32)> {
    match v {
        Value::StructRef { idx, frame_id } => Ok((*idx, *frame_id)),
        other => bail!("{what}: expected a struct value (StructRef), got {other:?}"),
    }
}
