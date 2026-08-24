use super::*;

// ── Runtime generic instantiation (G1, plan-generic-reflection) ──────────────

/// Extract a resolvable name from a generic-type-argument `Std.Type` value:
/// handle-backed → the TypeDesc FQ name; array → `<elem>[]`; else `__fullName`.
pub(super) fn type_arg_name(v: &Value) -> Option<String> {
    if let Value::Object(rc) = v {
        if let NativeData::TypeHandle(td) = &rc.borrow().native {
            return Some(td.name.clone());
        }
    }
    // Array Type carries a non-empty `__elementName` → reconstruct `<elem>[]`.
    // (Non-array Types initialize the slot to "" — guard against it, else a
    // primitive like `typeof(int)` would wrongly become `[]` → an Array type.)
    if let Value::Str(elem) = read_obj_slot(v, "__elementName") {
        if !elem.is_empty() {
            return Some(format!("{elem}[]"));
        }
    }
    match read_obj_slot(v, "__fullName") {
        Value::Str(s) => Some(s.to_string()),
        _ => None,
    }
}

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
    let target_simple = constraint.rsplit('.').next().unwrap_or(constraint);
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut stack = vec![arg_name.to_string()];
    while let Some(cur) = stack.pop() {
        if !seen.insert(cur.clone()) {
            continue;
        }
        if cur.rsplit('.').next().unwrap_or(&cur) == target_simple {
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
    cb: &crate::metadata::bytecode::ConstraintBundle,
    arg_name: &str,
    type_params: &[String],
    all_arg_names: &[String],
) -> Result<()> {
    let arg_td = resolve_td(ctx, arg_name);
    let is_value = arg_td
        .as_ref()
        .map(|td| td.class_flags & crate::metadata::bytecode::CLASS_FLAG_STRUCT != 0)
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
            let abstract_ = td.class_flags & crate::metadata::bytecode::CLASS_FLAG_ABSTRACT != 0;
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
    let simple = name.rsplit('.').next().unwrap_or(name);
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

/// `__type_make_generic(defType, Type[] argTypes) -> Std.Type` — construct a
/// generic type at runtime (`typeof(List<>).MakeGenericType(typeof(int))` →
/// `List<int>`). Reuses `make_constructed_type` (z42's reified type-erasure means
/// no monomorphization/codegen). Validates arg count + `where` constraints
/// (Q3 safety) before constructing.
pub fn builtin_type_make_generic(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let def_td = type_handle(args)
        .ok_or_else(|| anyhow::anyhow!("MakeGenericType: receiver is not a type handle"))?;
    let type_params: Vec<String> = def_td.type_params().to_vec();
    if type_params.is_empty() {
        bail!("MakeGenericType: `{}` is not a generic type definition", def_td.name);
    }
    let arg_types: Vec<Value> = match args.get(1) {
        Some(Value::Array(rc)) => rc.borrow().iter_boxed().collect(),
        _ => bail!("MakeGenericType: expected a Type[] of type arguments"),
    };
    if arg_types.len() != type_params.len() {
        bail!(
            "MakeGenericType: `{}` expects {} type argument(s), got {}",
            def_td.name,
            type_params.len(),
            arg_types.len()
        );
    }
    let mut arg_names: Vec<String> = Vec::with_capacity(arg_types.len());
    for at in &arg_types {
        let n = type_arg_name(at)
            .ok_or_else(|| anyhow::anyhow!("MakeGenericType: unresolvable type argument"))?;
        arg_names.push(n);
    }
    // Q3: constraint validation before construction.
    let constraints = def_td.type_param_constraints();
    for (i, param) in type_params.iter().enumerate() {
        if let Some(cb) = constraints.get(i) {
            validate_type_arg_constraint(ctx, param, cb, &arg_names[i], &type_params, &arg_names)?;
        }
    }
    Ok(make_constructed_type(ctx, &def_td.name, &arg_names))
}
