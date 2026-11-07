use super::*;

// ── Attribute reflection (C3 add-attribute-reflection) ──────────────────────

/// `__type_custom_attributes(typeObj) -> Std.Attribute[]` — live attribute
/// instances for this type's user attributes, in application order.
///
/// Each attribute is built by invoking its compiler-synthesized factory
/// `() => new T(args)` (a normal z42 function) via the registered engine. Attribute
/// construction is thus fully statically known (known class, known constructor,
/// constant args baked into the factory body) — no runtime `Activator`/`Invoke`
/// and no generic instantiation. Re-entering the registered engine keeps each activation in its own frame.
///
/// The cache dependency and result live in one immutable managed array. Publishing
/// one reference prevents concurrent readers from pairing an old result with a new
/// descriptor. The Type itself owns the result; the VM identity cache stays weak.
pub fn type_custom_attributes(ctx: &VmContext, ty: &Value) -> Result<Value, BuiltinError> {
    let descriptor = type_handle_of(ty);
    let owner = Value::I64(descriptor.as_ref().map_or(0, |td| Arc::as_ptr(td) as usize as i64));
    if let Value::Array(state) = read_type_slot_of(ty, "__attributeCache") {
        let state = state.borrow();
        if state.get(0) == Some(owner) {
            if let Some(attributes @ Value::Array(_)) = state.get(1) {
                return Ok(attributes);
            }
        }
    }
    let mut roots = ProjectionRoots::new(ctx);
    let attributes = roots.pin(match descriptor {
        Some(td) => call_attribute_factories(ctx, td.custom_attributes()).map_err(BuiltinError::Legacy)?,
        None => ctx.heap().alloc_array(Vec::new()),
    });
    if let Value::Object(object) = ty {
        if let Some(&slot) = object.type_desc().field_index.get("__attributeCache") {
            let state = ctx.heap().alloc_array(vec![owner, attributes]);
            let wrote = object.borrow().try_set_field_value(slot, &state)
                .map_err(BuiltinError::Legacy)?;
            wrote.with_barrier_value(&state, |value| ctx.heap().write_barrier_field(ty, slot, value));
        }
    }
    Ok(attributes)
}

/// `__method_custom_attributes(qualified) -> Std.Attribute[]` — live attribute
/// instances for the method with the given qualified function name. C3b: the
/// z42 `MethodInfo` passes its hidden `__qualified` name; the builtin resolves
/// the backing `Function` (main module first, then lazy loader) and calls each
/// of its attribute factories. Same factory-call mechanism as the class path.
pub fn method_custom_attributes(ctx: &VmContext, _this: &Value, qualified: &crate::object::string::vstr::Str) -> Result<Value, BuiltinError> {
    let qualified = qualified.to_string();
    let attrs: Vec<crate::metadata::ir::AttributeRef> = ctx
        .module()
        .and_then(|m| {
            m.func_index
                .get(qualified.as_str())
                .and_then(|&i| m.functions.get(i))
                .map(|f| f.custom_attributes().to_vec())
        })
        .or_else(|| ctx.try_lookup_function(&qualified).map(|f| f.custom_attributes().to_vec()))
        .unwrap_or_default();
    call_attribute_factories(ctx, &attrs).map_err(BuiltinError::Legacy)
}

/// `__field_custom_attributes(qualified) -> Std.Attribute[]` — live attribute
/// instances for the field named by "<Class>.<Field>" (FieldInfo passes its
/// hidden `__qualified`). Instance fields can be listed on a derived class,
/// so use the declaring class's attribute table before calling each factory.
pub fn field_custom_attributes(ctx: &VmContext, _this: &Value, qualified: &crate::object::string::vstr::Str) -> Result<Value, BuiltinError> {
    let qualified = qualified.to_string();
    // Split "<Class>.<Field>" at the last dot.
    let dot = match qualified.rfind('.') {
        Some(d) => d,
        None => return Ok(ctx.heap().alloc_array(Vec::new())),
    };
    let (class, field) = (&qualified[..dot], &qualified[dot + 1..]);
    let resolve = |name: &str| {
        ctx.module()
            .and_then(|m| m.type_registry.get(name).cloned().map(|td| ctx.current_type_desc(td)))
            .or_else(|| ctx.try_lookup_type(name))
    };
    let mut current = resolve(class);
    let mut seen = HashSet::new();
    while let Some(td) = current {
        if let Some((_, refs)) = td.field_attributes().iter().find(|(name, _)| name.as_ref() == field) {
            return call_attribute_factories(ctx, refs).map_err(BuiltinError::Legacy);
        }
        let Some(base) = td.base_name.as_deref().filter(|name| *name != STD_OBJECT) else {
            break;
        };
        if td.own_fields().iter().any(|slot| slot.name.as_ref() == field)
            || td.static_fields().iter().any(|slot| slot.name == field)
            || !seen.insert(td.name.clone())
        {
            break;
        }
        current = resolve(base);
    }
    Ok(ctx.heap().alloc_array(Vec::new()))
}

