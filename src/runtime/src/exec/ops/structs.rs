//! Engine-neutral struct allocation, copying and checked symbolic field access.
use crate::exec::call::transient_arena::StructView;
use crate::object as ty;
use crate::object::{Value, StructTypeLayout};
use crate::metadata::Module;
use crate::vm::VmContext;
use crate::exec::ops::object::struct_leaf::as_struct_ref;
use anyhow::Result;
use std::sync::Arc;

#[path = "struct_relayout.rs"]
mod relayout;
pub(crate) use relayout::{adapt_imported_struct, erased_layout, erased_payload};

/// Allocate a zeroed struct blob stamped with the current activation id.
pub fn struct_alloc_val(
    ctx: &VmContext, frame_id: u32, type_name: &str, size: u32,
) -> Value {
    let layout = resolve_layout(ctx, type_name, size);
    ctx.exec().alloc_struct(frame_id, Arc::from(type_name), layout)
}


/// Allocate a method type argument's concrete blob layout in the current frame.
pub fn method_struct_alloc(
    ctx: &VmContext, module: &Module, frame_id: u32, arguments: &[String], param_index: usize,
) -> Result<Value, Value> {
    let missing = |message| crate::exec::exception::make_missing_symbol_exception(ctx, module, message);
    let name = arguments.get(param_index)
        .ok_or_else(|| missing(format!("missing method type argument in slot {param_index}")))?;
    let descriptor = ctx.loader().try_lookup_type(name)
        .ok_or_else(|| missing(format!("unknown struct type `{name}` for method type argument {param_index}")))?;
    let layout = descriptor.struct_layout().filter(|_| descriptor.is_struct())
        .ok_or_else(|| missing(format!("method type argument `{name}` has no struct layout")))?;
    Ok(ctx.exec().alloc_struct(frame_id, Arc::from(descriptor.name.as_str()), layout))
}

/// Use the loaded struct layout, or the existing size-only native fallback.
pub(crate) fn resolve_layout(ctx: &VmContext, type_name: &str, size: u32) -> Arc<StructTypeLayout> {
    if let Some(td) = ctx.loader().try_lookup_type(type_name) {
        if let Some(layout) = td.struct_layout() {
            return layout;
        }
    }
    Arc::new(StructTypeLayout { float_field_ranges: Default::default(),
        size: size as usize,
        ref_offsets: Box::new([]),
        ref_kinds: Box::new([]),
        // 兜底路径没有类型元数据 ⇒ 无字段表（symbolic-struct-field-access P0）。
        fields: Box::new([]),
    })
}


/// Copy the boxed struct into independent current-frame arena storage.
pub fn unbox_struct(
    ctx: &VmContext, frame_id: u32, gc: &crate::object::ObjRef,
) -> Result<Value> {
    // add-boxed-struct-identity (P4b, 路 B2): the box is a shared struct-typed
    // `ScriptObject` — snapshot its blob (`struct_bytes`/`struct_refs`) into a fresh
    // current-frame arena `StructRef` (value-semantics unbox: the arena copy is
    // independent of the box).
    let (type_name, bytes, refs) = crate::object::snapshot_struct_box(gc);
    let layout = resolve_layout(ctx, &type_name, bytes.len() as u32);
    Ok(alloc_from_snapshot(ctx, frame_id, type_name, layout, &bytes, &refs))
}


/// Snapshot a transient struct view into current-frame storage by value.
pub fn copy_struct_view_out(ctx: &VmContext, frame_id: u32, idx: u32, owner_frame: u32) -> Result<Value> {
    let (src_bytes, src_refs, layout, tname) = snapshot_struct_view(ctx, idx, owner_frame)?;
    Ok(alloc_from_snapshot(ctx, frame_id, tname, layout, &src_bytes, &src_refs))
}

fn alloc_from_snapshot(
    ctx: &VmContext, frame_id: u32, type_name: Arc<str>, layout: Arc<StructTypeLayout>,
    bytes: &[u8], refs: &[Value],
) -> Value {
    ctx.exec().alloc_initialized_struct(frame_id, type_name, layout, |out_bytes, out_refs| {
        let byte_count = bytes.len().min(out_bytes.len());
        out_bytes[..byte_count].copy_from_slice(&bytes[..byte_count]);
        let ref_count = refs.len().min(out_refs.len());
        out_refs[..ref_count].copy_from_slice(&refs[..ref_count]);
    })
}


