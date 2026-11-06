use super::*;

// ── Property reflection ─────────────────────────────────────────────────────

/// `__type_properties(typeObj) -> PropertyInfo[]` — properties derived from the
/// `get_<X>` / `set_<X>` accessor-method convention (auto-properties desugar to
/// field + get_/set_ methods). No persisted PropertyDesc metadata and no
/// wire-format change: the accessor names already live in the vtable /
/// own_methods, and the property type comes from the accessor signature (same
/// source as MethodInfo). Getter + setter for the same name merge into one
/// PropertyInfo (CanRead && CanWrite). Empty for a handle-less Type.
pub fn type_properties(ctx: &VmContext, ty: &Value) -> Result<Value, BuiltinError> {
    let td = match type_handle_of(ty) {
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
    let subst = member_subst(ty);
    let mut out = Vec::with_capacity(props.len());
    for p in &props {
        out.push(alloc_property_info(ctx, &td.name, p, &subst).map_err(BuiltinError::Legacy)?);
    }
    Ok(ctx.heap().alloc_array(out))
}

/// Materialise one `PropertyInfo` for property `p` declared on `class`.
/// Shared by `__type_properties` (GetProperties) and `__propertyof`
/// (add-field-property-reference) so both stamp identical fields —
/// including the `__qualified` identity (`<class>.<prop>`) that value
/// equality (`PropertyInfo.==`) is defined on.
///
/// `subst` carries the queried type's generic substitution (a constructed
/// Type reports member types with its arguments substituted); `__propertyof`
/// references the **definition** (no type arguments accepted), so it passes
/// `&None` and gets the definition tag, mirroring the shared emission.
pub(super) fn alloc_property_info(
    ctx: &VmContext, class: &str, p: &PropAccum, subst: &Option<MemberSubst>,
) -> Result<Value> {
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
    let projected = p.getter_qualified.as_ref().or(p.setter_qualified.as_ref())
        .and_then(|key| subst.as_ref().filter(|subst| subst.applies_to(key)))
        .and_then(|subst| projected_member_type(ctx, type_tag, subst, &[]));
    let property_type = projected.unwrap_or_else(|| make_type_from_name(ctx, type_tag));
    alloc_named(
        ctx,
        STD_REFLECTION_PROPERTYINFO,
        &[
            ("Name", Value::Str(p.name.clone().into())),
            ("PropertyType", property_type),
            ("CanRead", Value::Bool(p.getter_type.is_some())),
            ("CanWrite", Value::Bool(p.setter_type.is_some())),
            ("CanReadPublicInstance", Value::Bool(p.getter_public_instance)),
            ("CanWritePublicInstance", Value::Bool(p.setter_public_instance)),
            ("__getterQualified", getter_q),
            ("__setterQualified", setter_q),
            ("__qualified", Value::Str(format!("{class}.{}", p.name).into())),
        ],
    )
}

/// Accumulator for merging a property's getter + setter into one PropertyInfo.
pub(super) struct PropAccum {
    name: String,
    getter_type: Option<String>,
    setter_type: Option<String>,
    getter_public_instance: bool,
    setter_public_instance: bool,
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
    let function = resolve_reflected_function(ctx, qualified);
    let sig = function.as_ref().map(AsRef::as_ref);
    if is_get {
        let (ty, public_instance) = match sig {
            Some(f) => {
                if f.param_count.saturating_sub(usize::from(!f.is_static)) != 0 {
                    return; // get_X(args) — a regular method, not a property
                }
                (f.ret_type.clone(), !f.is_static && f.visibility == 0)
            }
            None => ("?".to_string(), false),
        };
        let p = upsert_prop(props, prop_name);
        p.getter_type = Some(ty);
        p.getter_public_instance = public_instance;
        p.getter_qualified = Some(qualified.to_string());
    } else {
        let (ty, public_instance) = match sig {
            Some(f) => {
                let base = usize::from(!f.is_static);
                if f.param_count.saturating_sub(base) != 1 {
                    return; // set_X with != 1 value param — a regular method
                }
                let ty = f.param_types().get(base).cloned().unwrap_or_else(|| "?".to_string());
                (ty, !f.is_static && f.visibility == 0)
            }
            None => ("?".to_string(), false),
        };
        let p = upsert_prop(props, prop_name);
        p.setter_type = Some(ty);
        p.setter_public_instance = public_instance;
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
        getter_public_instance: false,
        setter_public_instance: false,
        getter_qualified: None,
        setter_qualified: None,
    });
    let last = props.len() - 1;
    &mut props[last]
}

