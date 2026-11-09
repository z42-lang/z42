//! ToString protocol invocation and formatting.

use crate::metadata::{Function, Module};
use crate::object::Value;
use crate::context::VmContext;
use anyhow::{bail, Result};

// ── ToString protocol ────────────────────────────────────────────────────────

/// The `ToString` protocol: stringify `val`, respecting `ToString()` overrides, and hand
/// the text to `f` as a `&str` (perf-str-concat-direct: nothing is materialised).
///
/// For `Value::Object` we try to dispatch `ToString` via the vtable. If the class has no
/// `ToString` method (e.g. it inherits the default from `Std.Object`) we fall back to the
/// `__obj_to_str` builtin (simple name). Boxed receivers go through `resolve_vcall`. All
/// other value types use `value_to_str` rules. A user `ToString`'s returned GC string is passed
/// through as-is (no copy); scalars are formatted into a stack buffer
/// ([`with_value_str`](crate::object::value::format::with_value_str)) — no heap allocation.
/// `"k" + i` / `$"{x}"` build their result straight from this view. Same dispatch, same
/// result text and exception propagation for every caller (concat, interpolation, `WriteLine`).
pub fn with_obj_str<R>(
    ctx: &VmContext, module: &Module, val: &Value, f: impl FnOnce(&str) -> R,
) -> Result<R> {
    use crate::object::value::format::with_value_str;
    let out = match tostring_result(ctx, module, val)? {
        ToStringResult::Value(Value::Str(s)) => f(&s),
        ToStringResult::Value(other) => with_value_str(&other, f),
        ToStringResult::Empty => f(""),
        ToStringResult::NotDispatched => with_value_str(val, f),
    };
    Ok(out)
}

/// perf-str-concat-direct: [`with_obj_str`] as a GC string (`ToStr` / `$"{x}"`). A user
/// `ToString`'s returned string is used as-is — no copy (the JIT's native `jit_to_str` path
/// does the same); everything else is formatted straight into one fresh GC block.
pub fn obj_to_gc_str(ctx: &VmContext, module: &Module, val: &Value) -> Result<crate::object::string::vstr::Str> {
    use crate::object::value::format::with_value_str;
    let heap = ctx.heap();
    Ok(match tostring_result(ctx, module, val)? {
        ToStringResult::Value(Value::Str(s)) => s,
        ToStringResult::Value(other) => with_value_str(&other, |t| heap.alloc_str(t)),
        ToStringResult::Empty => heap.alloc_str(""),
        ToStringResult::NotDispatched => with_value_str(val, |t| heap.alloc_str(t)),
    })
}

/// The raw outcome of the `ToString` protocol for one receiver (see [`with_obj_str`]).
enum ToStringResult {
    /// What the dispatched `ToString` (or the `__obj_to_str` / boxed fallback) returned.
    Value(Value),
    /// A `ToString` that returned nothing.
    Empty,
    /// No dispatch applies — stringify the receiver itself (`value_to_str` rules).
    NotDispatched,
}

