//! Engine-neutral struct allocation, copying and checked symbolic field access.
use crate::object as ty;
use crate::object::{Value, StructTypeLayout};
use crate::vm::context::VmContext;
use crate::objops::struct_leaf::as_struct_ref;
use anyhow::Result;
use std::sync::Arc;

/// Allocate a zeroed struct blob stamped with the current activation id.
pub(crate) fn struct_alloc_val(
    ctx: &VmContext, frame_id: u32, type_name: &str, size: u32,
) -> Value {
    let layout = resolve_layout(ctx, type_name, size);
    let idx = ctx.struct_alloc(frame_id, Arc::from(type_name), layout);
    Value::StructRef { idx, frame_id }
}


/// Use the loaded struct layout, or the existing size-only native fallback.
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
        // 兜底路径没有类型元数据 ⇒ 无字段表（symbolic-struct-field-access P0）。
        fields: Box::new([]),
    })
}


/// Copy the boxed struct into independent current-frame arena storage.
pub(crate) fn unbox_struct(
    ctx: &VmContext, frame_id: u32, gc: &crate::object::ObjRef,
) -> Result<Value> {
    // add-boxed-struct-identity (P4b, 路 B2): the box is a shared struct-typed
    // `ScriptObject` — snapshot its blob (`struct_bytes`/`struct_refs`) into a fresh
    // current-frame arena `StructRef` (value-semantics unbox: the arena copy is
    // independent of the box).
    let (type_name, bytes, refs) = crate::objops::struct_leaf::snapshot_box(gc);
    let layout = resolve_layout(ctx, &type_name, bytes.len() as u32);
    let idx = ctx.struct_alloc(frame_id, type_name, layout);
    ctx.exec.struct_arena.lock().with_mut(idx, frame_id, |s| {
        let n = bytes.len().min(s.bytes.len());
        s.bytes[..n].copy_from_slice(&bytes[..n]);
        let rn = refs.len().min(s.refs.len());
        s.refs[..rn].clone_from_slice(&refs[..rn]);
    })?;
    Ok(Value::StructRef { idx, frame_id })
}


/// Snapshot a struct array element into current-frame storage by value.
pub(crate) fn copy_array_elem_out(ctx: &VmContext, frame_id: u32, e: &ty::StructArrayElem) -> Result<Value> {
    let (src_bytes, src_refs, layout, tname) = crate::objops::struct_leaf::snapshot_elem(e)?;
    let idx = ctx.struct_alloc(frame_id, tname, layout);
    ctx.exec.struct_arena.lock().with_mut(idx, frame_id, |s| {
        let n = src_bytes.len().min(s.bytes.len());
        s.bytes[..n].copy_from_slice(&src_bytes[..n]);
        let rn = src_refs.len().min(s.refs.len());
        s.refs[..rn].clone_from_slice(&src_refs[..rn]);
    })?;
    Ok(Value::StructRef { idx, frame_id })
}


/// Copy between existing arena blobs with their original frame-id checks.
pub(crate) fn struct_copy_val(
    ctx: &VmContext, dst_val: &Value, src_val: &Value, size: u32,
) -> Result<()> {
    let (d_idx, d_fid) = as_struct_ref(dst_val, "StructCopy dst")?;
    let (s_idx, s_fid) = as_struct_ref(src_val, "StructCopy src")?;
    ctx.exec.struct_arena.lock().copy_into(d_idx, d_fid, s_idx, s_fid, size as usize)
}


/// Resolve a declared field spelling as-is, then under the owner namespace.
fn qualify_field_type(ctx: &VmContext, tag: &str, owner_fq: &str) -> Option<std::sync::Arc<str>> {
    // 已经能直接查到（本来就是 FQ、或是基元）⇒ 原样。
    if ctx.try_lookup_type(tag).is_some() {
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
    if ctx.try_lookup_type(tag).is_some() {
        return Some(tag.into());
    }
    // owner 的命名空间 = 其**基名**（去掉泛型实参）里最后一个 `.` 之前的部分。
    let base_end = owner_fq.find('<').unwrap_or(owner_fq.len());
    let ns = owner_fq[..base_end].rfind('.')?;
    let ns = &owner_fq[..ns];
    // 只限定基名：`Pair<int,long>` → `Demo.Pair<int,long>`。
    let tag_base_end = tag.find('<').unwrap_or(tag.len());
    let candidate = format!("{ns}.{}{}", &tag[..tag_base_end], &tag[tag_base_end..]);
    ctx.try_lookup_type(&candidate).map(|_| candidate.as_str().into())
}


/// Resolve field indices against the root layout; later levels are inline structs.
pub(crate) fn resolve_field_path(ctx: &VmContext, root_type: &str, path: &[u16]) -> Result<u32> {
    debug_assert!(!path.is_empty(), "decoder rejects depth 0");
    let mut off: u32 = 0;
    let mut cur: std::sync::Arc<str> = root_type.into();
    for (level, &idx) in path.iter().enumerate() {
        let td = ctx.try_lookup_type(&cur).ok_or_else(|| {
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
        if level + 1 < path.len() {
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
    Ok(off)
}


/// Reject a class/blob offset-space mismatch before calculating an offset.
pub(crate) fn check_base_space(ctx: &VmContext, root_type: &str, base_val: &Value, who: &str) -> Result<()> {
    let root_is_struct = match ctx.try_lookup_type(root_type) {
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
pub(crate) fn resolve_for_access(
    ctx: &VmContext, root_type: &str, path: &[u16], base_val: &Value, who: &str,
) -> Result<u32> {
    check_base_space(ctx, root_type, base_val, who)?;
    resolve_field_path(ctx, root_type, path)
}


