//! Shared cold call/function-reference binding, including loading and exceptions.
use anyhow::Result;
use std::sync::Arc;
use crate::object::Value;
use crate::vm::runtime::{Function, Module};
use crate::vm::runtime::ids::tokens::FnId;
use crate::vm::runtime::dispatch::functions::lazy_call_id;
use crate::vm::context::VmContext;


/// The cold half of [`call`]: bind `fname` by name — this module's `func_index` first,
/// then the lazy loader (which may load the defining package) — check the signature,
/// and store the binding into the site's token. Returns the callee and its JIT id
/// (`None` only for a lazily loaded callee that has no id under `module`). `Err`
/// carries the exception to throw. Shared with `jit_call`'s by-name tier.
///
/// fix-call-arity-skew：首次绑定点。resolver 预填时已拒绝过签名对不上的站点，所以它们每次都会
/// 走到这里 —— 判定在此抛，且**不写回**（写回就等于把错的绑定缓存下来）。判定先于 cctor 屏障
/// （fix-crosspkg-static-call-cctor 的顺序）：签名对不上的调用本身非法，不应触发类型初始化。
#[inline(never)]
pub fn bind_callee<'a>(
    ctx: &'a VmContext, module: &'a Module, fname: &str, argc: usize,
    method_token: Option<&std::sync::atomic::AtomicU32>,
    holder: &'a mut Option<Arc<Function>>,
) -> Result<(&'a Function, Option<usize>), Value> {
    use crate::vm::context::symres::wrong_arity_exception;
    use crate::vm::runtime::dispatch::functions::call_arity;
    use std::sync::atomic::Ordering;
    if let Some(&idx) = module.func_index.get(fname) {
        if let Some(f) = module.functions.get(idx) {
            if let Some(exc) = wrong_arity_exception(ctx, module, fname, call_arity(f), argc) {
                return Err(exc);
            }
            if let Some(slot) = method_token { slot.store(idx as u32, Ordering::Relaxed); }
            return Ok((f, Some(idx)));
        }
    }
    // fix-silent-symbol-resolution：所有回落（本模块 func_index → 惰性加载器）都穷尽了 ⇒
    // **确定不存在**，抛可 catch 的类型化 MissingSymbolException（不是 `bail!`——那条走
    // anyhow Err，不经 find_handler，用户 `catch` 抓不到）。与 JIT 侧同一场景统一。
    let Some(lazy) = ctx.try_lookup_function(fname) else {
        return Err(crate::vm::exec::exception::make_missing_symbol_exception(
            ctx, module, format!("undefined function `{fname}`")));
    };
    if let Some(exc) = wrong_arity_exception(ctx, module, fname, call_arity(lazy.as_ref()), argc) {
        return Err(exc);
    }
    // Cache the binding as the callee's `FnId` — only when tokens are `FnId`s for this
    // module, and only for the function the table actually holds under that id.
    let id = lazy_call_id((ctx).funcs(), module, &lazy);
    if let (Some(slot), Some(id)) = (method_token, id) {
        slot.store(id as u32, Ordering::Relaxed);
    }
    Ok((&**holder.insert(lazy), id))
}



/// The target id a `LoadFn` / `MkClos` site creates its value with: the site's token
/// (`ResolvedTokens::fn_ref_tokens`) when bound — one relaxed load, no hashing — else
/// [`bind_fn_ref`]. `Err` carries the exception to throw.
#[inline]
pub fn fn_ref_id(
    ctx: &VmContext, module: &Module, name: &str, token: Option<&std::sync::atomic::AtomicU32>,
) -> Result<FnId, Value> {
    if let Some(t) = token {
        let id = t.load(std::sync::atomic::Ordering::Relaxed);
        if id != crate::vm::runtime::ids::tokens::UNRESOLVED { return Ok(FnId(id)); }
    }
    bind_fn_ref(ctx, module, name, token)
}



/// The cold half of [`fn_ref_id`]: resolve a function reference's target by name, once,
/// when the value is created — this module's `func_index` first, then the lazy loader
/// (which loads the defining package) — and store the id into the site's token. The same
/// precedence `CallIndirect` used to apply on every call, and the same id space as `Call`
/// tokens ([`fn_by_id`] reads it back).
///
/// Errors surface here, at creation, instead of at the first invocation (runtime-audit
/// §7 P1-2 decision): an unknown name throws a catchable `MissingSymbolException`
/// (`undefined function `…``, the `Call` path's wording) where `CallIndirect` used to
/// fail with an internal error. A lazily loaded target is only representable when
/// `module` is the `FuncTable`'s entry module (every production run); against any
/// other module (unit tests on a bare `VmContext`) it throws instead.
#[inline(never)]
pub fn bind_fn_ref(
    ctx: &VmContext, module: &Module, name: &str, token: Option<&std::sync::atomic::AtomicU32>,
) -> Result<FnId, Value> {
    let id = match module.func_index.get(name) {
        Some(&idx) if idx < module.functions.len() => idx as u32,
        _ => {
            let Some(lazy) = ctx.try_lookup_function(name) else {
                return Err(crate::vm::exec::exception::make_missing_symbol_exception(
                    ctx, module, format!("undefined function `{name}`")));
            };
            match lazy_call_id((ctx).funcs(), module, &lazy) {
                Some(id) => id as u32,
                None => {
                    let msg = format!(
                        "cannot reference `{name}`: a lazily loaded function has no id outside the VM's entry module");
                    return Err(crate::vm::exec::exception::make_stdlib_exception(ctx, module, "Std.Exception", msg.clone())
                        .unwrap_or_else(|_| Value::Str(msg.into())));
                }
            }
        }
    };
    if let Some(t) = token { t.store(id, std::sync::atomic::Ordering::Relaxed); }
    Ok(FnId(id))
}

