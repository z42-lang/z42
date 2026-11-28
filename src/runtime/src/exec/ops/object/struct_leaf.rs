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

use crate::exec::call::transient_arena::StructView;
use crate::exec::ops::structs::{erased_layout, erased_payload, snapshot_struct_view};
use crate::object::types::named_struct_layout;
use crate::object::{
    decode_prim, encode_prim, is_ref_tag, prim_width, StructTypeLayout, Value,
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
            ctx.exec().with_stack_object(*idx, *frame_id, |obj| obj.struct_leaf(byte_off, kind))?
        }
        Value::StructRefHeap { idx, frame_id } => match ctx.exec().struct_view(*idx, *frame_id)? {
            StructView::Element(e) => {
                let arr = e.arr.borrow();
                let i = e.index as usize;
                if is_ref_tag(kind) {
                    arr.struct_elem_ref(i, byte_off)
                } else {
                    arr.struct_elem_prim(i, byte_off, prim_width(kind)?, kind)
                }
            }
            StructView::Inline { owner, offset, descriptor } =>
                struct_field_get_val(ctx, &owner, inline_leaf_offset(offset, byte_off, kind, &descriptor)?, kind),
        },
        _ => {
            let (idx, fid) = as_struct_ref(base_val, "StructFieldGetPrim base")?;
            if is_ref_tag(kind) {
                ctx.exec().struct_ref_leaf(idx, fid, byte_off)
            } else {
                let off = byte_off as usize;
                let w = prim_width(kind)?;
                ctx.exec().with_struct_bytes(idx, fid, |bytes| decode_prim(bytes, off, w, kind))?
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
            ctx.exec().with_stack_object(*idx, *frame_id, |obj| {
                obj.store_struct_leaf(byte_off, kind, v).map(|_| ())
            })?
        }
        Value::StructRefHeap { idx, frame_id } => match ctx.exec().struct_view(*idx, *frame_id)? {
            StructView::Element(e) => crate::object::store_struct_element_leaf(ctx.heap(), &e, byte_off, kind, v),
            StructView::Inline { owner, offset, descriptor } =>
                struct_field_set_val(ctx, &owner, inline_leaf_offset(offset, byte_off, kind, &descriptor)?, kind, v),
        },
        _ => {
            let (idx, fid) = as_struct_ref(base_val, "StructFieldSetPrim base")?;
            if is_ref_tag(kind) {
                return ctx.exec().set_struct_ref_leaf(idx, fid, byte_off, *v);
            }
            let off = byte_off as usize;
            let w = prim_width(kind)?;
            ctx.exec().with_struct_bytes_mut(idx, fid, |bytes| encode_prim(bytes, off, w, kind, v))?
        }
    }
}

