use super::*;

// ── Field reflection ────────────────────────────────────────────────────────

/// True if `name` is a compiler-synthesized auto-property backing field
/// (`__prop_<PropName>`, emitted by the IrGen auto-property stubs). These are
/// hidden from `GetFields()` — the property surfaces via `GetProperties()`,
/// mirroring C#'s hidden `<Name>k__BackingField`.
pub(super) fn is_autoprop_backing(name: &str) -> bool {
    name.starts_with("__prop_")
}

/// `__type_fields(typeObj) -> FieldInfo[]` — instance fields (incl. inherited;
/// base-first), each `FieldInfo { Name, FieldType: Type }`. Auto-property
/// backing fields (`__prop_*`) are excluded.
pub fn type_fields(ctx: &VmContext, ty: &Value) -> Result<Value, BuiltinError> {
    let td = match type_handle_of(ty) {
        Some(t) => t,
        None => return Ok(ctx.heap().alloc_array(Vec::new())),
    };
    // Declared fields use the exact arguments of the constructed Type. An
    // instantiation descriptor can supply specialized tags for inherited fields.
    let subst = member_subst(ty);
    let subst = subst.as_ref();
    let inst = instantiation_desc(ctx, ty);
    let own_fields = subst.map(|_| {
        td.own_fields().iter().map(|field| field.name.as_ref()).collect::<HashSet<&str>>()
    });
    let field_type = |i: usize, f: &crate::object::FieldSlot| {
        if let Some(subst) = subst {
            if own_fields.as_ref().is_some_and(|names| names.contains(f.name.as_ref())) {
                if let Some(projected) = projected_member_type(ctx, &f.type_tag, subst, &[]) {
                    return projected;
                }
            }
        }
        if let Some(g) = inst.as_ref().and_then(|d| d.fields.get(i)).filter(|g| g.name == f.name) {
            return make_type_from_name(ctx, &g.type_tag);
        }
        match subst {
            Some(subst) => inherited_member_type(ctx, &f.type_tag, subst, &[]),
            None => make_type_from_name(ctx, &f.type_tag),
        }
    };
    let mut out = Vec::with_capacity(td.fields.len() + td.static_fields().len());
    // Instance fields (already base-first — cross-zpkg fixup merges inherited
    // instance fields into `td.fields`; IsStatic = false). Auto-property backing
    // fields (compiler-synthesized `__prop_<Name>`, see IrGen auto-prop stubs) are
    // hidden from GetFields, mirroring C# which never surfaces the backing field —
    // the property is visible via GetProperties instead.
    for (i, f) in td.fields.iter().enumerate() {
        if is_autoprop_backing(&f.name) {
            continue;
        }
        out.push(build_field_info(ctx, &td.name, &f.name, field_type(i, f), false, f.visibility).map_err(BuiltinError::Legacy)?);
    }
    // add-reflection-inherited-static-fields: static fields are stored per
    // declaring class (no instance-field-style fixup), so walk the base chain
    // and aggregate each ancestor's static fields (most-derived first), matching
    // C# `GetFields()` which includes inherited public statics. Each FieldInfo's
    // `__qualified` uses the DECLARING class name so attribute resolution targets
    // the right class. Dedup by name (a derived `new`-style shadow wins).
    let mut seen = std::collections::HashSet::new();
    let mut cur = Some(td.clone());
    while let Some(c) = cur {
        for f in c.static_fields() {
            if is_autoprop_backing(&f.name) {
                continue;
            }
            if seen.insert(f.name.clone()) {
                let projected = subst.filter(|_| Arc::ptr_eq(&c, &td))
                    .and_then(|subst| projected_member_type(ctx, &f.type_tag, subst, &[]));
                let ftype = projected.unwrap_or_else(|| make_type_from_name(ctx, &f.type_tag));
                out.push(build_field_info(ctx, &c.name, &f.name, ftype, true, f.visibility).map_err(BuiltinError::Legacy)?);
            }
        }
        cur = c.base_name.as_ref().and_then(|b| {
            ctx.module()
                .and_then(|m| m.type_registry.get(b).cloned())
                .or_else(|| ctx.try_lookup_type(b))
        });
    }
    Ok(ctx.heap().alloc_array(out))
}

/// Build a `FieldInfo`. `__qualified` ("<Class>.<Field>") lets
/// `FieldInfo.GetCustomAttributes()` resolve the field's attribute factories
/// (add-field-attribute-reflection).
pub(super) fn build_field_info(
    ctx: &VmContext,
    class: &str,
    field: &str,
    ftype: Value,
    is_static: bool,
    visibility: u8,
) -> Result<Value> {
    alloc_named(
        ctx,
        STD_REFLECTION_FIELDINFO,
        &[
            ("Name", Value::Str(field.to_string().into())),
            ("FieldType", ftype),
            ("IsStatic", Value::Bool(is_static)),
            // add-member-visibility (unify P1-b): 0=public / 1=private /
            // 2=protected. `protected` reports neither (mirrors C# IsFamily).
            ("IsPublic", Value::Bool(visibility == 0)),
            ("IsPrivate", Value::Bool(visibility == 1)),
            ("__qualified", Value::Str(format!("{class}.{field}").into())),
        ],
    )
}

/// 成员列举迁 builtin ABI v2(P1-8 批次 5):纯合成(FieldInfo / MethodInfo /
/// ConstructorInfo / PropertyInfo 数组),不执行 z42 代码,不压轻帧。
#[z42_macros::builtins]
pub mod typed {
    use super::*;

    #[builtin(name = "__type_fields")]
    pub fn fields_of(ctx: &VmContext, ty: &Value) -> Result<Value, BuiltinError> {
        type_fields(ctx, ty)
    }
}
