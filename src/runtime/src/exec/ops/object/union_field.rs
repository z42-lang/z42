//! Whole-value access to inline union instance fields.
use crate::exec::ops::object::error::{OpError, OpResult};
use crate::exec::ops::object::struct_leaf::as_struct_ref;
use crate::object::value::union_blob;
use crate::object::{FieldAccess, FieldCell, ScriptObject, Value};
use crate::vm::VmContext;
use anyhow::{ensure, Result};

fn field<'a>(object: &'a ScriptObject, name: &str) -> Option<(FieldAccess, &'a str)> {
    let slot = *object.type_desc.field_index.get(name)?;
    let expected: &str = &object.type_desc.fields.get(slot)?.type_tag;
    if !union_blob::is_storage_name(expected) { return None; }
    let access = object.field_access_of(slot)?;
    (access.cell == FieldCell::Struct && access.width != 0).then_some((access, expected))
}

pub(crate) fn read(
    ctx: &VmContext, owner: &Value, name: &str, target: &Value,
) -> OpResult<Value> {
    let load = |object: &ScriptObject| {
        let (access, expected) = field(object, name)
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
            ensure!(target.type_name.as_ref() == expected && target.bytes.len() == width
                && target.refs.is_empty(),
                "inline union field type mismatch");
            union_blob::read(&cells, offset, &mut target.bytes)
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

pub(crate) fn try_write(
    ctx: &VmContext, object: &ScriptObject, name: &str, value: &Value,
) -> Result<bool> {
    let Some((access, expected)) = field(object, name) else { return Ok(false) };
    let width = access.width as usize;
    let offset = access.offset as usize;
    let cells = object.cells();
    if width <= 8 {
        let Value::I64(bits) = value else { anyhow::bail!("expected a packed union field value"); };
        union_blob::write(&cells, offset, &bits.to_le_bytes()[..width])?;
    } else {
        let (slot, frame) = as_struct_ref(value, "inline union field source")?;
        ctx.exec().with_struct_slot(slot, frame, |source| {
            ensure!(source.type_name.as_ref() == expected && source.bytes.len() == width
                && source.refs.is_empty(), "inline union field type mismatch");
            union_blob::write(&cells, offset, &source.bytes)
        })??;
    }
    Ok(true)
}
