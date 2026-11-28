//! Whole-value access to inline union instance fields.
use crate::exec::ops::object::error::{OpError, OpResult};
use crate::exec::ops::object::struct_leaf::as_struct_ref;
use crate::exec::ops::union_heap;
use crate::object::value::union_blob;
use crate::object::{FieldAccess, FieldCell, ObjRef, ScriptObject, Value};
use crate::vm::VmContext;
use crate::zpkg::ir::UnionLayoutDesc;
use anyhow::{ensure, Result};
use std::sync::Arc;

type Storage<'a> = (FieldAccess, &'a str, Option<&'a Arc<UnionLayoutDesc>>);

fn field<'a>(object: &'a ScriptObject, name: &str) -> Option<Storage<'a>> {
    let slot = *object.type_desc.field_index.get(name)?;
    let expected: &str = &object.type_desc.fields.get(slot)?.type_tag;
    if !union_blob::is_storage_name(expected) { return None; }
    let access = object.field_access_of(slot)?;
    if access.cell != FieldCell::Struct || access.width == 0 { return None; }
    let plan = object.type_desc.composed_object_layout_ref()
        .and_then(|layout| layout.union_refs.iter().find(|(offset, _)| *offset == access.offset))
        .map(|(_, plan)| plan);
    Some((access, expected, plan))
}

pub(crate) fn read(
    ctx: &VmContext, owner: &Value, name: &str, target: &Value,
) -> OpResult<Value> {
    let load = |object: &ScriptObject| {
        let (access, expected, plan) = field(object, name)
            .ok_or_else(|| anyhow::anyhow!("unknown inline union field `{name}`"))?;
        let width = access.width as usize;
        let offset = access.offset as usize;
        let cells = object.cells();
        if width <= 8 {
            let mut bits = [0; 8];
            union_blob::read(&cells, offset, &mut bits[..width])?;
            return Ok(Value::I64(i64::from_le_bytes(bits)));
        }
        let (slot, frame) = as_struct_ref(target, "inline union field destination")?;
        ctx.exec().with_struct_slot_mut(slot, frame, |target| {
            ensure!(target.type_name.as_ref() == expected && target.bytes.len() == width,
                "inline union field type mismatch");
            if let Some(plan) = plan {
                union_blob::snapshot(&cells, offset, plan, &mut target.bytes, &mut target.refs)
            } else {
                ensure!(target.refs.is_empty(), "inline union reference layout is missing");
                union_blob::read(&cells, offset, &mut target.bytes)
            }
        })??;
        Ok(Value::Null)
    };
    let result: Result<Value> = match owner {
        Value::Object(object) => load(&object.borrow()),
        Value::StackObject { idx, frame_id } => ctx.exec().with_stack_object(*idx, *frame_id, load)?,
        Value::Null => return Err(OpError::null_field_read(name)),
        _ => return Err(OpError::internal(format!("invalid receiver for inline union field `{name}`"))),
    };
    Ok(result?)
}

pub(crate) fn try_write_heap(
    ctx: &VmContext, owner: &ObjRef, name: &str, value: &Value,
) -> Result<bool> {
    let object = owner.borrow();
    let Some((access, expected, plan)) = field(object, name) else { return Ok(false) };
    let plan = plan.map(Arc::as_ref);
    let offset = access.offset as usize;
    union_heap::with_encoded(ctx, value, expected, access.width as usize, plan, |bytes| {
        write(object, offset, plan, bytes)
    })?;
    if let Some(plan) = plan {
        union_blob::visit_refs(&object.cells(), offset, plan, &mut |offset, landed| {
            ctx.heap().write_barrier_field(&Value::Object(*owner), offset, landed);
        })?;
    }
    Ok(true)
}

fn write(object: &ScriptObject, offset: usize, plan: Option<&UnionLayoutDesc>, bytes: &[u8]) -> Result<()> {
    if let Some(plan) = plan {
        union_blob::replace_encoded(&object.cells(), offset, plan, bytes)
    } else {
        union_blob::write(&object.cells(), offset, bytes)
    }
}

pub(crate) struct StackWrite {
    access: FieldAccess,
    expected: Box<str>,
    plan: Option<Arc<UnionLayoutDesc>>,
}

impl StackWrite {
    pub(crate) fn for_field(object: &ScriptObject, name: &str) -> Option<Self> {
        let (access, expected, plan) = field(object, name)?;
        Some(Self { access, expected: expected.into(), plan: plan.cloned() })
    }

    pub(crate) fn finish(self, ctx: &VmContext, idx: u32, frame: u32, value: &Value) -> Result<()> {
        let plan = self.plan.as_deref();
        // Boxing takes place after releasing the stack-object arena borrow.
        union_heap::with_encoded(ctx, value, &self.expected, self.access.width as usize, plan, |bytes| {
            ctx.exec().with_stack_object(idx, frame, |object| {
                write(object, self.access.offset as usize, plan, bytes)
            })?
        })
    }
}
