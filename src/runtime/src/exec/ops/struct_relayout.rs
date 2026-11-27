//! Physical layout conversion after a checked, closed struct cast.

use super::alloc_from_snapshot;
use crate::gc::{Heap, RootHandle};
use crate::object::types::named_struct_layout::{self, FieldLeaf};
use crate::object::{StructTypeLayout, Value, decode_prim, encode_prim};
use crate::vm::VmContext;
use crate::zpkg::index::type_names::erased;
use anyhow::{Context, Result, anyhow, ensure};
use std::sync::Arc;

pub(crate) fn adapt_imported_struct(ctx: &VmContext, value: &Value, target: &str) -> Result<Value> {
    let Value::StructRef { idx, frame_id } = *value else {
        ensure!(matches!(value, Value::Null), "expected a struct cast result");
        return Ok(*value);
    };
    if ctx.exec().with_struct_slot(idx, frame_id, |slot| slot.type_name.as_ref() == target)? {
        return Ok(*value);
    }

    let (source_name, bytes, refs) = ctx.exec().snapshot_struct(idx, frame_id)?;
    let source = Blob { layout: load_layout(ctx, &source_name)?, bytes, refs };
    let mut roots = Roots { heap: ctx.heap(), handles: Vec::new() };
    let result = repack(ctx, &source_name, source, target, &mut roots)?;
    Ok(alloc_from_snapshot(ctx, frame_id, target.into(), result.layout, &result.bytes, &result.refs))
}

struct Blob {
    layout: Arc<StructTypeLayout>,
    bytes: Vec<u8>,
    refs: Vec<Value>,
}

impl Blob {
    fn ref_index(&self, offset: u32) -> Result<usize> {
        self.layout.ref_index(offset).ok_or_else(|| anyhow!("missing reference at offset {offset}"))
    }

    fn nested(&self, ctx: &VmContext, field: &FieldLeaf) -> Result<Self> {
        let layout = load_layout(ctx, &field.type_name)?;
        ensure!(layout.size == field.size as usize, "nested struct layout size differs");
        let start = field.byte_off as usize;
        let bytes = self.bytes[start..start + field.size as usize].to_vec();
        let refs = layout.ref_offsets.iter()
            .map(|offset| self.ref_index(field.byte_off + offset).map(|i| self.refs[i]))
            .collect::<Result<_>>()?;
        Ok(Self { layout, bytes, refs })
    }
}

fn load_layout(ctx: &VmContext, name: &str) -> Result<Arc<StructTypeLayout>> {
    ctx.loader().try_lookup_type(name)
        .and_then(|td| td.struct_layout())
        .ok_or_else(|| anyhow!("missing layout for struct `{name}`"))
}

fn repack(
    ctx: &VmContext, source_name: &str, source: Blob, target_name: &str, roots: &mut Roots<'_>,
) -> Result<Blob> {
    ensure!(erased(source_name) == erased(target_name),
        "cannot adapt struct `{source_name}` to unrelated layout `{target_name}`");
    ensure!(source.bytes.len() == source.layout.size && source.refs.len() == source.layout.ref_count(),
        "invalid payload for layout `{source_name}`");
    let layout = load_layout(ctx, target_name)?;
    if *source.layout == *layout {
        return Ok(Blob { layout, ..source });
    }

    let resolve = |name: &str| ctx.loader().try_lookup_type(name);
    let from = named_struct_layout::compute(&resolve, source_name)?;
    let to = named_struct_layout::compute(&resolve, target_name)?;
    from.validate_against(&source.layout, source_name)?;
    to.validate_against(&layout, target_name)?;
    ensure!(from.leaves.len() == to.leaves.len(), "struct layout field counts differ");
    let mut result = Blob {
        bytes: vec![0; layout.size],
        refs: vec![Value::Null; layout.ref_count()],
        layout,
    };
    for (src, dst) in from.leaves.iter().zip(&to.leaves) {
        ensure!(src.name == dst.name, "struct layout field order differs");
        copy_field(ctx, &source, src, &mut result, dst, roots)
            .with_context(|| format!("adapting `{source_name}.{}` to `{target_name}`", src.name))?;
    }
    Ok(result)
}

fn copy_field(
    ctx: &VmContext, source: &Blob, src: &FieldLeaf, target: &mut Blob, dst: &FieldLeaf,
    roots: &mut Roots<'_>,
) -> Result<()> {
    let src_start = src.byte_off as usize;
    let dst_start = dst.byte_off as usize;
    if src.is_struct && dst.is_struct {
        let nested = repack(ctx, &src.type_name, source.nested(ctx, src)?, &dst.type_name, roots)?;
        ensure!(nested.bytes.len() == dst.size as usize, "nested struct layout size differs");
        target.bytes[dst_start..dst_start + dst.size as usize].copy_from_slice(&nested.bytes);
        for (offset, value) in nested.layout.ref_offsets.iter().zip(nested.refs) {
            let index = target.ref_index(dst.byte_off + offset)?;
            target.refs[index] = value;
        }
    } else if dst.is_ref {
        let index = target.ref_index(dst.byte_off)?;
        target.refs[index] = if src.is_struct {
            let nested = source.nested(ctx, src)?;
            let boxed = crate::exec::ops::object::struct_leaf::box_struct_blob(
                ctx, &src.type_name, &nested.bytes, &nested.refs,
            )?;
            roots.handles.push(roots.heap.pin_root(boxed));
            boxed
        } else if src.is_ref {
            source.refs[source.ref_index(src.byte_off)?]
        } else {
            // Erased generic constructor slots store primitive VM values directly.
            decode_prim(&source.bytes, src_start, src.size as usize, src.tag)?
        };
    } else if src.is_ref && !dst.is_struct {
        let value = source.refs[source.ref_index(src.byte_off)?];
        let scalar = match value {
            Value::BoxedStruct(boxed) => boxed.borrow().boxed_scalar()
                .ok_or_else(|| anyhow!("expected a boxed scalar"))?,
            value => value,
        };
        encode_prim(&mut target.bytes, dst_start, dst.size as usize, dst.tag, &scalar)?;
    } else {
        ensure!(!src.is_struct && !dst.is_struct && src.tag == dst.tag && src.size == dst.size,
            "incompatible inline fields");
        target.bytes[dst_start..dst_start + dst.size as usize]
            .copy_from_slice(&source.bytes[src_start..src_start + src.size as usize]);
    }
    Ok(())
}

struct Roots<'a> {
    heap: &'a Heap,
    handles: Vec<RootHandle>,
}

impl Drop for Roots<'_> {
    fn drop(&mut self) {
        for root in self.handles.drain(..) {
            self.heap.unpin_root(root);
        }
    }
}
