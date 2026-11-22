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
    decode_prim, encode_prim, is_ref_tag, prim_width, Value,
};
use crate::vm::VmContext;
use anyhow::{bail, Result};

/// Copy a struct blob into a fresh heap object with boxed identity.
/// A fresh box is unreachable until returned, so its initial writes need no barrier.
pub fn box_struct_blob(
    ctx: &VmContext, type_name: &str, bytes: &[u8], refs: &[Value],
) -> Result<Value> {
    let td = ctx.loader().try_lookup_type(type_name).ok_or_else(|| {
        anyhow::anyhow!("__box_struct: unknown struct type `{type_name}`")
    })?;
    crate::object::box_struct(ctx.heap(), td, bytes, refs)
}

/// Read a nested field into its own value snapshot.
fn snapshot_struct_leaf(
    ctx: &VmContext,
    o: &crate::object::ScriptObject,
    parent: &named_struct_layout::ComputedLayout,
    leaf: &named_struct_layout::FieldLeaf,
) -> Result<Value> {
    let resolve = |n: &str| ctx.loader().try_lookup_type(n);
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
    let resolve = |n: &str| ctx.loader().try_lookup_type(n);
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
        Value::Object(gc) | Value::BoxedStruct(gc) => gc.borrow().struct_leaf(byte_off, kind),
        // The same, for an object escape analysis put in the stack arena.
        Value::StackObject { idx, frame_id } => {
            ctx.exec.stack_arena.lock().with_obj(*idx, *frame_id, |obj| obj.struct_leaf(byte_off, kind))?
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

/// Frame-agnostic core of `StructFieldSetPrim` — write `v` into the `base_val` struct's leaf
/// at `byte_off` in place. Heap bases route a reference store through the write barrier with
/// the reference that landed (a box, for a value that does not fit 8 B). Shared by interp and
/// the JIT struct helpers.
pub fn struct_field_set_val(
    ctx: &VmContext, base_val: &Value, byte_off: u32, kind: u8, v: &Value,
) -> Result<()> {
    match base_val {
        Value::Object(gc) | Value::BoxedStruct(gc) =>
            crate::object::store_struct_leaf(ctx.heap(), gc, byte_off, kind, v),
        // 🔴 **No write barrier** for a stack object, mirroring `field_set`: it is not a heap
        // slot, its references are kept live by root-scanning the arena every cycle.
        Value::StackObject { idx, frame_id } => {
            ctx.exec.stack_arena.lock().with_obj_mut(*idx, *frame_id, |obj| {
                obj.store_struct_leaf(byte_off, kind, v).map(|_| ())
            })?
        }
        // Leaf write into a struct[] element.
        Value::StructRefHeap { idx, frame_id } => {
            let e = ctx.exec.transient_arena.lock().struct_elem(*idx, *frame_id)?;
            crate::object::store_struct_element_leaf(ctx.heap(), &e, byte_off, kind, v)
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

pub fn as_struct_ref(v: &Value, what: &str) -> Result<(u32, u32)> {
    match v {
        Value::StructRef { idx, frame_id } => Ok((*idx, *frame_id)),
        other => bail!("{what}: expected a struct value (StructRef), got {other:?}"),
    }
}

// ── add-value-auto-iequatable：任一表示的 struct 值快照与值相等（引擎无关核心） ──────────
// `__struct_eq` builtin（convert.rs）与装箱 vcall `Equals` 快路（vcall.rs 的
// `VCallTarget::StructValueEq`，interp+JIT 两引擎的 invoke 臂）共用——同一实现保证
// builtin 与 vcall 两条路对同两个值答案一致。

/// 任一表示（frame arena `StructRef` / 堆 `StructRefHeap` 元素视图 / `BoxedStruct`）的
/// struct 值快照 `(类型名, 基元字节, 引用叶子)`。`None` = 不是 struct 值——调用方按
/// 「不是这个 struct」处理（值相等答 false），不报错。
pub fn struct_payload(
    ctx: &VmContext, v: &Value,
) -> Result<Option<(String, Vec<u8>, Vec<Value>)>> {
    match v {
        Value::BoxedStruct(gc) => {
            let o = gc.borrow();
            let (bytes, refs) = o.boxed_blob();
            Ok(Some((o.type_desc.name.to_string(), bytes.to_vec(), refs.to_vec())))
        }
        Value::StructRef { idx, frame_id } => {
            let s = ctx.exec().struct_arena.lock().with(*idx, *frame_id, |s| {
                (s.type_name.to_string(), s.bytes.to_vec(), s.refs.to_vec())
            })?;
            Ok(Some(s))
        }
        Value::StructRefHeap { idx, frame_id } => {
            let e = ctx.exec().transient_arena.lock().struct_elem(*idx, *frame_id)?;
            let (bytes, refs, _layout, type_name) = crate::object::snapshot_struct_element(&e)?;
            Ok(Some((type_name.to_string(), bytes, refs)))
        }
        _ => Ok(None),
    }
}

/// 值相等：同精确类型名 + 基元字节相等 + 引用叶子逐位 `Value` 相等（string 内容 /
/// object 引用——与 `Value::eq` 装箱臂同式）。非 struct 侧（Null/object/string/…）答
/// false——`other is P ? eq : false` 的运行期形态。按位相等 ⇔ 逐叶 `Eq` 仅对**非浮点**叶
/// 成立，故编译器只对 float-free 布局把调用引到这条路（保留名 `Equals$1$synth` /
/// builtin 发射侧的 float-free 谓词）。
pub fn struct_value_eq(ctx: &VmContext, a: &Value, b: &Value) -> Result<bool> {
    let (pa, pb) = match (struct_payload(ctx, a)?, struct_payload(ctx, b)?) {
        (Some(pa), Some(pb)) => (pa, pb),
        _ => return Ok(false),
    };
    Ok(pa.0 == pb.0 && pa.1 == pb.1 && pa.2 == pb.2)
}
