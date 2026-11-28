//! Builtin adapters for compiler-emitted frame union operations.
use crate::exec::builtin_abi as abi;
use crate::exec::ops::union_frame;
use crate::object::Value;
use crate::vm::VmContext;

fn fault(err: anyhow::Error) -> abi::BuiltinError {
    abi::BuiltinError::Internal(err.to_string())
}

#[z42_macros::builtins]
pub(crate) mod typed {
    use super::*;

    #[builtin(name = "__union_cell")]
    pub fn cell(ctx: &VmContext, value: &Value, index: i64) -> Result<i64, abi::BuiltinError> {
        union_frame::read_cell(ctx, value, index).map_err(fault)
    }

    #[builtin(name = "__union_ref")]
    pub fn reference(ctx: &VmContext, value: &Value, index: i64) -> Result<Value, abi::BuiltinError> {
        union_frame::read_ref(ctx, value, index).map_err(fault)
    }

    #[builtin(name = "__union_array_read")]
    pub fn array_read(ctx: &VmContext, target: &Value, array: &Value, index: i64) -> Result<(), abi::BuiltinError> {
        crate::exec::ops::object::array::read_union_into(ctx, target, array, index)?;
        Ok(())
    }

    #[builtin(name = "__union_field_read")]
    pub fn field_read(ctx: &VmContext, owner: &Value, name: &str, target: &Value) -> Result<Value, abi::BuiltinError> {
        Ok(crate::exec::ops::object::union_field::read(ctx, owner, name, target)?)
    }
}

pub(crate) const STORE: &[abi::BuiltinDesc] = &[abi::BuiltinDesc {
    name: "__union_store",
    // The destination is followed by cells and references. Its layout
    // supplies the argument counts, including the optional reference tail.
    params: &[abi::ParamKind::Any],
    ret: abi::RetKind::Void,
    flags: abi::BuiltinFlags { blocking: false, pure: false, reenters: false },
    call_regs: |ctx, this, regs, indices| store(ctx, &abi::RegArgs::new(this, regs, indices)),
    call_slice: |ctx, args| store(ctx, args),
}];

fn store(ctx: &VmContext, args: &(impl abi::ArgSource + ?Sized)) -> abi::CallResult {
    union_frame::store(ctx, args).map(|()| None).map_err(|err| Box::new(fault(err)))
}