/// `__property_custom_attributes(qualified) -> Std.Attribute[]` — live attribute
/// instances for the property named by "<Class>.<Property>". An auto-property desugars
/// to a private backing field `__prop_<Property>`; the compiler (ClassDescBuilder)
/// attaches the property's attributes to that backing field, so this looks them up in
/// `cold.field_attributes` under the `__prop_<Property>` name (reusing the existing
/// field-attribute format — no wire-format bump). Computed properties (no backing
/// field) carry no attributes. add-json-serde.
pub fn property_custom_attributes(ctx: &VmContext, _this: &Value, qualified: &crate::object::string::vstr::Str) -> Result<Value, BuiltinError> {
    let qualified = qualified.to_string();
    // Accept the accessor-qualified name "<Class>.get_<Prop>" / "<Class>.set_<Prop>"
    // (z42 JsonReflect passes PropertyInfo.__getterQualified directly — avoids z42-side
    // string manipulation on the cross-package field). Split at the last dot, strip the
    // get_/set_ accessor prefix → the property name → its backing field `__prop_<Prop>`.
    let dot = match qualified.rfind('.') {
        Some(d) => d,
        None => return Ok(ctx.heap().alloc_array(Vec::new())),
    };
    let class = &qualified[..dot];
    let accessor = &qualified[dot + 1..];
    let prop = accessor
        .strip_prefix("get_")
        .or_else(|| accessor.strip_prefix("set_"))
        .unwrap_or(accessor);
    let backing = format!("__prop_{prop}");
    let td = ctx
        .module()
        .and_then(|m| m.type_registry.get(class).cloned().map(|td| ctx.current_type_desc(td)))
        .or_else(|| ctx.try_lookup_type(class));
    let attrs: Vec<crate::metadata::ir::AttributeRef> = td
        .as_ref()
        .map(|td| {
            td.field_attributes()
                .iter()
                .find(|(n, _)| n.as_ref() == backing.as_str())
                .map(|(_, refs)| refs.to_vec())
                .unwrap_or_default()
        })
        .unwrap_or_default();
    call_attribute_factories(ctx, &attrs).map_err(BuiltinError::Legacy)
}

/// `__param_custom_attributes(qualified, position) -> Std.Attribute[]` — live
/// attribute instances for parameter `position` (source index, excluding the
/// implicit `this`) of the method/function named by `qualified`. The z42
/// `ParameterInfo` passes its hidden `__qualified` + `__position`. The backing
/// `Function`'s `param_attributes` are SIGS-aligned (include the `this` slot),
/// so the wire index = position + (is_static ? 0 : 1). add-parameter-attribute-reflection.
pub fn param_custom_attributes(ctx: &VmContext, _this: &Value, qualified: &crate::object::string::vstr::Str, position: i64) -> Result<Value, BuiltinError> {
    let qualified = qualified.to_string();
    if position < 0 {
        return Ok(ctx.heap().alloc_array(Vec::new()));
    }
    let position = position as usize;
    // Resolve the backing Function (main module first, then lazy loader) and read
    // its SIGS-aligned per-param attrs. wire_index = position + (this offset).
    let lookup = |f: &crate::vm::runtime::Function| {
        let wire_index = position + if f.is_static { 0 } else { 1 };
        f.param_attributes().get(wire_index).map(|a| a.to_vec())
    };
    let attrs: Vec<crate::metadata::ir::AttributeRef> = ctx
        .module()
        .and_then(|m| {
            m.func_index
                .get(qualified.as_str())
                .and_then(|&i| m.functions.get(i))
                .and_then(lookup)
        })
        .or_else(|| ctx.try_lookup_function(&qualified).and_then(|f| lookup(&f)))
        .unwrap_or_default();
    call_attribute_factories(ctx, &attrs).map_err(BuiltinError::Legacy)
}

