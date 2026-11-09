//! Registered native type dispatch behind the VM's native call contract.
use crate::object::Value;
use crate::context::VmContext;
use crate::exec::native::NativeCallError;
use super::{marshal, dispatch as ndisp};

pub(crate) fn call_registered(
    ctx: &VmContext, module_name: &str, type_name: &str, symbol: &str,
    registers: &[Value], args: &[u32],
) -> Result<Value, NativeCallError> {
    let ty = crate::interop::resolve_native_type(ctx, module_name, type_name)?.ok_or_else(|| {
        anyhow::anyhow!(
            "CallNative: unknown native type {module_name}::{type_name}"
        )
    })?;
    let method = ty.method(symbol).ok_or_else(|| {
        anyhow::anyhow!(
            "CallNative: unknown method {module_name}::{type_name}::{symbol}"
        )
    })?;

    // Marshal each register into a Z42Value targeting the corresponding
    // ABI-side parameter type.
    if args.len() != method.params.len() {
        return Err(anyhow::anyhow!(
            "CallNative {module_name}::{type_name}::{symbol}: arity mismatch (caller passed {}, signature wants {})",
            args.len(),
            method.params.len()
        ).into());
    }
    // Spec C8: marshal arena owns temporaries (e.g. CString backing
    // for `*const c_char`) for the call's duration; dropped after
    // dispatch returns.
    let mut arena = marshal::Arena::new();
    let mut z_args: Vec<z42_abi::Z42Value> = Vec::with_capacity(method.params.len());
    for (reg, param_ty) in args.iter().zip(method.params.iter()) {
        let v = registers.get(*reg as usize)
            .ok_or_else(|| anyhow::anyhow!("undefined register %{reg}"))?;
        z_args.push(marshal::value_to_z42(v, param_ty, &mut arena)?);
    }

    // SAFETY: cif was built from `params`/`return_type` matching the
    // native function pointer at registration time; native lib keeps
    // the function alive via interop extension state. CURRENT_VM names
    // this ctx (engine guards, installed at the bottom frame) so a
    // re-entrant z42_* call finds the right ctx.
    let z_ret = unsafe {
        ndisp::call(
            &method.cif,
            method.fn_ptr,
            &z_args,
            &method.params,
            &method.return_type,
        )
    }?;
    drop(arena);

    Ok(marshal::z42_to_value(&z_ret, &method.return_type)?)
}