fn tostring_result(ctx: &VmContext, module: &Module, val: &Value) -> Result<ToStringResult> {
    fn outcome(ctx: &VmContext, o: crate::exec::call::outcome::ExecOutcome) -> Result<ToStringResult> {
        match o {
            crate::exec::call::outcome::ExecOutcome::Returned(Some(v)) => Ok(ToStringResult::Value(v)),
            crate::exec::call::outcome::ExecOutcome::Returned(None)    => Ok(ToStringResult::Empty),
            crate::exec::call::outcome::ExecOutcome::Thrown(v)         => Err(tostring_threw(ctx, v)),
        }
    }
    if let Value::Object(rc) = val {
        let type_desc = rc.type_desc_arc().clone();
        // Try vtable first (O(1))
        let func_name_opt = type_desc.vtable_index.get("ToString")
            .map(|&slot| type_desc.vtable[slot].1.clone());
        if let Some(func_name) = func_name_opt {
            let callee = module.func_index.get(func_name.as_str())
                .and_then(|&idx| module.functions.get(idx));
            if let Some(callee) = callee {
                return outcome(ctx, crate::exec::engine::invoke(ctx, module, callee, &[val.clone()])?);
            }
        }
        // Fallback: the Rust default ToString (unqualified type name), as the
        // `__obj_to_str` builtin behind the vtable slot would answer.
        return (ctx.core.exec.object_protocol.default_to_string)(val)
            .map(|name| ToStringResult::Value(Value::Str(name.into())))
            .map_err(|e| e.into_anyhow(ctx));
    }
    // dispatch-tostring-in-native-stringify: **装箱**接收者（值 struct 装箱 / 基元装箱 / enum 盒）。
    // 复用 `resolve_vcall` 的整套判据 —— 它已经把三种盒各自的正确答案都定好了：
    // enum → 成员名、基元 → 标量、struct → 自身槽位的 `ToString`（没有才短类型名），
    // 且**刻意不回落 `Std.Object.ToString`**（那个 builtin 收装箱 struct 直接抛
    // `__obj_to_str: expected an object`）。自己再写一份判据必然与它漂移。
    // P1-2 PR 8: a callable holds its target's `FnId`; the display names the function
    // (`<fn N>` / `<closure N>`, the text `value_to_str` printed when the value held the name).
    if let Some(id) = val.callable_id() {
        if let Some(f) = crate::metadata::dispatch::functions::fn_by_id((ctx).funcs(), module, id.0) {
            let kind = if matches!(val, Value::FuncRef(_)) { "fn" } else { "closure" };
            let text = format!("<{kind} {}>", f.name);
            return Ok(ToStringResult::Value(Value::Str(ctx.heap().alloc_str(&text))));
        }
        return Ok(ToStringResult::NotDispatched);
    }
    if matches!(val, Value::BoxedStruct(_)) {
        let r = crate::exec::ops::vcall::resolve_vcall(ctx, module, val, "ToString", 0, None)?;
        return match r.target {
            crate::exec::ops::vcall::VCallTarget::Immediate(v) => Ok(ToStringResult::Value(v)),
            crate::exec::ops::vcall::VCallTarget::Thrown(v) => Err(tostring_threw(ctx, v)),
            crate::exec::ops::vcall::VCallTarget::Local { func, .. } =>
                outcome(ctx, crate::exec::engine::invoke(ctx, module, func, &[r.this.clone()])?),
        };
    }
    Ok(ToStringResult::NotDispatched)
}

/// A user `ToString` threw while stringifying (concatenation, interpolation,
/// `Console.WriteLine(obj)`). The exception propagates with its own type, like
/// any other call's: the value goes into `pending_thrown` — the same channel
/// callback builtins use — and the returned error tells the caller to take it.
/// Every caller of [`with_obj_str`] / [`obj_to_gc_str`] / [`stringify_dispatch`] must therefore
/// check `take_pending_thrown()` on `Err` (interp `Add` / `ToStr`, JIT
/// `jit_add` / `jit_to_str`; builtins get it from `exec_call::builtin`).
///
/// Earlier the exception was swallowed into an `<exception: …>` string and the
/// program carried on.
fn tostring_threw(ctx: &VmContext, thrown: Value) -> anyhow::Error {
    ctx.set_pending_thrown(thrown);
    anyhow::anyhow!("ToString threw an exception")
}

/// dispatch-tostring-in-native-stringify: `with_obj_str` 的 **ctx-only** 包装 —— 给手里只有
/// `&VmContext` 的 native 落点用（`Console.WriteLine` 一族 builtin、字符串拼接的混合臂）。
///
/// 此前这些落点直接用无 ctx 的 `value_to_str` ⇒ 对象一律打 `类型名{...}`，于是同一个对象
/// 「插值对、`WriteLine` 错」。`module` 从 `ctx.core.runtime.module` 取（与 `reflection/invoke.rs`
/// 的取法同源）；取不到（未装载模块的宿主场景）就回落 `value_to_str`，不为展示路径制造失败。
pub fn stringify_dispatch(ctx: &VmContext, val: &Value) -> Result<String> {
    with_stringify_dispatch(ctx, val, |s| s.to_string())
}

/// perf-str-concat-direct: [`stringify_dispatch`] as a GC string (see [`obj_to_gc_str`]).
pub fn stringify_dispatch_gc(ctx: &VmContext, val: &Value) -> Result<crate::object::string::vstr::Str> {
    match ctx.core.runtime.module.as_ref() {
        Some(m) => {
            let m = m.clone();
            obj_to_gc_str(ctx, m.as_ref(), val)
        }
        None => Ok(crate::object::value::format::with_value_str(val, |t| ctx.heap().alloc_str(t))),
    }
}

/// perf-str-concat-direct: [`stringify_dispatch`] as a `&str` view (see [`with_obj_str`]).
pub fn with_stringify_dispatch<R>(ctx: &VmContext, val: &Value, f: impl FnOnce(&str) -> R) -> Result<R> {
    match ctx.core.runtime.module.as_ref() {
        Some(m) => {
            let m = m.clone();
            with_obj_str(ctx, m.as_ref(), val, f)
        }
        None => Ok(crate::object::value::format::with_value_str(val, f)),
    }
}

// ── Virtual method resolution (fallback) ─────────────────────────────────────

// ── Fallback TypeDesc ────────────────────────────────────────────────────────