fn inline_leaf_offset(offset: u32, leaf: u32, kind: u8, td: &crate::object::TypeDesc) -> Result<u32> {
    let width = if is_ref_tag(kind) { 8 } else { prim_width(kind)? };
    if td.struct_layout_ref().is_none_or(|layout| {
        leaf as usize + width > layout.size || layout.ref_index(leaf).is_some() != is_ref_tag(kind)
    }) {
        bail!("invalid inline struct leaf at byte offset {leaf} for `{}`", td.name);
    }
    offset.checked_add(leaf).ok_or_else(|| anyhow::anyhow!("inline struct leaf offset overflow"))
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

/// Default field hash across frame, array-element, and boxed struct representations.
pub fn struct_value_hash(ctx: &VmContext, value: &Value) -> Result<Option<i64>> {
    use crate::object::value::boxed_hash::hash_struct_payload;

    let hash = match &value.unproject_interface() {
        Value::StructRef { idx, frame_id } => hash_frame_struct(ctx, *idx, *frame_id)?,
        Value::BoxedStruct(boxed) => {
            const STACK_HASH_BYTES: usize = 64;
            let td = boxed.type_desc();
            let layout = td.struct_layout_ref();
            let object = boxed.borrow();
            let cells = object.cells();
            if cells.len() <= STACK_HASH_BYTES
                && layout.is_some_and(|layout| layout.ref_offsets.is_empty())
            {
                let mut scratch = [0; STACK_HASH_BYTES];
                let bytes = &mut scratch[..cells.len()];
                cells.copy_out(0, bytes);
                hash_snapshot(ctx, &td.name, bytes, &[], layout)?
            } else {
                let (mut bytes, refs) = object.boxed_blob();
                hash_snapshot(ctx, &td.name, &mut bytes, &refs, layout)?
            }
        }
        Value::StructRefHeap { idx, frame_id } => {
            let (mut bytes, refs, layout, name) = snapshot_struct_view(ctx, *idx, *frame_id)?;
            hash_snapshot(ctx, &name, &mut bytes, &refs, Some(&layout))?
        }
        Value::Object(o) if o.type_desc().is_zero_payload_struct() =>
            hash_struct_payload(&[], &[]),
        _ => return Ok(None),
    };
    Ok(Some(hash))
}

fn hash_frame_struct(ctx: &VmContext, idx: u32, frame_id: u32) -> Result<i64> {
    use crate::object::value::boxed_hash::hash_struct_with_floats;
    use std::ops::ControlFlow;

    let plan = ctx.exec().with_struct_slot(idx, frame_id, |slot| {
        if let Some(fields) = slot.layout.float_field_ranges.get().filter(|_| !slot.type_name.contains('<')) {
            return ControlFlow::Break(hash_struct_with_floats(&slot.bytes, &slot.refs, fields));
        }
        ControlFlow::Continue((slot.type_name.clone(), slot.layout.clone()))
    })?;
    let (name, layout) = match plan {
        ControlFlow::Break(hash) => return hash,
        ControlFlow::Continue(layout) => layout,
    };

    if erased_layout(ctx, &name, &layout).is_some() {
        let (_, mut bytes, refs) = ctx.exec().snapshot_struct(idx, frame_id)?;
        return hash_snapshot(ctx, &name, &mut bytes, &refs, Some(&layout));
    }
    // Type resolution can run loader events. Prepare the plan outside the arena lock.
    let fields = named_struct_layout::float_field_ranges(&layout, &|n| ctx.loader().try_lookup_type(n), &name)?;
    ctx.exec().with_struct_slot(idx, frame_id, |slot| {
        hash_struct_with_floats(&slot.bytes, &slot.refs, fields)
    })?
}

fn hash_snapshot(
    ctx: &VmContext, name: &str, bytes: &mut [u8], refs: &[Value],
    layout: Option<&StructTypeLayout>,
) -> Result<i64> {
    use crate::object::value::boxed_hash::{hash_struct_payload, normalize_signed_zero};
    if let Some(payload) = erased_payload(ctx, name, bytes, refs)? {
        let fields = named_struct_layout::float_field_ranges(
            &payload.layout, &|n| ctx.loader().try_lookup_type(n), payload.name,
        )?;
        return crate::object::value::boxed_hash::hash_struct_with_floats(&payload.bytes, &payload.refs, fields);
    }
    if let Some(layout) = layout {
        let fields = named_struct_layout::float_field_ranges(
            layout, &|n| ctx.loader().try_lookup_type(n), name,
        )?;
        normalize_signed_zero(bytes, fields)?;
    }
    Ok(hash_struct_payload(bytes, refs))
}

struct StructPayload {
    identity: String,
    erased_storage: Option<String>,
    bytes: Vec<u8>,
    refs: Vec<Value>,
}

impl StructPayload {
    fn storage(&self) -> &str {
        self.erased_storage.as_deref().unwrap_or(&self.identity)
    }
}

fn struct_payload(
    ctx: &VmContext, v: &Value,
) -> Result<Option<StructPayload>> {
    match v {
        Value::BoxedStruct(gc) => {
            let o = gc.borrow();
            let (bytes, refs) = o.boxed_blob();
            let storage = &o.type_desc.name;
            let (identity, erased_storage) = if o.type_args().is_empty() {
                (storage.clone(), None)
            } else {
                (crate::zpkg::index::type_names::constructed(storage, o.type_args()), Some(storage.clone()))
            };
            Ok(Some(StructPayload { identity, erased_storage, bytes, refs }))
        }
        Value::StructRef { idx, frame_id } => {
            let (name, bytes, refs) = ctx.exec().snapshot_struct(*idx, *frame_id)?;
            Ok(Some(StructPayload { identity: name.to_string(), erased_storage: None, bytes, refs }))
        }
        Value::StructRefHeap { idx, frame_id } => {
            let (bytes, refs, _layout, type_name) = snapshot_struct_view(ctx, *idx, *frame_id)?;
            Ok(Some(StructPayload { identity: type_name.to_string(), erased_storage: None, bytes, refs }))
        }
        Value::Object(o) if o.type_desc().is_zero_payload_struct() => {
            let td = o.type_desc();
            let args = o.type_args();
            let (identity, erased_storage) = if args.is_empty() || !td.type_args().is_empty() {
                (td.name.clone(), None)
            } else {
                (crate::zpkg::index::type_names::constructed(&td.name, args), Some(td.name.clone()))
            };
            Ok(Some(StructPayload { identity, erased_storage, bytes: Vec::new(), refs: Vec::new() }))
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
    struct_payload_eq(ctx, a, b, false)
}

/// The caller established the exact closed type of both operands. A frame slot's
/// storage name may use either the erased definition or a constructed descriptor.
pub(crate) fn same_typed_struct_value_eq(ctx: &VmContext, a: &Value, b: &Value) -> Result<bool> {
    struct_payload_eq(ctx, a, b, true)
}

fn struct_payload_eq(ctx: &VmContext, a: &Value, b: &Value, same_type: bool) -> Result<bool> {
    if let (
        Value::StructRef { idx: a_idx, frame_id: a_frame },
        Value::StructRef { idx: b_idx, frame_id: b_frame },
    ) = (a, b) {
        return ctx.exec().struct_slots_equal((*a_idx, *a_frame), (*b_idx, *b_frame), same_type);
    }

    let (pa, pb) = match (
        struct_payload(ctx, &a.unproject_interface())?,
        struct_payload(ctx, &b.unproject_interface())?,
    ) {
        (Some(pa), Some(pb)) => (pa, pb),
        _ => return Ok(false),
    };
    payloads_equal(ctx, &pa, &pb, same_type, false, 0)
}

/// Default equality when invoked directly on the ValueType declaration. Ordinary
/// boxed calls can select the concrete type's compiled field comparison.
pub fn value_type_equals(ctx: &VmContext, a: &Value, b: &Value) -> Result<bool> {
    value_type_equals_nested(ctx, a, b, 0)
}

fn value_type_equals_nested(ctx: &VmContext, a: &Value, b: &Value, depth: usize) -> Result<bool> {
    if depth > 64 { bail!("struct equality: layout conversion nesting too deep"); }
    let a = a.unproject_interface();
    let b = b.unproject_interface();
    if matches!(&a, Value::BoxedStruct(o) if o.borrow().boxed_scalar().is_some())
        || matches!(a, Value::I64(_) | Value::F64(_) | Value::Bool(_) | Value::Char(_))
    {
        return Ok(a == b);
    }
    let (Some(lhs), Some(rhs)) =
        (struct_payload(ctx, &a)?, struct_payload(ctx, &b)?)
    else {
        return Ok(false);
    };
    payloads_equal(ctx, &lhs, &rhs, false, true, depth)
}

fn payloads_equal(
    ctx: &VmContext, lhs: &StructPayload, rhs: &StructPayload, same_type: bool, scalar_fields: bool,
    depth: usize,
) -> Result<bool> {
    if !same_type && lhs.identity != rhs.identity {
        return Ok(false);
    }
    let lhs_storage = lhs.storage();
    let rhs_storage = rhs.storage();
    let (lhs_erased, rhs_erased) = if lhs_storage == rhs_storage {
        (None, None)
    } else {
        (
            erased_payload(ctx, lhs_storage, &lhs.bytes, &lhs.refs)?,
            erased_payload(ctx, rhs_storage, &rhs.bytes, &rhs.refs)?,
        )
    };
    let (name, left) = match &lhs_erased {
        Some(payload) => (payload.name, (payload.bytes.as_slice(), payload.refs.as_slice())),
        None => (lhs_storage, (lhs.bytes.as_slice(), lhs.refs.as_slice())),
    };
    let right = match &rhs_erased {
        Some(payload) => (payload.bytes.as_slice(), payload.refs.as_slice()),
        None => (rhs.bytes.as_slice(), rhs.refs.as_slice()),
    };
    let generic_refs = !left.1.is_empty() && (lhs.identity.contains('<') || rhs.identity.contains('<'));
    equal_payload_fields(ctx, name, left, right, scalar_fields, generic_refs, depth)
}

fn equal_payload_fields(
    ctx: &VmContext, name: &str, lhs: (&[u8], &[Value]), rhs: (&[u8], &[Value]),
    scalar_fields: bool, generic_refs: bool, depth: usize,
) -> Result<bool> {
    let (lhs, lhs_refs) = lhs;
    let (rhs, rhs_refs) = rhs;
    if lhs.len() != rhs.len() || lhs_refs.len() != rhs_refs.len() {
        return Ok(false);
    }
    for (left, right) in lhs_refs.iter().zip(rhs_refs) {
        if left == right { continue; }
        if !generic_refs { return Ok(false); }
        let equal = match (left.unproject_interface(), right.unproject_interface()) {
            (Value::BoxedStruct(a), Value::BoxedStruct(b))
                if a.type_desc().name != b.type_desc().name =>
                    value_type_equals_nested(ctx, left, right, depth + 1)?,
            (Value::BoxedStruct(a), Value::BoxedStruct(b)) => {
                let left = a.borrow().boxed_scalar();
                left.is_some() && left == b.borrow().boxed_scalar()
            }
            (Value::BoxedStruct(a), Value::F64(b)) | (Value::F64(b), Value::BoxedStruct(a)) =>
                a.borrow().boxed_scalar() == Some(Value::F64(b)),
            (Value::Object(a), Value::Object(b))
            | (Value::Object(a), Value::BoxedStruct(b))
            | (Value::BoxedStruct(a), Value::Object(b))
                if a.type_desc().is_zero_payload_struct() && b.type_desc().is_zero_payload_struct() =>
                    value_type_equals_nested(ctx, left, right, depth + 1)?,
            _ => false,
        };
        if !equal { return Ok(false); }
    }
    if !scalar_fields || lhs.is_empty() {
        return Ok(lhs == rhs);
    }
    equal_scalar_fields(&|n| ctx.loader().try_lookup_type(n), name, lhs, rhs)
}

fn equal_scalar_fields(
    resolve: &named_struct_layout::TypeResolver<'_>, name: &str, lhs: &[u8], rhs: &[u8],
) -> Result<bool> {
    for field in &named_struct_layout::compute(resolve, name)?.leaves {
        if field.is_ref { continue; }
        let start = field.byte_off as usize;
        let end = start + field.size as usize;
        let a = lhs.get(start..end).ok_or_else(|| anyhow::anyhow!("invalid field layout for {name}"))?;
        let b = rhs.get(start..end).ok_or_else(|| anyhow::anyhow!("invalid field layout for {name}"))?;
        let equal = if field.is_struct {
            equal_scalar_fields(resolve, &field.type_name, a, b)?
        } else {
            decode_prim(a, 0, a.len(), field.tag)? == decode_prim(b, 0, b.len(), field.tag)?
        };
        if !equal { return Ok(false); }
    }
    Ok(true)
}
