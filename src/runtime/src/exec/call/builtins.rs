//! Builtin invocation owns argument views, counters, typed faults and void handling.
use super::builtin_abi as abi;
use crate::{object::Value, context::VmContext};
use anyhow::Result;

/// Native module calls supplied by interop, independent of any concrete provider.
pub struct NativeProviderServices {
    pub invoke: fn(&VmContext, Option<&str>, &str, &[Value]) -> Option<abi::CallResult>,
}

fn native_export(ctx: &VmContext, name: &str, args: &[Value]) -> Option<abi::CallResult> {
    let services = ctx.ext::<NativeProviderServices>()?;
    (services.invoke)(ctx, None, name, args)
}

/// Invoke only the named provider. A miss never falls back to a bare builtin.
pub fn invoke_module(
    ctx: &VmContext, provider: &str, name: &str, regs: &[Value], args: &[u32],
) -> abi::CallResult {
    ctx.counters().native_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if ctx.has_runtime_observers() {
        ctx.fire_runtime_event(&crate::diagnostics::observer::RuntimeEvent::NativeCallEntered {
            module: provider.to_owned(), symbol: name.to_owned(),
        });
    }
    let values: Result<Vec<_>, _> = args.iter().map(|&index| {
        regs.get(index as usize).copied().ok_or_else(|| Box::new(
            abi::BuiltinError::Internal(format!("undefined native argument register %{index}"))))
    }).collect();
    let values = values?;
    if let Some(services) = ctx.ext::<NativeProviderServices>() {
        if let Some(result) = (services.invoke)(ctx, Some(provider), name, &values) {
            return result;
        }
    }
    Err(Box::new(abi::BuiltinError::Exception {
        class: "Std.MissingSymbolException",
        message: format!("native module `{provider}` does not export `{name}`"),
    }))
}

/// Name-keyed entry for callers that hold the arguments as a slice — unit tests,
/// embedders, and the interpreter's `ToString` dispatch fallback
/// (`exec_builtin_value`); the engines use [`invoke_builtin`]. A name not in
/// `BUILTINS` throws `Std.MissingSymbolException` (as an undefined function does).
pub fn exec_builtin(ctx: &VmContext, name: &str, args: &[Value]) -> Result<Option<Value>> {
    // add-runtime-counters : name-keyed slow path also increments
    // for consistency with invoke_builtin_id (callers may hit either).
    ctx.counters().builtin_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    if let Some(id) = ctx.core.runtime.builtins.id_of(name) {
        return (ctx.core.exec.builtins[id.0 as usize].call_slice)(ctx, args).map_err(|e| (*e).into_anyhow(ctx));
    }
    if let Some(result) = native_export(ctx, name, args) {
        return result.map_err(|error| (*error).into_anyhow(ctx));
    }
    Err(unknown_builtin(name).into_anyhow(ctx))
}

/// An unresolvable builtin name at run time — a stdlib built against another VM.
#[cold]
fn unknown_builtin(name: &str) -> abi::BuiltinError {
    abi::BuiltinError::Exception {
        class: "Std.MissingSymbolException",
        message: crate::loader::locate::probing::with_sdk_missing_hint(format!("unknown builtin `{name}`")),
    }
}

/// The engines' builtin call (interp `Builtin`, JIT `jit_builtin`): the arguments are
/// read straight from the caller's register file — `this` first when the receiver is
/// not in a register (vcall), then `regs[args[i]]`; a typed builtin reads them in place.
/// `Ok(None)` = void.
///
/// `id` = the site's resolved `BuiltinId`, or `UNRESOLVED` to resolve `name` now
/// (a miss throws `MissingSymbolException`).
#[inline]
pub fn invoke_builtin(
    ctx: &VmContext, id: u32, name: &str,
    this: Option<Value>, regs: &[Value], args: &[u32],
) -> abi::CallResult {
    if id == crate::metadata::ids::tokens::UNRESOLVED {
        return invoke_by_name(ctx, name, this, regs, args);
    }
    invoke_builtin_id(ctx, id, this, regs, args)
}

/// [`invoke_builtin`] for a resolved id (the `[Native]` forward short-circuit always has one).
#[inline(always)]
pub(crate) fn invoke_builtin_id(
    ctx: &VmContext, id: u32, this: Option<Value>, regs: &[Value], args: &[u32],
) -> abi::CallResult {
    ctx.counters().builtin_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let idx = id as usize;
    debug_assert!(idx < ctx.core.exec.builtins.len(), "BuiltinId {id} out of range");
    (ctx.core.exec.builtins[idx].call_regs)(ctx, this, regs, args)
}

/// [`invoke_builtin`] by name: the table, else `MissingSymbolException`.
#[cold]
#[inline(never)]
fn invoke_by_name(
    ctx: &VmContext, name: &str, this: Option<Value>, regs: &[Value], args: &[u32],
) -> abi::CallResult {
    if let Some(id) = ctx.core.runtime.builtins.id_of(name) {
        return invoke_builtin_id(ctx, id.0, this, regs, args);
    }
    use abi::ArgSource;
    let source = abi::RegArgs::new(this, regs, args);
    let values: Vec<_> = (0..source.argc()).map(|index| *source.arg(index)).collect();
    if let Some(result) = native_export(ctx, name, &values) { return result; }
    Err(Box::new(unknown_builtin(name)))
}

/// 给**明确期待有返回值**的调用方（`__obj_to_str` 这类）：撞上 void builtin 就报错，
/// 而不是静默给一个占位值。
///
/// split-null-sentinel-channels ④：此前这些调用方拿到的是 `Value`，void 与「返回 null」
/// 无从分辨；现在 `None` 会变成一条明确的错误，说明表与调用方对这个 builtin 的
/// 「有没有返回值」认知不一致。
pub fn exec_builtin_value(ctx: &VmContext, name: &str, args: &[Value]) -> Result<Value> {
    exec_builtin(ctx, name, args)?.ok_or_else(|| {
        anyhow::anyhow!("builtin `{name}` is declared void but its result was used as a value")
    })
}
