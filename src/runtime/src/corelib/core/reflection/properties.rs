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
    let scopes = MemberScopes::new(ctx, ty, &td);
    let mut roots = ProjectionRoots::new(ctx);
    let mut out = Vec::with_capacity(props.len());
    for p in &props {
        let subst = p.getter_qualified.as_deref().or(p.setter_qualified.as_deref())
            .and_then(|key| scopes.for_method(key));
        let info = alloc_property_info(ctx, &td.name, p, subst).map_err(BuiltinError::Legacy)?;
        out.push(if scopes.has_inherited() { roots.pin(info) } else { info });
    }
    Ok(ctx.heap().alloc_array(out))
}

/// Materialise one `PropertyInfo` for property `p` declared on `class`.
/// Shared by `__type_properties` (GetProperties) and `__propertyof`
/// (add-field-property-reference) so both stamp identical fields —
/// including the `__qualified` identity (`<class>.<prop>`) that value
/// equality (`PropertyInfo.==`) is defined on.
///
/// `subst` carries the accessor's declaring type arguments as seen from the
/// queried Type. `__propertyof` references the definition and passes `None`.
pub(super) fn alloc_property_info(
    ctx: &VmContext, class: &str, p: &PropAccum, subst: Option<&MemberSubst>,
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
    let projected = subst.and_then(|subst| projected_member_type(ctx, type_tag, subst, &[]));
    let property_type = projected.unwrap_or_else(|| make_type_from_name(ctx, type_tag));
    // The declaring class is the accessor's class (vtable / own_methods store the
    // DECLARING implementation), NOT the queried class — an inherited property keeps
    // its base declarer. `__qualified` feeds `MemberInfo.Owner` and value equality,
    // both documented as declaring-class-scoped (C# semantics).
    let declaring = p
        .getter_qualified
        .as_deref()
        .or(p.setter_qualified.as_deref())
        .and_then(|acc| acc.rfind('.').map(|i| &acc[..i]))
        .unwrap_or(class);
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
            ("__qualified", Value::Str(format!("{declaring}.{}", p.name).into())),
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
                let logical = f.param_count.saturating_sub(usize::from(!f.is_static));
                if logical != 0 && prop_name != "Item" {
                    return; // get_X(args) — a regular method, not a property
                }
                // indexer (get_Item(args)) — a parameterized property (C# model);
                // z42 indexers are `this[...]`-only, so "Item" is the sole name.
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
                let logical = f.param_count.saturating_sub(base);
                // Plain property: exactly the value param. Indexer (set_Item):
                // index params + the trailing value param — the value type is the
                // LAST param; everything else mirrors the plain shape.
                if logical == 0 || (logical != 1 && prop_name != "Item") {
                    return; // set_X without exactly one value param — a regular method
                }
                let ty = f.param_types().get(f.param_count - 1).cloned().unwrap_or_else(|| "?".to_string());
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

    #[builtin(name = "__type_properties", reenters)]
    pub fn properties_of(ctx: &VmContext, ty: &Value) -> Result<Value, BuiltinError> {
        type_properties(ctx, ty)
    }

    #[builtin(name = "__propertyof")]
    pub fn property_of(ctx: &VmContext, qualified: &crate::object::string::vstr::Str) -> Result<Value, BuiltinError> {
        propertyof(ctx, qualified)
    }

    #[builtin(name = "__property_get_method", reenters)]
    pub fn property_get_method(ctx: &VmContext, this: &Value) -> Result<Value, BuiltinError> {
        accessor_method(ctx, this, true)
    }

    #[builtin(name = "__property_set_method", reenters)]
    pub fn property_set_method(ctx: &VmContext, this: &Value) -> Result<Value, BuiltinError> {
        accessor_method(ctx, this, false)
    }

    #[builtin(name = "__property_index_parameters", reenters)]
    pub fn index_parameters(ctx: &VmContext, this: &Value) -> Result<Value, BuiltinError> {
        property_index_parameters(ctx, this).map_err(BuiltinError::Legacy)
    }
}

/// `__property_index_parameters(pi) -> ParameterInfo[]` — the property's index
/// parameters (C# `PropertyInfo.GetIndexParameters`): the getter's parameter
/// list, or a write-only indexer's setter list minus the trailing value param.
/// Plain properties (no accessor parameters) → empty array. Each parameter's
/// `__method` host is the accessor `MethodInfo` (the family convention).
fn property_index_parameters(ctx: &VmContext, pi: &Value) -> Result<Value> {
    let getter = match read_type_slot_of(pi, "__getterQualified") {
        Value::Str(s) => s.to_string(),
        _ => String::new(),
    };
    let setter = match read_type_slot_of(pi, "__setterQualified") {
        Value::Str(s) => s.to_string(),
        _ => String::new(),
    };
    // Prefer the getter (its params ARE the index params); write-only → setter
    // minus the trailing value. Neither accessor → empty.
    let mut params: Vec<Value> = Vec::new();
    let mut host: Option<Value> = None;
    if !getter.is_empty() {
        host = accessor_method(ctx, pi, true).ok();
        params = accessor_params(ctx, &getter)?;
    } else if !setter.is_empty() {
        host = accessor_method(ctx, pi, false).ok();
        params = accessor_params(ctx, &setter)?;
        params.pop();
    }
    if let Some(h) = &host {
        for p in &params {
            write_published_slot(ctx, p, "__method", h);
        }
    }
    Ok(ctx.heap().alloc_array(params))
}

/// Build the `MethodInfo` for a property's `get_<X>` / `set_<X>` accessor from the
/// qualified name stamped at property-build time (`__getterQualified` /
/// `__setterQualified`); absent accessor → null. `build_method_info` resolves the
/// backing function, so the result carries the full signature (params / staticness /
/// visibility) — `GetGetMethod().Invoke(obj, args)` is the same path
/// `PropertyInfo.GetValue` walks.
fn accessor_method(ctx: &VmContext, prop: &Value, getter: bool) -> Result<Value, BuiltinError> {
    let slot = if getter { "__getterQualified" } else { "__setterQualified" };
    match read_type_slot_of(prop, slot) {
        Value::Str(qualified) => {
            let qualified = qualified.to_string();
            let simple = simple_method_name(&qualified);
            build_method_info(ctx, simple, &qualified, false).map_err(BuiltinError::Legacy)
        }
        _ => Ok(Value::Null),
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
    let td = ctx.loader().resolve_type(class);
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
    alloc_property_info(ctx, &td.name, &props[0], None).map_err(BuiltinError::Legacy)
}

/// `simple` is the `get_<X>` / `set_<X>` accessor of property `prop`?
fn is_accessor_of(simple: &str, prop: &str) -> bool {
    simple
        .strip_prefix("get_")
        .or_else(|| simple.strip_prefix("set_"))
        .is_some_and(|n| n == prop)
}
