use super::*;

// ── Runtime generic instantiation (G1, plan-generic-reflection) ──────────────

/// Resolve a type name to its `TypeDesc` (main module then lazy loader).
pub(super) fn resolve_td(ctx: &VmContext, name: &str) -> Option<Arc<TypeDesc>> {
    ctx.module()
        .and_then(|m| m.type_registry.get(name).cloned())
        .or_else(|| ctx.try_lookup_type(name))
}

/// True if `derived` is assignable to `target` (subclass / interface impl / self),
/// via the VM's canonical subclass walk. Handle-less names compare by identity.
pub(super) fn type_name_assignable(ctx: &VmContext, derived: &str, target: &str) -> bool {
    match ctx.module() {
        Some(m) => crate::interp::dispatch::is_subclass_or_eq_td(ctx, &m.type_registry, derived, target),
        None => derived == target,
    }
}

/// True if type `arg_name` satisfies a base-class / interface constraint named
/// `constraint`. First tries the VM's exact-FQ subclass walk; then falls back to
/// a simple-name match down `arg_name`'s transitive base + interface chain —
/// because `where T: IEntity` stores the *source spelling* (`IEntity`, often
/// unqualified) while a type's interface list carries FQ names.
pub(super) fn constraint_satisfied_by(ctx: &VmContext, arg_name: &str, constraint: &str) -> bool {
    if type_name_assignable(ctx, arg_name, constraint) {
        return true;
    }
    let target_simple = crate::metadata::index::type_names::unqualified(constraint);
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut stack = vec![arg_name.to_string()];
    while let Some(cur) = stack.pop() {
        if !seen.insert(cur.clone()) {
            continue;
        }
        if crate::metadata::index::type_names::unqualified(&cur) == target_simple {
            return true;
        }
        if let Some(td) = resolve_td(ctx, &cur) {
            if let Some(b) = &td.base_name {
                stack.push(b.clone());
            }
            for i in td.interfaces() {
                stack.push(i.to_string());
            }
        }
    }
    false
}

/// Numeric / bool / char primitive names are value types (`struct` constraint
/// satisfiers); `string` / `object` are reference types.
pub(super) fn is_value_type_primitive(name: &str) -> bool {
    matches!(
        name,
        "int" | "long" | "short" | "sbyte" | "byte" | "ushort" | "uint" | "ulong"
            | "float" | "double" | "bool" | "char"
            | "i8" | "u8" | "i16" | "u16" | "i32" | "u32" | "i64" | "u64" | "f32" | "f64"
    )
}

/// Q3 safety: validate one type argument against its parameter's constraint
/// bundle. Reflection (`MakeGenericType`) is the sole entry that bypasses the
/// compiler's compile-time constraint checking, so it MUST self-police — else a
/// reflectively-constructed type could violate `where` clauses and crash later.
/// Throws a catchable `Std.Exception` on violation.
pub(super) fn validate_type_arg_constraint(
    ctx: &VmContext,
    param_name: &str,
    cb: &crate::metadata::ir::ConstraintBundle,
    arg_name: &str,
    type_params: &[String],
    all_arg_names: &[String],
) -> Result<()> {
    let arg_td = resolve_td(ctx, arg_name);
    let is_value = arg_td
        .as_ref()
        .map(|td| td.class_flags & crate::metadata::ir::CLASS_FLAG_STRUCT != 0)
        .unwrap_or_else(|| is_value_type_primitive(arg_name));
    let is_enum = arg_td.as_ref().map(|td| td.is_enum()).unwrap_or(false);

    if cb.requires_class && is_value {
        bail!("MakeGenericType: `{arg_name}` for type parameter `{param_name}` must be a reference type (constraint `class`)");
    }
    if cb.requires_struct && !is_value {
        bail!("MakeGenericType: `{arg_name}` for type parameter `{param_name}` must be a value type (constraint `struct`)");
    }
    if cb.requires_enum && !is_enum {
        bail!("MakeGenericType: `{arg_name}` for type parameter `{param_name}` must be an enum (constraint `enum`)");
    }
    if let Some(base) = &cb.base_class {
        if !constraint_satisfied_by(ctx, arg_name, base) {
            bail!("MakeGenericType: `{arg_name}` for type parameter `{param_name}` must derive from `{base}`");
        }
    }
    for iface in &cb.interfaces {
        if !constraint_satisfied_by(ctx, arg_name, iface) {
            bail!("MakeGenericType: `{arg_name}` for type parameter `{param_name}` must implement `{iface}`");
        }
    }
    if cb.requires_constructor {
        // Primitives satisfy `new()`; a class arg must be non-abstract with a
        // reachable no-arg ctor (or no explicit ctor = default construction).
        if let Some(td) = &arg_td {
            let abstract_ = td.class_flags & crate::metadata::ir::CLASS_FLAG_ABSTRACT != 0;
            if abstract_ || !type_has_no_arg_ctor(ctx, &td.name) {
                bail!("MakeGenericType: `{arg_name}` for type parameter `{param_name}` must have a public parameterless constructor (constraint `new()`)");
            }
        }
    }
    // where <param>: <other type parameter> (bare-typeparam L3-G2.5): the arg for
    // `param` must be assignable to the arg bound to `other_tp`.
    if let Some(other_tp) = &cb.type_param_constraint {
        if let Some(idx) = type_params.iter().position(|p| p == other_tp) {
            if let Some(other_arg) = all_arg_names.get(idx) {
                if !type_name_assignable(ctx, arg_name, other_arg) {
                    bail!("MakeGenericType: `{arg_name}` for type parameter `{param_name}` must be assignable to `{other_arg}` (constraint `{other_tp}`)");
                }
            }
        }
    }
    Ok(())
}

