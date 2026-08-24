use super::*;

// ── Property reflection ─────────────────────────────────────────────────────

/// `__type_properties(typeObj) -> PropertyInfo[]` — properties derived from the
/// `get_<X>` / `set_<X>` accessor-method convention (auto-properties desugar to
/// field + get_/set_ methods). No persisted PropertyDesc metadata and no
/// wire-format change: the accessor names already live in the vtable /
/// own_methods, and the property type comes from the accessor signature (same
/// source as MethodInfo). Getter + setter for the same name merge into one
/// PropertyInfo (CanRead && CanWrite). Empty for a handle-less Type.
pub fn builtin_type_properties(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let td = match type_handle(args) {
        Some(t) => t,
        None => return Ok(ctx.heap().alloc_array(Vec::new())),
    };
    // vtable (virtual / inherited, base-first) then declared non-virtual methods
    // — same ordering + dedup as GetMethods, so property order is stable.
    let mut props: Vec<PropAccum> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for (simple, qualified) in &td.vtable {
        if seen.insert(qualified.clone()) {
            accumulate_property(ctx, simple, qualified, &mut props);
        }
    }
    for qualified in td.own_methods() {
        let q = qualified.to_string();
        if seen.insert(q.clone()) {
            let simple = simple_method_name(&q).to_string();
            accumulate_property(ctx, &simple, &q, &mut props);
        }
    }
    let mut out = Vec::with_capacity(props.len());
    for p in &props {
        let type_tag = p
            .getter_type
            .as_deref()
            .or(p.setter_type.as_deref())
            .unwrap_or("?");
        let getter_q = match &p.getter_qualified {
            Some(q) => Value::Str(q.clone().into()),
            None => Value::Null,
        };
        let setter_q = match &p.setter_qualified {
            Some(q) => Value::Str(q.clone().into()),
            None => Value::Null,
        };
        out.push(alloc_named(
            ctx,
            STD_REFLECTION_PROPERTYINFO,
            &[
                ("Name", Value::Str(p.name.clone().into())),
                ("PropertyType", make_type_from_name(ctx, type_tag)),
                ("CanRead", Value::Bool(p.getter_type.is_some())),
                ("CanWrite", Value::Bool(p.setter_type.is_some())),
                ("__getterQualified", getter_q),
                ("__setterQualified", setter_q),
            ],
        )?);
    }
    Ok(ctx.heap().alloc_array(out))
}

/// Accumulator for merging a property's getter + setter into one PropertyInfo.
pub(super) struct PropAccum {
    name: String,
    getter_type: Option<String>,
    setter_type: Option<String>,
    /// Qualified name of the `get_<Name>` accessor (for reflective GetValue).
    getter_qualified: Option<String>,
    /// Qualified name of the `set_<Name>` accessor (for reflective SetValue).
    setter_qualified: Option<String>,
}

/// Classify one method as a property getter / setter (by `get_` / `set_` prefix)
/// and merge it into `props`. A resolvable accessor must have the right logical
/// arity (getter 0, setter 1, ignoring `this`) — otherwise it's a regular method
/// that merely shares the prefix and is skipped. Unresolvable signatures
/// (extern / native getters) are accepted leniently with a best-effort type.
pub(super) fn accumulate_property(ctx: &VmContext, simple: &str, qualified: &str, props: &mut Vec<PropAccum>) {
    let (is_get, prop_name) = match (simple.strip_prefix("get_"), simple.strip_prefix("set_")) {
        (Some(n), _) => (true, n),
        (_, Some(n)) => (false, n),
        _ => return,
    };
    if prop_name.is_empty() {
        return;
    }
    let sig = resolve_func_sig(ctx, qualified);
    if is_get {
        let ty = match &sig {
            Some((pc, ret, is_static, _, _, _, _, _, _, _)) => {
                if pc.saturating_sub(if *is_static { 0 } else { 1 }) != 0 {
                    return; // get_X(args) — a regular method, not a property
                }
                ret.clone()
            }
            None => "?".to_string(),
        };
        let p = upsert_prop(props, prop_name);
        p.getter_type = Some(ty);
        p.getter_qualified = Some(qualified.to_string());
    } else {
        let ty = match &sig {
            Some((pc, _, is_static, ptypes, _, _, _, _, _, _)) => {
                let base = if *is_static { 0 } else { 1 };
                if pc.saturating_sub(base) != 1 {
                    return; // set_X with != 1 value param — a regular method
                }
                ptypes.get(base).cloned().unwrap_or_else(|| "?".to_string())
            }
            None => "?".to_string(),
        };
        let p = upsert_prop(props, prop_name);
        p.setter_type = Some(ty);
        p.setter_qualified = Some(qualified.to_string());
    }
}

/// Find-or-insert a property accumulator by name, preserving first-seen order.
pub(super) fn upsert_prop<'a>(props: &'a mut Vec<PropAccum>, name: &str) -> &'a mut PropAccum {
    if let Some(i) = props.iter().position(|p| p.name == name) {
        return &mut props[i];
    }
    props.push(PropAccum {
        name: name.to_string(),
        getter_type: None,
        setter_type: None,
        getter_qualified: None,
        setter_qualified: None,
    });
    let last = props.len() - 1;
    &mut props[last]
}
