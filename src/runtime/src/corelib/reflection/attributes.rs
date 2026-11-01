use super::*;

// ── Attribute reflection (C3 add-attribute-reflection) ──────────────────────

/// `__type_custom_attributes(typeObj) -> Std.Attribute[]` — live attribute
/// instances for this type's user attributes, in application order.
///
/// Each attribute is built by invoking its compiler-synthesized factory
/// `() => new T(args)` (a normal z42 function) via `run_outcome`. Attribute
/// construction is thus fully statically known (known class, known constructor,
/// constant args baked into the factory body) — no runtime `Activator`/`Invoke`
/// and no generic instantiation. Re-entering the interpreter here is safe:
/// `exec_function` keeps all per-call state in a stack-local `Frame`.
///
/// z42-level `Type.GetCustomAttributes()` caches the returned array, so repeated
/// calls on the same Type yield the same instances. Empty array for a
/// handle-less Type or a type with no attributes.
pub fn type_custom_attributes(ctx: &VmContext, ty: &Value) -> Result<Value, BuiltinError> {
    match type_handle_of(ty) {
        Some(td) => call_attribute_factories(ctx, td.custom_attributes()).map_err(BuiltinError::Legacy),
        None => Ok(ctx.heap().alloc_array(Vec::new())),
    }
}

/// `__method_custom_attributes(qualified) -> Std.Attribute[]` — live attribute
/// instances for the method with the given qualified function name. C3b: the
/// z42 `MethodInfo` passes its hidden `__qualified` name; the builtin resolves
/// the backing `Function` (main module first, then lazy loader) and calls each
/// of its attribute factories. Same factory-call mechanism as the class path.
pub fn method_custom_attributes(ctx: &VmContext, _this: &Value, qualified: &crate::metadata::vstr::Str) -> Result<Value, BuiltinError> {
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
/// hidden `__qualified`). Resolves the class TypeDesc, finds the field in
/// `cold.field_attributes`, and calls each factory. add-field-attribute-reflection.
pub fn field_custom_attributes(ctx: &VmContext, _this: &Value, qualified: &crate::metadata::vstr::Str) -> Result<Value, BuiltinError> {
    let qualified = qualified.to_string();
    // Split "<Class>.<Field>" at the last dot.
    let dot = match qualified.rfind('.') {
        Some(d) => d,
        None => return Ok(ctx.heap().alloc_array(Vec::new())),
    };
    let (class, field) = (&qualified[..dot], &qualified[dot + 1..]);
    let td = ctx
        .module()
        .and_then(|m| m.type_registry.get(class).cloned())
        .or_else(|| ctx.try_lookup_type(class));
    let attrs: Vec<crate::metadata::ir::AttributeRef> = td
        .as_ref()
        .map(|td| {
            td.field_attributes()
                .iter()
                .find(|(n, _)| n.as_ref() == field)
                .map(|(_, refs)| refs.to_vec())
                .unwrap_or_default()
        })
        .unwrap_or_default();
    call_attribute_factories(ctx, &attrs).map_err(BuiltinError::Legacy)
}

/// `__property_custom_attributes(qualified) -> Std.Attribute[]` — live attribute
/// instances for the property named by "<Class>.<Property>". An auto-property desugars
/// to a private backing field `__prop_<Property>`; the compiler (ClassDescBuilder)
/// attaches the property's attributes to that backing field, so this looks them up in
/// `cold.field_attributes` under the `__prop_<Property>` name (reusing the existing
/// field-attribute format — no wire-format bump). Computed properties (no backing
/// field) carry no attributes. add-json-serde.
pub fn property_custom_attributes(ctx: &VmContext, _this: &Value, qualified: &crate::metadata::vstr::Str) -> Result<Value, BuiltinError> {
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
        .and_then(|m| m.type_registry.get(class).cloned())
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
pub fn param_custom_attributes(ctx: &VmContext, _this: &Value, qualified: &crate::metadata::vstr::Str, position: i64) -> Result<Value, BuiltinError> {
    let qualified = qualified.to_string();
    if position < 0 {
        return Ok(ctx.heap().alloc_array(Vec::new()));
    }
    let position = position as usize;
    // Resolve the backing Function (main module first, then lazy loader) and read
    // its SIGS-aligned per-param attrs. wire_index = position + (this offset).
    let lookup = |f: &crate::metadata::ir::Function| {
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
/// (`() => new T(args)`) via `run_outcome`. Shared by the class
/// (`__type_custom_attributes`) and method (`__method_custom_attributes`) paths.
/// Cross-zpkg factories resolve via the lazy loader. Re-entering the interpreter
/// here is safe — `exec_function` keeps per-call state in a stack-local `Frame`.
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
            Some(crate::interp::run_outcome(ctx, &module, &module.functions[idx], &[])?)
        } else if let Some(func) = ctx.try_lookup_function(&a.factory_func) {
            Some(crate::interp::run_outcome(ctx, &module, func.as_ref(), &[])?)
        } else {
            None
        };
        let instance = match outcome {
            Some(crate::interp::ExecOutcome::Returned(v)) => v,
            // An attribute constructor threw: propagate it with its own type
            // through the builtin error channel (`exec_call::builtin` takes it),
            // instead of flattening it into a `Std.Exception` message.
            Some(crate::interp::ExecOutcome::Thrown(v)) => {
                ctx.set_pending_thrown(v);
                anyhow::bail!("attribute constructor threw an exception");
            }
            None => None,
        };
        out.push(instance.unwrap_or(Value::Null));
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
    pub fn method_attrs(ctx: &VmContext, this: &Value, qualified: &crate::metadata::vstr::Str) -> Result<Value, BuiltinError> {
        method_custom_attributes(ctx, this, qualified)
    }

    #[builtin(name = "__field_custom_attributes", reenters)]
    pub fn field_attrs(ctx: &VmContext, this: &Value, qualified: &crate::metadata::vstr::Str) -> Result<Value, BuiltinError> {
        field_custom_attributes(ctx, this, qualified)
    }

    #[builtin(name = "__property_custom_attributes", reenters)]
    pub fn property_attrs(ctx: &VmContext, this: &Value, qualified: &crate::metadata::vstr::Str) -> Result<Value, BuiltinError> {
        property_custom_attributes(ctx, this, qualified)
    }

    #[builtin(name = "__param_custom_attributes", reenters)]
    pub fn param_attrs(ctx: &VmContext, this: &Value, qualified: &crate::metadata::vstr::Str, position: i64) -> Result<Value, BuiltinError> {
        param_custom_attributes(ctx, this, qualified, position)
    }
}