/// True if the class `name` has a reachable no-arg constructor (or no explicit
/// ctor at all → default field construction). Mirrors `builtin_activator_create`.
pub(super) fn type_has_no_arg_ctor(ctx: &VmContext, name: &str) -> bool {
    // fix-nested-ctor-key: shared ctor-name spelling (this function's own `rsplit('.')`
    // left a nested type's `+` prefix on, so every candidate key missed and the answer
    // came out of the `!has_any_ctor` fallback — "yes" for the wrong reason, and "yes"
    // even for a nested type whose only ctor takes arguments).
    let simple = crate::object::TypeDesc::ctor_member_name(name);
    let cand_bare = format!("{name}.{simple}");
    let cand_zero = format!("{name}.{simple}$0");
    let Some(m) = ctx.module() else { return true };
    // Any explicit ctor with args means no-arg construction may be unavailable;
    // but a class with NO ctor at all constructs by default → treat as satisfied.
    let has_bare = m.func_index.contains_key(&cand_bare) || ctx.try_lookup_function(&cand_bare).is_some();
    let has_zero = m.func_index.contains_key(&cand_zero) || ctx.try_lookup_function(&cand_zero).is_some();
    let has_any_ctor = m.func_index.keys().any(|k| k.starts_with(&format!("{name}.{simple}$")));
    has_bare || has_zero || !has_any_ctor
}

/// fix-class-level-typeof : `__class_type_arg(receiver, index) -> Std.Type`
/// — materialize a **class-level** type parameter into a concrete `Std.Type`.
///
/// Mirrors `exec_address::method_type_arg`, but reads the **per-instance**
/// `type_args` (the carrier `DefaultOf` has always used for class-level
/// `default(T)`) instead of the callee frame's `method_type_args`. That carrier
/// is populated by `ObjNew` from the IR instruction's type args, so `Box<int>`
/// and `Box<string>` instances differ at runtime despite sharing one TypeDesc
/// (z42's reified erasure with a per-instance type-arg view).
///
/// **Why a builtin and not a new opcode**: a new `Instruction` would mean a zbc
/// format bump plus the two-nightly bootstrap dance (`self-hosting.md`) for
/// semantics a builtin already delivers — and `Instruction::Builtin` is dispatched
/// by name/BuiltinId in the JIT too, so JIT support comes for free. `__methodof`
/// set the same precedent (see `TypeOpEmitter._emitMethodOf`).
///
/// Lenient by construction (mirrors the whole reflection module's contract):
/// non-object receiver / out-of-range index / empty `type_args` all yield the
/// placeholder constructed type named `"T"` — byte-for-byte what the method-level
/// OOB path yields, and **exactly what class-level `typeof(T)` produced before
/// this change**. Static contexts and type params inherited from a generic base
/// therefore keep their previous behaviour rather than silently resolving to a
/// plausible-but-wrong type.
pub fn class_type_arg(ctx: &VmContext, recv: &Value, idx: i64, owner: &crate::object::string::vstr::Str) -> Value {
    if idx < 0 {
        return make_constructed_type(ctx, "T", &[]);
    }
    let owner = (!owner.is_empty()).then(|| owner.to_string());
    match resolve_class_type_arg(ctx, recv, idx as usize, owner.as_deref()) {
        Some(n) => make_type_from_name(ctx, &n),
        None => make_constructed_type(ctx, "T", &[]),
    }
}

/// Companion of [`builtin_class_type_arg`] for **values**: `__class_default(recv, idx, owner)`
/// → the zero value of the class-level type parameter.
///
/// The `DefaultOf` instruction carries only the parameter index, so it can read nothing but
/// the receiver's *own* `type_args` — and `class DInt : Box<int> {}` has none, which is why
/// `default(T)` yielded `null` instead of `0` (measured). Widening the instruction would mean
/// a zbc format bump plus the two-nightly bootstrap dance; a builtin delivers the same
/// semantics with neither (same precedent as `__class_type_arg`).
pub fn class_default(ctx: &VmContext, recv: &Value, idx: i64, owner: &crate::object::string::vstr::Str) -> Value {
    if idx < 0 {
        return Value::Null;
    }
    let owner = (!owner.is_empty()).then(|| owner.to_string());
    match resolve_class_type_arg(ctx, recv, idx as usize, owner.as_deref()) {
        Some(n) => crate::object::default_value_for(&n),
        None => Value::Null,
    }
}