/// Copy between existing arena blobs with their original frame-id checks.
pub fn struct_copy_val(
    ctx: &VmContext, dst_val: &Value, src_val: &Value, size: u32,
) -> Result<()> {
    let dst = as_struct_ref(dst_val, "StructCopy dst")?;
    copy_struct_source(ctx, dst, src_val, Some(size as usize), "StructCopy src")
}


/// Copy a generic value between existing slots, including an ancestor frame's return slot.
pub fn struct_copy_dynamic(
    ctx: &VmContext, module: &Module, dst: &Value, src: &Value,
) -> Result<(), Value> {
    let copy = || -> Result<()> {
        let dst = as_struct_ref(dst, "StructCopyDynamic dst")?;
        copy_struct_source(ctx, dst, src, None, "StructCopyDynamic src")
    };
    copy().map_err(|err| {
        let message = err.to_string();
        crate::exec::exception::make_stdlib_exception(ctx, module, "Std.InvalidOperationException", message.clone())
            .unwrap_or_else(|_| Value::Str(message.into()))
    })
}

/// A receiver can be a view into its array. Materialize a value when the
/// method copies or returns that receiver.
fn copy_struct_source(
    ctx: &VmContext, dst: (u32, u32), src: &Value, size: Option<usize>, op: &str,
) -> Result<()> {
    let Value::StructRefHeap { idx, frame_id } = src else {
        return ctx.exec().copy_struct_slots(dst, as_struct_ref(src, op)?, size);
    };
    let (bytes, refs, layout, name) = snapshot_struct_view(ctx, *idx, *frame_id)?;
    ctx.exec().with_struct_slot_mut(dst.0, dst.1, |slot| {
        if bytes.len() != slot.bytes.len() || refs.len() != slot.refs.len()
            || size.is_some_and(|size| size != slot.bytes.len())
            || (size.is_none() && (name != slot.type_name || layout != slot.layout))
        {
            anyhow::bail!("{op}: incompatible array element `{name}` and destination `{}`", slot.type_name);
        }
        slot.bytes.copy_from_slice(&bytes);
        slot.refs.copy_from_slice(&refs);
        Ok(())
    })?
}


/// Resolve a declared field spelling as-is, then under the owner namespace.
fn qualify_field_type(ctx: &VmContext, tag: &str, owner_fq: &str) -> Option<std::sync::Arc<str>> {
    // 已经能直接查到（本来就是 FQ、或是基元）⇒ 原样。
    if ctx.loader().try_lookup_type(tag).is_some() {
        return Some(tag.into());
    }
    // ⚠️ 声明拼写里的**空白**：`Loc<P2, long>`（逗号后有空格）对应的注册表键是
    // `Demo.Loc<P2,long>`（无空格）。类型名里不存在有意义的空格，所以去掉全部空格是安全的
    // ——这是第三处「同一个类型、两种拼写」，前两处是「短名 vs FQ」与「实参限定与否」。
    let tag: std::borrow::Cow<str> = if tag.contains(' ') {
        std::borrow::Cow::Owned(tag.replace(' ', ""))
    } else {
        std::borrow::Cow::Borrowed(tag)
    };
    let tag: &str = &tag;
    if ctx.loader().try_lookup_type(tag).is_some() {
        return Some(tag.into());
    }
    // owner 的命名空间 = 其**基名**（去掉泛型实参）里最后一个 `.` 之前的部分。
    let base_end = owner_fq.find('<').unwrap_or(owner_fq.len());
    let ns = owner_fq[..base_end].rfind('.')?;
    let ns = &owner_fq[..ns];
    // 只限定基名：`Pair<int,long>` → `Demo.Pair<int,long>`。
    let tag_base_end = tag.find('<').unwrap_or(tag.len());
    let candidate = format!("{ns}.{}{}", &tag[..tag_base_end], &tag[tag_base_end..]);
    ctx.loader().try_lookup_type(&candidate).map(|_| candidate.as_str().into())
}


/// Resolve field indices against the root layout; later levels are inline structs.
pub fn resolve_field_path(ctx: &VmContext, root_type: &str, path: &[u16]) -> Result<u32> {
    debug_assert!(!path.is_empty(), "decoder rejects depth 0");
    Ok(resolve_struct_path(ctx, root_type, path, false)?.0)
}

