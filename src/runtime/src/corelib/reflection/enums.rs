use super::*;

/// `__enum_names(typeObj) -> string[]` — enum member names in declaration order.
/// Empty for non-enum / handle-less Types. add-enum-type-metadata.
pub fn builtin_enum_names(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let td = match type_handle(args) {
        Some(t) => t,
        None => return Ok(ctx.heap().alloc_array(Vec::new())),
    };
    let out: Vec<Value> = td
        .enum_members()
        .iter()
        .map(|(n, _)| Value::Str(n.clone().into()))
        .collect();
    Ok(ctx.heap().alloc_array(out))
}

/// `__enum_values(typeObj) -> int[]` — enum member i64 values in declaration order.
/// Empty for non-enum / handle-less Types. add-enum-type-metadata.
pub fn builtin_enum_values(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let td = match type_handle(args) {
        Some(t) => t,
        None => return Ok(ctx.heap().alloc_array(Vec::new())),
    };
    let out: Vec<Value> = td
        .enum_members()
        .iter()
        .map(|(_, v)| Value::I64(*v))
        .collect();
    Ok(ctx.heap().alloc_array(out))
}

/// `__enum_name(typeObj, i64) -> string` — member name for a value, or "" if none.
/// add-enum-type-metadata.
pub fn builtin_enum_name(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let td = match type_handle(args) {
        Some(t) => t,
        None => return Ok(Value::Str(String::new().into())),
    };
    let val = match args.get(1) {
        Some(Value::I64(v)) => *v,
        _ => return Ok(Value::Str(String::new().into())),
    };
    for (n, v) in td.enum_members() {
        if *v == val {
            return Ok(Value::Str(n.clone().into()));
        }
    }
    Ok(Value::Str(String::new().into()))
}

/// `__enum_parse(typeObj, name) -> i64` — member value for a name (inverse of
/// `__enum_name`). Throws a catchable `Std.Exception` if the name is not a
/// member (mirrors C# `Enum.Parse` → ArgumentException). **Case-sensitive**
/// (z42 is a case-sensitive language — no `ignoreCase` variant). Reads the
/// existing `enum_members` metadata; no format change. add-enum-parse-isdefined.
pub fn builtin_enum_parse(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let td = match type_handle(args) {
        Some(t) => t,
        None => bail!("Enum.Parse: receiver is not a type handle"),
    };
    let name = match args.get(1) {
        Some(Value::Str(s)) => s.clone(),
        _ => bail!("Enum.Parse: expected a string member name"),
    };
    for (n, v) in td.enum_members() {
        if n.as_str() == name.as_ref() {
            return Ok(Value::I64(*v));
        }
    }
    bail!("Enum.Parse: `{}` is not a member of enum `{}`", name, td.name)
}

/// `__enum_is_defined(typeObj, i64) -> bool` — true iff the value is a defined
/// enum member. Non-enum / handle-less / non-int arg → false. Reads
/// `enum_members`; no format change. add-enum-parse-isdefined.
pub fn builtin_enum_is_defined(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let td = match type_handle(args) {
        Some(t) => t,
        None => return Ok(Value::Bool(false)),
    };
    let val = match args.get(1) {
        Some(Value::I64(v)) => *v,
        _ => return Ok(Value::Bool(false)),
    };
    Ok(Value::Bool(td.enum_members().iter().any(|(_, v)| *v == val)))
}

/// `__type_enum_underlying(typeObj) -> Type` — the underlying integer type of an
/// enum. z42 backs every enum with **i64** (`long`): IrGen emits enum member
/// values as i64 and discards any declared `: byte` base, and `Enum.GetValues`
/// returns `long[]` — so the honest underlying type is uniformly `long`. Throws a
/// catchable `Std.Exception` for a non-enum type (mirrors C#
/// `GetEnumUnderlyingType` → ArgumentException). add-enum-underlying-type.
/// (Accurately reflecting a *declared* `: byte` would need persisting it in the
/// TYPE enum block — a format bump — and z42 honoring it; deferred.)
pub fn builtin_type_enum_underlying(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let is_enum = type_handle(args)
        .map(|td| td.class_flags & crate::metadata::bytecode::CLASS_FLAG_ENUM != 0)
        .unwrap_or(false);
    if !is_enum {
        bail!("GetEnumUnderlyingType: type is not an enum");
    }
    Ok(make_type_from_name(ctx, "long"))
}