#[cfg(test)]
fn class_type_arg_of(recv: &Value, idx: usize) -> Option<String> {
    match recv {
        Value::Object(rc) => rc.borrow().type_args().get(idx).cloned(),
        _ => None,
    }
}

/// The class-level resolution itself lives with the VM's type-parameter binding
/// (`interp::type_param_env::class_type_arg`), shared with `o is T` / `new List<T>()` in the
/// same class body — so those and `typeof(T)` read one argument.
fn resolve_class_type_arg(ctx: &VmContext, recv: &Value, idx: usize, owner: Option<&str>) -> Option<String> {
    crate::interp::type_param_env::class_type_arg(ctx, recv, idx, owner)
}

/// The pure name-selection half of `builtin_class_type_arg`: `(receiver, index)`
/// → the bound type-arg tag, or `None` for every degraded case.
///
/// **Split out so the selection logic is unit-testable.** `build_type_ex`
/// degrades to `Value::Null` when `Std.Type` isn't loaded (no z42.core in Rust
/// unit tests), so asserting on the builtin's *returned value* would compare
/// `Null` against `Null` — a gate that can never fire. Asserting on the selected
/// name instead keeps both sides discriminating.
#[cfg(test)]
pub(super) fn class_type_arg_name(recv: &Value, idx: usize) -> Option<String> {
    class_type_arg_of(recv, idx)
}

/// `__type_make_generic(defType, Type[] argTypes) -> Std.Type` — construct a
/// generic type at runtime (`typeof(List<>).MakeGenericType(typeof(int))` →
/// `List<int>`). Reuses `make_constructed_type` (z42's reified type-erasure means
/// no monomorphization/codegen). Validates arg count + `where` constraints
/// (Q3 safety) before constructing.
pub fn type_make_generic(ctx: &VmContext, def: &Value, arg_types_arr: &Value) -> Result<Value, BuiltinError> {
    let arg = |m: String| BuiltinError::Argument(m);
    let def_td = type_handle_of(def)
        .ok_or_else(|| arg("MakeGenericType: receiver is not a type handle".into()))?;
    let type_params: Vec<String> = def_td.type_params().to_vec();
    if type_params.is_empty() {
        return Err(arg(format!("MakeGenericType: `{}` is not a generic type definition", def_td.name)));
    }
    let arg_types: Vec<Value> = match arg_types_arr {
        Value::Array(rc) => rc.borrow().iter_boxed().collect(),
        _ => return Err(arg("MakeGenericType: expected a Type[] of type arguments".into())),
    };
    if arg_types.len() != type_params.len() {
        return Err(arg(format!(
            "MakeGenericType: `{}` expects {} type argument(s), got {}",
            def_td.name, type_params.len(), arg_types.len())));
    }
    let mut arg_names: Vec<String> = Vec::with_capacity(arg_types.len());
    for at in &arg_types {
        // The argument's canonical name (`typeof(List<int>)` → `Std.Collections.List<int>`, not
        // the handle's bare definition name, which dropped nested arguments).
        let n = type_value_name(at)
            .ok_or_else(|| arg("MakeGenericType: unresolvable type argument".into()))?;
        arg_names.push(n);
    }
    // Q3: constraint validation before construction (violations → ArgumentException,
    // matching C# MakeGenericType).
    let constraints = def_td.type_param_constraints();
    for (i, param) in type_params.iter().enumerate() {
        if let Some(cb) = constraints.get(i) {
            validate_type_arg_constraint(ctx, param, cb, &arg_names[i], &type_params, &arg_names)
                .map_err(BuiltinError::Legacy)?;
        }
    }
    Ok(make_constructed_type(ctx, &def_td.name, &arg_names))
}

/// 泛型反射合成迁 builtin ABI v2(P1-8 批次 5)。三个都是纯合成 / 查表——
/// `MakeGenericType` 的约束校验只查类型表,不执行 z42 代码,不标 `reenters`;
/// 违约 → `ArgumentException`(同 C#)。
#[z42_macros::builtins]
pub mod typed {
    use super::*;

    #[builtin(name = "__class_type_arg")]
    pub fn type_arg_of(ctx: &VmContext, recv: &Value, idx: i64, owner: &crate::object::string::vstr::Str) -> Value {
        super::class_type_arg(ctx, recv, idx, owner)
    }

    #[builtin(name = "__class_default")]
    pub fn default_of(ctx: &VmContext, recv: &Value, idx: i64, owner: &crate::object::string::vstr::Str) -> Value {
        super::class_default(ctx, recv, idx, owner)
    }

    #[builtin(name = "__type_make_generic")]
    pub fn make_generic(ctx: &VmContext, def: &Value, arg_types: &Value) -> Result<Value, BuiltinError> {
        type_make_generic(ctx, def, arg_types)
    }
}