fn resolve_struct_path(
    ctx: &VmContext, root_type: &str, path: &[u16], project: bool,
) -> Result<(u32, Arc<str>)> {
    let mut off: u32 = 0;
    let mut cur: std::sync::Arc<str> = root_type.into();
    for (level, &idx) in path.iter().enumerate() {
        let td = ctx.loader().try_lookup_type(&cur).ok_or_else(|| {
            anyhow::anyhow!(
                "struct field path: type `{cur}` (level {level} of `{root_type}`) is not loaded"
            )
        })?;
        let i = idx as usize;
        // 第 1 级且 root 是 class ⇒ 对象合成布局；其余一律 blob 布局。
        let step = if level == 0 && !td.is_struct() {
            let col = td.composed_object_layout().ok_or_else(|| {
                anyhow::anyhow!("struct field path: class `{cur}` has no composed object layout")
            })?;
            col.field_offsets.get(i).copied().ok_or_else(|| {
                anyhow::anyhow!(
                    "struct field path: field index {i} out of range for class `{cur}` \
                     ({} field(s))",
                    col.field_offsets.len()
                )
            })?
        } else {
            let sl = td.struct_layout().ok_or_else(|| {
                anyhow::anyhow!("struct field path: type `{cur}` has no struct layout")
            })?;
            sl.field_offset(i).ok_or_else(|| {
                anyhow::anyhow!(
                    "struct field path: field index {i} out of range for struct `{cur}` \
                     ({} field(s))",
                    sl.field_count()
                )
            })?
        };
        off += step;
        // 下一跳的类型名走 `TypeDesc.fields[i].type_tag` —— 它与上面那张偏移表
        // **同序平行**（zbc 1.45 的约定，载入期有 `debug_assert` 守着，见 T1）。
        if project || level + 1 < path.len() {
            let next = td.fields.get(i).ok_or_else(|| {
                anyhow::anyhow!(
                    "struct field path: `{cur}` has no field #{i} to continue the path through \
                     — the offset table and `fields` are out of step"
                )
            })?;
            let tag = next.type_tag.clone();
            cur = qualify_field_type(ctx, &tag, &cur).ok_or_else(|| {
                anyhow::anyhow!(
                    "struct field path: field #{i} of `{cur}` is declared `{tag}`, which resolves \
                     to no loaded type (tried it as-is and qualified with `{cur}`'s namespace)"
                )
            })?;
        }
    }
    Ok((off, cur))
}

pub fn project_inline_struct(
    ctx: &VmContext, owner: &Value, root: &str, path: &[u16],
) -> Result<Value> {
    let Some(&first) = path.first() else { anyhow::bail!("empty struct field path") };
    let root_td = ctx.loader().try_lookup_type(root)
        .filter(|td| !td.is_struct())
        .ok_or_else(|| anyhow::anyhow!("unknown inline struct owner `{root}`"))?;
    let field = root_td.fields.get(first as usize)
        .ok_or_else(|| anyhow::anyhow!("inline struct field index {first} outside `{root}`"))?;
    let (offset, type_name) = resolve_struct_path_on_owner(ctx, root, path, owner, true)?;
    let descriptor = ctx.loader().try_lookup_type(&type_name)
        .filter(|td| td.is_struct())
        .ok_or_else(|| anyhow::anyhow!("inline field is not a struct: `{type_name}`"))?;
    let layout = descriptor.struct_layout_ref()
        .ok_or_else(|| anyhow::anyhow!("inline struct `{type_name}` has no layout"))?;
    let validate = |object: &ty::ScriptObject| -> Result<()> {
        let access = object.type_desc.field_index.get(&field.name)
            .and_then(|&index| object.field_access_of(index))
            .filter(|access| access.cell == ty::FieldCell::Struct)
            .ok_or_else(|| anyhow::anyhow!("inline struct field `{}` is absent on the receiver", field.name))?;
        let end = (offset as usize).checked_add(layout.size);
        if offset < access.offset || end.is_none_or(|end| end > access.offset as usize + access.width as usize) {
            anyhow::bail!("inline struct `{type_name}` exceeds its field's storage");
        }
        Ok(())
    };
    match owner {
        Value::Object(object) => validate(&object.borrow())?,
        Value::StackObject { idx, frame_id } => ctx.exec().with_stack_object(*idx, *frame_id, validate)??,
        _ => anyhow::bail!("invalid owner for inline struct `{type_name}`"),
    }
    Ok(ctx.exec().alloc_struct_view(
        ctx.exec().next_frame_id(), StructView::Inline { owner: *owner, offset, descriptor },
    ))
}

