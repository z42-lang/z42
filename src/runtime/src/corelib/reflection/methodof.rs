//! `methodof(Type.Member(sig))` runtime support (add-method-reference, 2026-09-11).
//!
//! The compiler resolves the overload **entirely at bind time** and emits a single
//! interned qualified name (`<declaring class FQN>.<RegKey>`) — byte-identical to
//! what a plain `Call` to the same method would emit. So this builtin does no
//! signature matching: it takes that one name and materialises the reflection
//! object, exactly as `Instruction::Typeof` materialises a `Std.Type`.
//!
//! Deliberately **not cached**: `typeof` allocates a fresh `Std.Type` on every
//! execution (`typeof(T) == typeof(T)` is `false` today), and `methodof` stays
//! symmetric with it. Interning reflection objects is a separate optimisation
//! that has to settle object-identity semantics for both at once.

use super::*;

/// `__methodof(qualified: string) -> Std.Reflection.MethodInfo`
///
/// The name is compiler-generated and always names a real emitted function, so a
/// miss here is a **compiler bug, not user error** — it must be loud rather than
/// degrade into a half-populated `MethodInfo`. `build_method_info` on its own is
/// lenient (a missing SIGS entry just yields `sig_found = false` and an empty
/// parameter list), which would turn "emitted a name nobody can resolve" into a
/// silent wrong answer — precisely the failure mode this feature exists to kill.
pub fn methodof(ctx: &VmContext, qualified: &crate::metadata::vstr::Str) -> Result<Value, BuiltinError> {
    let qualified = qualified.to_string();

    // Existence check mirrors `invoke_qualified`'s two-step lookup: the current
    // module's own index first, then the cross-module table.
    let known = match ctx.core.module.as_ref() {
        Some(m) => m.func_index.contains_key(qualified.as_str()),
        None => false,
    } || ctx.try_lookup_function(&qualified).is_some();
    if !known {
        return Err(BuiltinError::Internal(format!(
            "__methodof: no emitted function named `{qualified}` — the compiler emitted a \
             method reference that does not resolve (this is a z42c bug, not a source error)")));
    }

    // `simple` is the source-level name: strip the `$arity$types` dispatch mangle
    // and the owning-class prefix. `build_method_info` re-strips the mangle itself,
    // so passing the last path segment is enough.
    let simple = qualified.rsplit('.').next().unwrap_or(qualified.as_str());
    build_method_info(ctx, simple, &qualified, false).map_err(BuiltinError::Legacy)
}

/// `__methodof` 迁 builtin ABI v2(P1-8 批次 5)。`try_lookup_function` 只查已注册表,
/// 不执行 z42 代码;名字解析失败是编译器缺陷 → `Internal`(致命)。
#[z42_macros::builtins]
pub mod typed {
    use super::*;

    #[builtin(name = "__methodof")]
    pub fn method_of(ctx: &VmContext, qualified: &crate::metadata::vstr::Str) -> Result<Value, BuiltinError> {
        methodof(ctx, qualified)
    }
}
