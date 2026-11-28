//! Frame storage for unions carrying references.
use crate::exec::builtin_abi::ArgSource;
use crate::exec::call::struct_arena::StructSlot;
use crate::exec::ops::object::struct_leaf::as_struct_ref;
use crate::object::Value;
use crate::vm::VmContext;
use anyhow::{bail, Result};

const WORD_BYTES: usize = std::mem::size_of::<i64>();

pub(crate) fn read_cell(ctx: &VmContext, value: &Value, index: i64) -> Result<i64> {
    let (slot, frame) = as_struct_ref(value, "union cell")?;
    ctx.exec().with_struct_slot(slot, frame, |slot| {
        check_slot(slot)?;
        let index = usize::try_from(index)?;
        let word = slot.bytes.chunks_exact(WORD_BYTES).nth(index)
            .ok_or_else(|| anyhow::anyhow!("invalid union cell index {index}"))?;
        Ok(i64::from_le_bytes(word.try_into()?))
    })?
}

pub(crate) fn read_ref(ctx: &VmContext, value: &Value, index: i64) -> Result<Value> {
    let (slot, frame) = as_struct_ref(value, "union reference")?;
    ctx.exec().with_struct_slot(slot, frame, |slot| {
        check_slot(slot)?;
        let index = usize::try_from(index)?;
        slot.refs.get(index).copied().ok_or_else(|| anyhow::anyhow!("invalid union reference index {index}"))
    })?
}

/// The first register is the destination. The remaining registers contain a complete
/// byte image followed by the case's references, including nulls for inactive nested cases.
pub(crate) fn store(ctx: &VmContext, args: &(impl ArgSource + ?Sized)) -> Result<()> {
    if args.argc() == 0 { bail!("missing union destination"); }
    let (slot, frame) = as_struct_ref(args.arg(0), "union destination")?;
    ctx.exec().with_struct_slot_mut(slot, frame, |slot| replace(slot, args))?
}

fn check_slot(slot: &StructSlot) -> Result<()> {
    if !slot.is_reference_union() || slot.bytes.is_empty() || slot.bytes.len() % WORD_BYTES != 0 {
        bail!("invalid reference union storage for `{}`", slot.type_name);
    }
    Ok(())
}

fn valid_reference_payload(value: &Value) -> bool {
    // Values of type object also use these self-describing scalar representations.
    matches!(value, Value::Bool(_) | Value::Char(_) | Value::F64(_)
        | Value::Null | Value::Str(_) | Value::Object(_) | Value::Array(_)
        | Value::BoxedStruct(_) | Value::InterfaceRef(_) | Value::FuncRef(_) | Value::Closure(_))
}

fn replace(slot: &mut StructSlot, args: &(impl ArgSource + ?Sized)) -> Result<()> {
    check_slot(slot)?;
    let ref_start = 1 + slot.bytes.len() / WORD_BYTES;
    if !(ref_start..=ref_start + slot.refs.len()).contains(&args.argc()) {
        bail!("invalid union payload length for `{}`", slot.type_name);
    }
    if !(1..ref_start).all(|index| matches!(args.arg(index), Value::I64(_))) {
        bail!("non-integer union cell for `{}`", slot.type_name);
    }
    if !(ref_start..args.argc()).all(|index| valid_reference_payload(args.arg(index))) {
        bail!("invalid union reference payload for `{}`", slot.type_name);
    }

    // Arguments are rooted in registers. After validation, update bytes and
    // references under the same lock used by the arena's root scanner.
    for (index, cell) in slot.bytes.chunks_exact_mut(WORD_BYTES).enumerate() {
        if let Value::I64(bits) = args.arg(index + 1) {
            cell.copy_from_slice(&bits.to_le_bytes());
        }
    }
    let active = args.argc() - ref_start;
    for (index, value) in slot.refs[..active].iter_mut().enumerate() {
        *value = *args.arg(ref_start + index);
    }
    slot.refs[active..].fill(Value::Null);
    Ok(())
}

#[cfg(all(test, z42_test_facade))]
#[path = "union_frame_tests.rs"]
mod tests;