pub(crate) fn struct_view_type_name(ctx: &VmContext, idx: u32, frame_id: u32) -> Result<Arc<str>> {
    Ok(match ctx.exec().struct_view(idx, frame_id)? {
        StructView::Element(element) => element.arr.borrow().element_type().arc(),
        StructView::Inline { descriptor, .. } => Arc::from(descriptor.name.as_str()),
    })
}

pub fn snapshot_struct_view(
    ctx: &VmContext, idx: u32, frame_id: u32,
) -> Result<(Vec<u8>, Vec<Value>, Arc<StructTypeLayout>, Arc<str>)> {
    match ctx.exec().struct_view(idx, frame_id)? {
        StructView::Element(element) => ty::snapshot_struct_element(&element),
        StructView::Inline { owner, offset, descriptor } => {
            let layout = descriptor.struct_layout()
                .ok_or_else(|| anyhow::anyhow!("inline struct `{}` has no layout", descriptor.name))?;
            let snapshot = |object: &ty::ScriptObject| {
                object.read_blob(offset as usize, layout.size, &layout.ref_offsets)
            };
            let (bytes, refs) = match owner {
                Value::Object(object) => snapshot(&object.borrow()),
                Value::StackObject { idx, frame_id } => ctx.exec().with_stack_object(idx, frame_id, snapshot)?,
                _ => anyhow::bail!("invalid inline struct owner"),
            };
            Ok((bytes, refs, layout, Arc::from(descriptor.name.as_str())))
        }
    }
}


/// Reject a class/blob offset-space mismatch before calculating an offset.
pub(crate) fn check_base_space(ctx: &VmContext, root_type: &str, base_val: &Value, who: &str) -> Result<()> {
    let root_is_struct = match ctx.loader().try_lookup_type(root_type) {
        Some(td) => td.is_struct(),
        // 类型还没加载：路径解析那一步会给出更准确的诊断，这里不抢着报。
        None => return Ok(()),
    };
    let base_is_struct_space = match base_val {
        Value::StructRef { .. } | Value::StructRefHeap { .. } => Some(true),
        Value::Object(_) | Value::StackObject { .. } => Some(false),
        // `BoxedStruct` 走的是 struct 空间，但它由 `Value::Object` 承载（装箱的 struct
        // 是个 struct 类型的 `ScriptObject`）——由 root_type 说了算，不在这里判。
        _ => None,
    };
    if let Some(base_is_struct) = base_is_struct_space {
        // 装箱 struct：base 是 Object 但 root 是 struct —— 合法，跳过。
        let boxed_struct = root_is_struct && !base_is_struct && matches!(base_val, Value::Object(_));
        if !boxed_struct && root_is_struct != base_is_struct {
            anyhow::bail!(
                "{who}: the instruction says the path is rooted at `{root_type}` (a {}), but the \
                 base register holds {base_val:?} — the two disagree about which offset \
                 numbering space applies (blob-relative vs composed-object-relative), so the \
                 resolved offset would be measured from the wrong origin.",
                if root_is_struct { "value struct" } else { "class" },
            );
        }
    }
    Ok(())
}


/// Check the numbering space before resolving; diagnostic order is semantic.
#[inline]
pub fn resolve_for_access(
    ctx: &VmContext, root_type: &str, path: &[u16], base_val: &Value, who: &str,
) -> Result<u32> {
    check_base_space(ctx, root_type, base_val, who)?;
    Ok(resolve_struct_path_on_owner(ctx, root_type, path, base_val, false)?.0)
}

fn resolve_struct_path_on_owner(
    ctx: &VmContext, root_type: &str, path: &[u16], base_val: &Value, project: bool,
) -> Result<(u32, Arc<str>)> {
    let declared = resolve_struct_path(ctx, root_type, path, project);
    if declared.is_ok() || (!project && path.len() < 2) {
        return declared;
    }

    // Shared class bodies name the generic definition. An intermediate
    // inline field can need the receiver's closed descriptor to name its layout.
    let actual_name = match base_val {
        Value::Object(object) => object.type_desc().name.clone(),
        Value::StackObject { idx, frame_id } => ctx.exec()
            .with_stack_object(*idx, *frame_id, |object| object.type_desc.name.clone())?,
        _ => return declared,
    };
    if crate::object::TypeDesc::erased_definition_name(&actual_name) != Some(root_type) {
        return declared;
    }
    resolve_struct_path(ctx, &actual_name, path, project).or(declared)
}