/// 成员列举迁 builtin ABI v2(P1-8 批次 5):纯合成(FieldInfo / MethodInfo /
/// ConstructorInfo / PropertyInfo 数组),不执行 z42 代码,不压轻帧。
#[z42_macros::builtins]
pub mod typed {
    use super::*;

    #[builtin(name = "__type_properties")]
    pub fn properties_of(ctx: &VmContext, ty: &Value) -> Result<Value, BuiltinError> {
        type_properties(ctx, ty)
    }

    #[builtin(name = "__propertyof")]
    pub fn property_of(ctx: &VmContext, qualified: &crate::object::string::vstr::Str) -> Result<Value, BuiltinError> {
        propertyof(ctx, qualified)
    }
}

/// `__propertyof("<declaring FQN>.<Prop>") -> Std.Reflection.PropertyInfo`
/// (add-field-property-reference, sibling of `__methodof` / `__fieldof`).
///
/// The compiler resolves the property's declaring class at bind time; here we
/// re-derive the PropertyInfo exactly the way `GetProperties()` would on that
/// class: same vtable + own_methods scan, same accessor arity validation
/// (a `get_X(args)` regular method doesn't count), getter + setter merged.
/// A miss (no class, or no accessor pair for that name) is a compiler bug —
/// loud `Internal`, never a half-populated object.
pub fn propertyof(ctx: &VmContext, qualified: &crate::object::string::vstr::Str) -> Result<Value, BuiltinError> {
    let qualified = qualified.to_string();
    let (class, prop) = match qualified.rsplit_once('.') {
        Some((c, p)) => (c, p),
        None => (qualified.as_str(), ""),
    };
    let td = ctx
        .module()
        .and_then(|m| m.type_registry.get(class).cloned())
        .or_else(|| ctx.try_lookup_type(class));
    let td = match td {
        Some(t) => t,
        None => {
            return Err(BuiltinError::Internal(format!(
                "__propertyof: no type named `{class}` — the compiler emitted a property \
                 reference that does not resolve (this is a z42c bug, not a source error)"
            )));
        }
    };
    // Same scan + dedup + accumulation as `type_properties`, filtered to this
    // one property's accessors (`get_<prop>` / `set_<prop>`).
    let mut props: Vec<PropAccum> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for (simple, qualified) in &td.vtable {
        if is_accessor_of(simple, prop) && seen.insert(qualified.clone()) {
            accumulate_property(ctx, simple, qualified, &mut props);
        }
    }
    for qualified in td.own_methods() {
        let q = qualified.to_string();
        if seen.insert(q.clone()) {
            let simple = simple_method_name(&q).to_string();
            if is_accessor_of(&simple, prop) {
                accumulate_property(ctx, &simple, &q, &mut props);
            }
        }
    }
    if props.is_empty() {
        return Err(BuiltinError::Internal(format!(
            "__propertyof: no accessor pair named `{prop}` on `{class}` — the compiler \
             emitted a property reference that does not resolve (this is a z42c bug, not a \
             source error)"
        )));
    }
    // One property name ⇒ at most one accumulator (upsert by name). No type
    // substitution: propertyof references the definition (no type arguments).
    alloc_property_info(ctx, &td.name, &props[0], &None).map_err(BuiltinError::Legacy)
}

/// `simple` is the `get_<X>` / `set_<X>` accessor of property `prop`?
fn is_accessor_of(simple: &str, prop: &str) -> bool {
    simple
        .strip_prefix("get_")
        .or_else(|| simple.strip_prefix("set_"))
        .is_some_and(|n| n == prop)
}
