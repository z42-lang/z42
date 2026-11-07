//! Struct argument copies and boxed returns at erased delegate boundaries.
use anyhow::{anyhow, Result};
use crate::metadata::ir::METHOD_FLAG_SRET;
use crate::object::Value;
use crate::vm::runtime::{Function, Module};
use crate::vm_context::VmContext;
use super::{args::CallArgs, outcome::ExecOutcome};

/// Explicit sret calls already pass their trailing slot. An erased caller passes
/// only the logical arguments and expects a value; other arities remain errors.
pub(crate) fn needs_boxed_return(function: &Function, argument_count: usize) -> bool {
    function.method_flags & METHOD_FLAG_SRET != 0 && argument_count == function.param_count
}

pub(crate) fn has_declared_boxed_argument(function: &Function, index: usize, value: &Value) -> bool {
    matches!(value, Value::BoxedStruct(_))
        && function.param_types().get(index).is_some_and(|name| name != "?")
}

pub(crate) fn invoke(
    ctx: &VmContext, module: &Module, function: &Function, id: usize,
    frame_id: u32, mut arguments: Vec<Value>,
) -> Result<ExecOutcome> {
    // The caller's registers/closure keep original arguments rooted. New struct
    // copies and the return slot live in its arena across any callee collection.
    for (argument, declared_type) in arguments.iter_mut().zip(function.param_types().iter()) {
        if let Value::BoxedStruct(object) = *argument {
            let is_struct = ctx.try_lookup_type(declared_type)
                .is_some_and(|ty| ty.struct_layout().is_some());
            if is_struct {
                *argument = crate::vm::exec::ops::structs::unbox_struct(ctx, frame_id, &object)?;
            }
        }
    }
    let result_slot = if needs_boxed_return(function, arguments.len()) {
        let descriptor = ctx.try_lookup_type(&function.ret_type)
            .ok_or_else(|| anyhow!("indirect struct return: unknown type `{}`", function.ret_type))?;
        let layout = descriptor.struct_layout()
            .ok_or_else(|| anyhow!("indirect struct return: `{}` has no struct layout", function.ret_type))?;
        let index = ctx.struct_alloc(frame_id, descriptor.name.clone().into(), layout);
        arguments.push(Value::StructRef { idx: index, frame_id });
        Some(index)
    } else { None };
    let native = ctx.tier().and_then(|tier| tier.invoke(
        ctx, crate::vm::exec::engine::TierTarget::Site(id), CallArgs::Values(&arguments), &[],
    ));
    let outcome = match native {
        Some(result) => result?,
        None => crate::vm::exec::engine::invoke(ctx, module, function, &arguments)?,
    };
    match (result_slot, outcome) {
        (Some(index), ExecOutcome::Returned(_)) => {
            let (name, bytes, references) = ctx.exec.struct_arena.lock()
                .with(index, frame_id, |slot| (slot.type_name.clone(), slot.bytes.clone(), slot.refs.clone()))?;
            let result = crate::objops::struct_leaf::box_struct_blob(ctx, &name, &bytes, &references)?;
            Ok(ExecOutcome::Returned(Some(result)))
        }
        (_, outcome) => Ok(outcome),
    }
}
