use super::*;

/// `__repl_member_names(staticFieldFqn: string) -> string[]` — D2 live-reflection
/// member completion (add-completion-query-api 阶段 3b). Reads a REPL session
/// variable's **live value** from its static-field slot (key
/// `Repl.R{N}.Vars{N}.{var}`, see loader.rs static-field FQN convention), then
/// returns the names of its runtime type's members (fields/methods/properties/
/// nested — same set as `Type.GetMembers()`). Reading a stored static field is
/// side-effect-free (no expression re-evaluation), which is why session-variable
/// `obj.` completion is safe here (D2). Null / primitive / unloaded → empty.
pub fn member_names_of_static_value(ctx: &VmContext, fqn: &str) -> anyhow::Result<Value> {
    let v = ctx.static_get(fqn);
    let tname = match &v {
        Value::Object(rc) => rc.type_desc().name.clone(),
        // add-repl-completion-iter2: primitive-typed session vars → the **same** canonical
        // stdlib class `GetType()` resolves (Std.String / Std.Int32 / boxed's exact class),
        // so member reflection returns the real member set (string.Length/CharAt,
        // int.ToString, …). Mirrors `object::builtin_obj_get_type` so the names actually
        // resolve in `make_type_from_name` (hardcoding "string"/"int" would not).
        Value::Str(_) => crate::zpkg::index::well_known_names::STD_STRING.to_string(),
        // unify Phase 2 R3: 装箱值类型（struct 或基元）→ 精确类型名（type_desc.name）。
        Value::BoxedStruct(b) => b.type_desc().name.to_string(),
        v @ (Value::I64(_) | Value::F64(_) | Value::Bool(_) | Value::Char(_)) => {
            match crate::metadata::dispatch::receiver::primitive_class_name(v) {
                Some(cn) => cn.to_string(),
                None => return Ok(ctx.heap().alloc_array(Vec::new())),
            }
        }
        // Null / arrays / other → no member completion.
        _ => return Ok(ctx.heap().alloc_array(Vec::new())),
    };
    let type_val = make_type_from_name(ctx, &tname);
    let members = type_members(ctx, &type_val);
    let mut names: Vec<Value> = Vec::new();
    if let Value::Array(a) = members {
        for m in a.borrow().iter_boxed() {
            if let Some(n) = member_name_of(&m) {
                names.push(Value::Str(n.into()));
            }
        }
    }
    Ok(ctx.heap().alloc_array(names))
}

/// Read the `Name` slot of a reflection `MemberInfo` (or subclass) object.
fn member_name_of(v: &Value) -> Option<String> {
    if let Value::Object(rc) = v {
        let idx = rc.type_desc().field_index.get("Name").copied();
        if let Some(i) = idx {
            if let Value::Str(s) = rc.borrow().field_value(i) {
                return Some(s.to_string());
            }
        }
    }
    None
}

#[z42_macros::builtins]
pub mod typed {
    use crate::{context::VmContext, exec::call::builtin_abi::BuiltinError, object::Value};
    // The script declaration remains a core ModuleLoader operation.
    #[builtin(name = "__repl_member_names", reenters)]
    pub fn member_names(ctx: &VmContext, fqn: &str) -> Result<Value, BuiltinError> {
        super::reentry(ctx, super::member_names_of_static_value(ctx, fqn))
    }
}