/// Build live attribute instances by invoking each synthesized factory function
/// (`() => new T(args)`) via the registered engine. Shared by the class
/// (`__type_custom_attributes`) and method (`__method_custom_attributes`) paths.
/// Cross-zpkg factories resolve via the lazy loader. Re-entering the registered engine keeps each activation in its own frame.
pub(super) fn call_attribute_factories(
    ctx: &VmContext,
    attrs: &[crate::metadata::ir::AttributeRef],
) -> Result<Value> {
    if attrs.is_empty() {
        return Ok(ctx.heap().alloc_array(Vec::new()));
    }
    let module = match ctx.module() {
        Some(m) => m.clone(),
        None => return Ok(ctx.heap().alloc_array(Vec::new())),
    };
    let mut roots = ProjectionRoots::new(ctx);
    let mut out = Vec::with_capacity(attrs.len());
    for a in attrs {
        // attribute-handler-registry: internal sentinels use a `$` prefix that cannot
        // collide with real attribute type names (`$` is not a valid identifier char):
        // `$Deprecated` (PR5), `$Default` (PR6), `$Caller:*` (PR6b). They carry compiler
        // metadata, not user attributes, and have no factory function — skip them so
        // `GetCustomAttributes()` doesn't surface a bogus `Null` element for each.
        if a.type_name.starts_with('$') {
            continue;
        }
        let outcome = if let Some(&idx) = module.func_index.get(a.factory_func.as_str()) {
            Some(crate::vm::exec::engine::invoke(ctx, &module, &module.functions[idx], &[])?)
        } else if let Some(func) = ctx.try_lookup_function(&a.factory_func) {
            Some(crate::vm::exec::engine::invoke(ctx, &module, func.as_ref(), &[])?)
        } else {
            None
        };
        let instance = match outcome {
            Some(crate::vm::exec::call::outcome::ExecOutcome::Returned(v)) => v,
            // An attribute constructor threw: propagate it with its own type
            // through the builtin error channel (`exec_call::builtin` takes it),
            // instead of flattening it into a `Std.Exception` message.
            Some(crate::vm::exec::call::outcome::ExecOutcome::Thrown(v)) => {
                ctx.set_pending_thrown(v);
                anyhow::bail!("attribute constructor threw an exception");
            }
            None => None,
        };
        out.push(roots.pin(instance.unwrap_or(Value::Null)));
    }
    Ok(ctx.heap().alloc_array(out))
}

/// 反射 attribute 查询迁 builtin ABI v2(P1-8 批次 5)。attribute factory 是**用户写的
/// z42 构造函数**(`call_attribute_factories` 重入解释器),全部标 `reenters`(压轻帧,
/// factory 抛的异常栈与旧表一致)。
#[z42_macros::builtins]
pub mod typed {
    use super::*;

    #[builtin(name = "__type_custom_attributes", reenters)]
    pub fn type_attrs(ctx: &VmContext, ty: &Value) -> Result<Value, BuiltinError> {
        type_custom_attributes(ctx, ty)
    }

    #[builtin(name = "__method_custom_attributes", reenters)]
    pub fn method_attrs(ctx: &VmContext, this: &Value, qualified: &crate::object::string::vstr::Str) -> Result<Value, BuiltinError> {
        method_custom_attributes(ctx, this, qualified)
    }

    #[builtin(name = "__field_custom_attributes", reenters)]
    pub fn field_attrs(ctx: &VmContext, this: &Value, qualified: &crate::object::string::vstr::Str) -> Result<Value, BuiltinError> {
        field_custom_attributes(ctx, this, qualified)
    }

    #[builtin(name = "__property_custom_attributes", reenters)]
    pub fn property_attrs(ctx: &VmContext, this: &Value, qualified: &crate::object::string::vstr::Str) -> Result<Value, BuiltinError> {
        property_custom_attributes(ctx, this, qualified)
    }

    #[builtin(name = "__param_custom_attributes", reenters)]
    pub fn param_attrs(ctx: &VmContext, this: &Value, qualified: &crate::object::string::vstr::Str, position: i64) -> Result<Value, BuiltinError> {
        param_custom_attributes(ctx, this, qualified, position)
    }
}
