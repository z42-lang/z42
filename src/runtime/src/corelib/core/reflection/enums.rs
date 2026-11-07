use super::*;

/// `__enum_names(typeObj) -> string[]` — enum member names in declaration order.
/// Empty for non-enum / handle-less Types. add-enum-type-metadata.
pub fn enum_names(ctx: &VmContext, ty: &Value) -> Value {
    match type_handle(std::slice::from_ref(ty)) {
        Some(td) => {
            let out: Vec<Value> = td.enum_members().iter().map(|(n, _)| Value::Str(n.clone().into())).collect();
            ctx.heap().alloc_array(out)
        }
        None => ctx.heap().alloc_array(Vec::new()),
    }
}

/// `__enum_values(typeObj) -> int[]` — enum member i64 values in declaration order.
/// Empty for non-enum / handle-less Types. add-enum-type-metadata.
pub fn enum_values(ctx: &VmContext, ty: &Value) -> Value {
    match type_handle(std::slice::from_ref(ty)) {
        Some(td) => {
            let out: Vec<Value> = td.enum_members().iter().map(|(_, v)| Value::I64(*v)).collect();
            ctx.heap().alloc_array(out)
        }
        None => ctx.heap().alloc_array(Vec::new()),
    }
}

/// `__enum_name(typeObj, i64) -> string` — member name for a value, or "" if none.
/// add-enum-type-metadata.
pub fn enum_name(ty: &Value, val: i64) -> Value {
    let Some(td) = type_handle(std::slice::from_ref(ty)) else {
        return Value::Str(String::new().into());
    };
    for (n, v) in td.enum_members() {
        if *v == val {
            return Value::Str(n.clone().into());
        }
    }
    Value::Str(String::new().into())
}

/// `__enum_parse(typeObj, name) -> i64` — member value for a name (inverse of
/// `__enum_name`). Throws `ArgumentException` if the receiver is not a type handle or
/// the name is not a member (mirrors C# `Enum.Parse` → ArgumentException). **Case-sensitive**
/// (z42 is a case-sensitive language — no `ignoreCase` variant). Reads the
/// existing `enum_members` metadata; no format change. add-enum-parse-isdefined.
pub fn enum_parse(ty: &Value, name: &crate::object::string::vstr::Str) -> Result<i64, BuiltinError> {
    let td = type_handle(std::slice::from_ref(ty))
        .ok_or_else(|| BuiltinError::Argument(format!("Enum.Parse: receiver is not a type handle")))?;
    for (n, v) in td.enum_members() {
        if n.as_str() == &**name {
            return Ok(*v);
        }
    }
    Err(BuiltinError::Argument(format!(
        "Enum.Parse: `{}` is not a member of enum `{}`", &**name, td.name)))
}

/// `__enum_is_defined(typeObj, i64) -> bool` — true iff the value is a defined
/// enum member. Non-enum / handle-less / non-int arg → false. Reads
/// `enum_members`; no format change. add-enum-parse-isdefined.
pub fn enum_is_defined(ty: &Value, val: i64) -> bool {
    match type_handle(std::slice::from_ref(ty)) {
        Some(td) => td.enum_members().iter().any(|(_, v)| *v == val),
        None => false,
    }
}

/// `__type_enum_underlying(typeObj) -> Type` — the underlying integer type of an
/// enum. z42 backs every enum with **i64** (`long`): IrGen emits enum member
/// values as i64 and discards any declared `: byte` base, and `Enum.GetValues`
/// returns `long[]` — so the honest underlying type is uniformly `long`. Throws
/// `ArgumentException` for a non-enum type (mirrors C#
/// `GetEnumUnderlyingType` → ArgumentException). add-enum-underlying-type.
/// (Accurately reflecting a *declared* `: byte` would need persisting it in the
/// TYPE enum block — a format bump — and z42 honoring it; deferred.)
pub fn type_enum_underlying(ctx: &VmContext, ty: &Value) -> Result<Value, BuiltinError> {
    let is_enum = type_handle(std::slice::from_ref(ty))
        .map(|td| td.class_flags & crate::metadata::ir::CLASS_FLAG_ENUM != 0)
        .unwrap_or(false);
    if !is_enum {
        return Err(BuiltinError::Argument("GetEnumUnderlyingType: type is not an enum".to_string()));
    }
    Ok(make_type_from_name(ctx, "long"))
}

/// 反射枚举查询迁 builtin ABI v2(P1-8 批次 1):纯查询、不执行 z42 代码——不压轻帧,
/// 也不标 `reenters`。
#[z42_macros::builtins]
pub mod typed {
    use super::*;

    #[builtin(name = "__enum_names")]
    pub fn names(ctx: &VmContext, ty: &Value) -> Value { enum_names(ctx, ty) }

    #[builtin(name = "__enum_values")]
    pub fn values(ctx: &VmContext, ty: &Value) -> Value { enum_values(ctx, ty) }

    #[builtin(name = "__enum_name")]
    pub fn name(ty: &Value, val: i64) -> Value { enum_name(ty, val) }

    #[builtin(name = "__enum_parse")]
    pub fn parse(ty: &Value, name: &crate::object::string::vstr::Str) -> Result<i64, BuiltinError> { enum_parse(ty, name) }

    #[builtin(name = "__enum_is_defined")]
    pub fn is_defined(ty: &Value, val: i64) -> bool { enum_is_defined(ty, val) }

    #[builtin(name = "__type_enum_underlying")]
    pub fn underlying(ctx: &VmContext, ty: &Value) -> Result<Value, BuiltinError> {
        type_enum_underlying(ctx, ty)
    }
}
