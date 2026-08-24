use super::*;

/// `__type_full_name(typeObj) -> string` — fully-qualified name (`Type.FullName`).
pub fn builtin_type_full_name(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    Ok(read_type_str_slot(args, "__fullName"))
}

/// `__type_element(typeObj) -> Type | null` — the element type of an array Type
/// (`Type.GetElementType()`), or null for a non-array Type. add-reflection-array-
/// element-type: reads the VM-written `__elementName` slot and resolves it.
pub fn builtin_type_element(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    match read_type_str_slot(args, "__elementName") {
        Value::Str(s) if !s.is_empty() => Ok(make_type_from_name(ctx, &s)),
        _ => Ok(Value::Null),
    }
}
// ── Base type & generic arguments ───────────────────────────────────────────

/// `__type_base(typeObj) -> Type | null` — base class Type; `Std.Object` for
/// classes with no explicit base; `null` for `Std.Object` itself / no handle.
pub fn builtin_type_base(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let td = match type_handle(args) {
        Some(t) => t,
        None => return Ok(Value::Null),
    };
    match &td.base_name {
        Some(b) => Ok(make_type_from_name(ctx, b)),
        None => {
            if td.name == STD_OBJECT {
                Ok(Value::Null)
            } else {
                Ok(make_type_from_name(ctx, STD_OBJECT))
            }
        }
    }
}

/// `__type_generic_args(typeObj) -> Type[]` — instantiated generic type args
/// (`Box<int>` → `[typeof(int)]`); empty for non-generic / open types.
pub fn builtin_type_generic_args(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    // add-reflection-generic-type-definition: a *constructed* type built from
    // `typeof(Box<int>)` carries the resolved arg `Std.Type`s in its `__typeArgs`
    // slot — return them directly. (Fixes `typeof(Box<int>).GetGenericArguments()`
    // which previously returned empty because the typeof resolves to the
    // definition TypeDesc whose `type_args` is empty.)
    let slot = read_type_str_slot(args, "__typeArgs");
    if matches!(slot, Value::Array(_)) {
        return Ok(slot);
    }
    // Fallback: `TypeDesc.type_args` — the `new Box<int>()` instance path
    // (`obj.GetType().GetGenericArguments()`), unchanged.
    let td = match type_handle(args) {
        Some(t) => t,
        None => return Ok(ctx.heap().alloc_array(Vec::new())),
    };
    let out: Vec<Value> = td
        .type_args()
        .iter()
        .map(|tag| make_type_from_name(ctx, tag))
        .collect();
    Ok(ctx.heap().alloc_array(out))
}

/// `__type_is_generic_definition(typeObj) -> bool` — true iff the type is a
/// generic type (has type params) AND is not a constructed instantiation (no
/// attached `__typeArgs`). Mirrors C# `Type.IsGenericTypeDefinition`:
/// `typeof(Box<int>)` → false; its `GetGenericTypeDefinition()` → true.
pub fn builtin_type_is_generic_definition(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let is_generic = type_handle(args)
        .map(|td| !td.type_params().is_empty() || !td.type_args().is_empty())
        .unwrap_or(false);
    let constructed = matches!(read_type_str_slot(args, "__typeArgs"), Value::Array(_));
    Ok(Value::Bool(is_generic && !constructed))
}

/// `__type_generic_definition(typeObj) -> Std.Type` — the open generic definition
/// of a constructed type (`typeof(Box<int>)` → `Box<>`). Mirrors C#
/// `Type.GetGenericTypeDefinition()`. The handle already points at the definition
/// TypeDesc (the compiler emits the definition name), so a fresh handle-backed
/// Type without `__typeArgs` IS the open definition. Throws (catchable
/// `Std.Exception`) for non-generic types, matching C#'s `InvalidOperationException`.
pub fn builtin_type_generic_definition(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    match type_handle(args) {
        Some(td) if !td.type_params().is_empty() => Ok(make_type_object(ctx, td)),
        _ => bail!("GetGenericTypeDefinition: type is not a generic type"),
    }
}
/// `__type_interfaces(typeObj) -> Type[]` — the interfaces this type implements.
/// add-reflection-get-interfaces: interfaces are stored per declaring class (the
/// zbc TYPE section carries each class's directly-declared interface names), so
/// walk the base chain and aggregate each ancestor's interfaces (most-derived
/// first), matching C# `GetInterfaces()` which includes inherited interfaces.
/// Dedup by name (a class re-declaring a base's interface appears once). Each
/// name becomes a name-only `Std.Type` via `make_type_from_name`. Transitive
/// interface implementation (interface-extends-interface) is deferred.
pub fn builtin_type_interfaces(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let td = match type_handle(args) {
        Some(t) => t,
        None => return Ok(ctx.heap().alloc_array(Vec::new())),
    };
    let resolve = |name: &str| {
        ctx.module()
            .and_then(|m| m.type_registry.get(name).cloned())
            .or_else(|| ctx.try_lookup_type(name))
    };
    // Seed the queue with the interfaces declared along the class base chain
    // (most-derived first), then BFS-expand each interface's own base interfaces
    // (add-reflection-transitive-interfaces) so the result is the transitive
    // closure. Dedup by FQ name.
    let mut queue: VecDeque<String> = VecDeque::new();
    let mut cur = Some(td);
    while let Some(c) = cur {
        for iface in c.interfaces() {
            queue.push_back(iface.to_string());
        }
        // add-crosspkg-impl-reflection (unify P1-e): traits added to this class
        // via cross-package `impl Trait for Type` (IMPL section of loaded
        // packages). Same queue → transitive closure + dedup apply uniformly.
        for tr in ctx.impl_traits_for(&c.name) {
            queue.push_back(tr);
        }
        cur = c.base_name.as_ref().and_then(|b| resolve(b));
    }
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    while let Some(name) = queue.pop_front() {
        if !seen.insert(name.clone()) {
            continue;
        }
        out.push(make_type_from_name(ctx, &name));
        if let Some(itd) = resolve(&name) {
            for bi in itd.interfaces() {
                queue.push_back(bi.to_string());
            }
        }
    }
    Ok(ctx.heap().alloc_array(out))
}

/// `__type_members(typeObj) -> MemberInfo[]` — fields, then methods, then
/// properties. Built in Rust to sidestep z42 array covariance (the mixed array
/// holds FieldInfo + MethodInfo + PropertyInfo, all `MemberInfo` subclasses).
/// Mirrors C# `GetMembers()`, which surfaces properties alongside their backing
/// `get_`/`set_` accessor methods (the accessors remain in the methods slice).
pub fn builtin_type_members(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let mut out = Vec::new();
    if let Value::Array(a) = builtin_type_fields(ctx, args)? {
        out.extend(a.borrow().iter_boxed());
    }
    if let Value::Array(m) = builtin_type_methods(ctx, args)? {
        out.extend(m.borrow().iter_boxed());
    }
    if let Value::Array(p) = builtin_type_properties(ctx, args)? {
        out.extend(p.borrow().iter_boxed());
    }
    // add-nested-types: C# GetMembers() surfaces nested types (MemberTypes.NestedType)
    // alongside fields/methods/properties (Type : MemberInfo, so covariance holds).
    if let Value::Array(n) = builtin_type_nested_types(ctx, args)? {
        out.extend(n.borrow().iter_boxed());
    }
    Ok(ctx.heap().alloc_array(out))
}

// ── Nested types (add-nested-types) ─────────────────────────────────────────
// Nested types carry a `+` separator in their FQ name (`Ns.Outer+Inner`, source
// spelling `Outer.Inner`). Relationships are derived purely from the name — no
// zbc/zpkg format field — mirroring the array-`[]` and generic-`<>` conventions.

/// `__type_is_nested(typeObj) -> bool` — true if this type is nested (its FQ
/// name carries a `+` declaring-type separator). Handle-less types → false.
pub fn builtin_type_is_nested(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let nested = type_handle(args)
        .map(|td| td.name.contains('+'))
        .unwrap_or(false);
    Ok(Value::Bool(nested))
}

/// `__type_declaring_type(typeObj) -> Type | null` — for a nested type, the
/// enclosing type (FQ name minus the trailing `+segment`, resolved back to a
/// real handle); null for a top-level (non-nested) or handle-less type.
pub fn builtin_type_declaring_type(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let td = match type_handle(args) {
        Some(t) => t,
        None => return Ok(Value::Null),
    };
    match td.name.rfind('+') {
        Some(i) => Ok(make_type_from_name(ctx, &td.name[..i])),
        None => Ok(Value::Null),
    }
}

/// `__type_nested_types(typeObj) -> Type[]` — the types directly nested in this
/// type: loaded types whose FQ name is `<this>+<simple>` with no further `+`
/// (direct children only, mirroring C# `GetNestedTypes()` which excludes deeper
/// and inherited nesting). Deterministic (sorted). Handle-less → empty.
pub fn builtin_type_nested_types(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let td = match type_handle(args) {
        Some(t) => t,
        None => return Ok(ctx.heap().alloc_array(Vec::new())),
    };
    let prefix = format!("{}+", td.name);
    let mut names: Vec<String> = ctx
        .loaded_type_names()
        .into_iter()
        .filter(|n| n.starts_with(&prefix) && !n[prefix.len()..].contains('+'))
        .collect();
    names.sort();
    names.dedup();
    let out: Vec<Value> = names.iter().map(|n| make_type_from_name(ctx, n)).collect();
    Ok(ctx.heap().alloc_array(out))
}
// ── Type flags (add-reflection-type-flags, zbc 1.12) ────────────────────────

/// `__type_is_abstract(typeObj) -> bool` — true if the class was declared
/// `abstract`. False for handle-less Types (primitives / arrays).
pub fn builtin_type_is_abstract(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(class_flag_set(
        args,
        crate::metadata::bytecode::CLASS_FLAG_ABSTRACT,
    )))
}

/// `__type_is_sealed(typeObj) -> bool` — true if the class was declared
/// `sealed`. False for handle-less Types.
pub fn builtin_type_is_sealed(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(class_flag_set(
        args,
        crate::metadata::bytecode::CLASS_FLAG_SEALED,
    )))
}

/// `__type_is_value_type(typeObj) -> bool` — true for a `struct` (value type).
/// Reads the struct bit captured in the TYPE-section flags byte (no new wire).
/// add-reflection-value-record-flags.
pub fn builtin_type_is_value_type(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(class_flag_set(
        args,
        crate::metadata::bytecode::CLASS_FLAG_STRUCT,
    )))
}

/// `__type_is_record(typeObj) -> bool` — true if the type was declared `record`.
pub fn builtin_type_is_record(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(class_flag_set(
        args,
        crate::metadata::bytecode::CLASS_FLAG_RECORD,
    )))
}

/// `__type_is_interface(typeObj) -> bool` — true if the reflected type is an
/// `interface` (its minimal TYPE entry carries the interface flag). Handle-less
/// Types (primitive / array) → false. add-reflection-interface-class-predicates.
pub fn builtin_type_is_interface(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(class_flag_set(
        args,
        crate::metadata::bytecode::CLASS_FLAG_INTERFACE,
    )))
}

/// `__type_is_enum(typeObj) -> bool` — true if the reflected type is an `enum`.
/// Reads CLASS_FLAG_ENUM from the TYPE-section flags byte. add-enum-type-metadata.
pub fn builtin_type_is_enum(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(class_flag_set(
        args,
        crate::metadata::bytecode::CLASS_FLAG_ENUM,
    )))
}

/// `__type_is_delegate(typeObj) -> bool` — true if the reflected type is a
/// `delegate` (delegate-as-class TYPE entry). Reads CLASS_FLAG_DELEGATE.
/// add-delegate-metadata (unify P1-e).
pub fn builtin_type_is_delegate(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(class_flag_set(
        args,
        crate::metadata::bytecode::CLASS_FLAG_DELEGATE,
    )))
}
/// `__type_is_assignable_from(this, c) -> bool` — true if an instance of `c`
/// can be assigned to a variable of `this` type (mirrors C#
/// `Type.IsAssignableFrom`): `c` is `this`, derives from `this`, or implements
/// interface `this`. Reuses the VM's canonical `is_subclass_or_eq_td` (real
/// TypeDesc FQ-name comparison over `c`'s base chain + interfaces) — no string
/// matching on synthesized Type objects. Handle-less operands (primitive / array
/// synthetic Types) fall back to FullName equality. `null` source → false.
/// add-reflection-assignable-from.
pub fn builtin_type_is_assignable_from(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    // args[0] = this (target type), args[1] = c (source type).
    match args.get(1) {
        Some(v) if !matches!(v, Value::Null) => {}
        _ => return Ok(Value::Bool(false)),
    }
    let this_slot = args;          // .first() == this
    let c_slot = &args[1..];       // .first() == c
    // C# semantics: everything is assignable to `object` — value types box, all
    // reference types / interfaces / arrays derive from it. `typeof(object)` is a
    // handle-less Type named "object" (also accept the FQ "Std.Object"), so the
    // handle/FullName paths below would wrongly report false for value types etc.
    // add-reflection-object-assignable. (`c` is already non-null, checked above.)
    let this_name = match type_handle(this_slot) {
        Some(td) => td.name.to_string(),
        None => match read_type_str_slot(this_slot, "__fullName") {
            Value::Str(s) => s.to_string(),
            _ => String::new(),
        },
    };
    if this_name == "object" || this_name == STD_OBJECT {
        return Ok(Value::Bool(true));
    }
    let result = match (type_handle(this_slot), type_handle(c_slot)) {
        (Some(this_td), Some(c_td)) => match ctx.module() {
            Some(m) => crate::interp::dispatch::is_subclass_or_eq_td(
                ctx, &m.type_registry, &c_td.name, &this_td.name,
            ),
            None => c_td.name == this_td.name,
        },
        // Handle-less (primitive / array synthetic): no base chain — same
        // identity only, compared by the __fullName slot.
        _ => {
            let a = read_type_str_slot(this_slot, "__fullName");
            let b = read_type_str_slot(c_slot, "__fullName");
            matches!((&a, &b), (Value::Str(x), Value::Str(y)) if x == y)
        }
    };
    Ok(Value::Bool(result))
}

/// `__type_is_class(typeObj) -> bool` — true for a reference class type
/// (incl. `record`). Mirrors C# `Type.IsClass`: a type with a real handle that
/// is neither a value type (`struct`) nor an interface. Handle-less Types
/// (primitive / array / enum) → false (z42 arrays are name-only synthetic, so
/// — unlike C# — `typeof(int[]).IsClass == false`; see reflection.md Deferred).
pub fn builtin_type_is_class(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    use crate::metadata::bytecode::{CLASS_FLAG_INTERFACE, CLASS_FLAG_STRUCT};
    let v = type_handle(args)
        .map(|td| td.class_flags & CLASS_FLAG_STRUCT == 0
                  && td.class_flags & CLASS_FLAG_INTERFACE == 0)
        .unwrap_or(false);
    Ok(Value::Bool(v))
}

/// add-reflection-generic-predicates: true if the type name is a primitive
/// (keyword form like `int`/`bool`/`char`, or its BCL `Std.*` struct name).
/// `string` is NOT primitive (matches C# `typeof(string).IsPrimitive == false`).
pub(super) fn is_primitive_type_name(name: &str) -> bool {
    matches!(
        name,
        // source-keyword forms (reflection normalizes i32→int etc.)
        "int" | "long" | "short" | "byte" | "sbyte"
            | "uint" | "ulong" | "ushort"
            | "float" | "double" | "bool" | "char"
            // BCL PascalCase struct names (well_known_names)
            | "Std.Int32" | "Std.Int64" | "Std.Int16" | "Std.SByte" | "Std.Byte"
            | "Std.UInt16" | "Std.UInt32" | "Std.UInt64"
            | "Std.Single" | "Std.Double" | "Std.Boolean" | "Std.Char"
    )
}

/// `__type_is_generic(typeObj) -> bool` — true if the type has type parameters
/// (`Box<T>`). Mirrors C# `Type.IsGenericType`. Derived from already-loaded
/// metadata; no wire change. NB: z42 `typeof(Box<int>)` currently resolves to
/// the definition `TypeDesc` (the compiler drops instantiation args), so the
/// open-definition-vs-instantiation distinction (`IsGenericTypeDefinition`) is
/// not yet expressible and is deferred — see reflection.md.
pub fn builtin_type_is_generic(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let v = type_handle(args)
        .map(|td| !td.type_params().is_empty() || !td.type_args().is_empty())
        .unwrap_or(false);
    Ok(Value::Bool(v))
}

/// `__type_is_primitive(typeObj) -> bool` — true if the reflected type is a
/// primitive (see `is_primitive_type_name`). Primitive Types are name-only (no
/// `TypeDesc` handle), so read the `Name` / `__fullName` slots written by
/// `build_type` rather than going through a handle.
pub fn builtin_type_is_primitive(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let is_prim = matches!(read_type_str_slot(args, "Name"),
                           Value::Str(ref s) if is_primitive_type_name(s))
        || matches!(read_type_str_slot(args, "__fullName"),
                    Value::Str(ref s) if is_primitive_type_name(s));
    Ok(Value::Bool(is_prim))
}

/// Is the given class-flag bit set on the reflected Type's `TypeDesc`?
/// Handle-less Types (primitive / array, `NativeData::None`) → false (lenient).
pub(super) fn class_flag_set(args: &[Value], bit: u8) -> bool {
    type_handle(args)
        .map(|td| td.class_flags & bit != 0)
        .unwrap_or(false)
}

// ── complete-class-access-control: class visibility reflection (Type.Visibility) ──
// The zbc 1.33 visibility byte (0=public/1=private/2=protected/3=internal), stored
// on `TypeDesc::visibility`. A single `__type_visibility` builtin surfaces it as an
// int; z42 `Type.Visibility` wraps it in the `TypeVisibility` enum, and callers pair
// it with `Type.IsNested` (the orthogonal axis) to reconstruct C#'s top-level vs
// nested distinction. Handle-less Types (primitive / array) → Public (0): primitives
// behave as public top-level types, mirroring C#.

/// `__type_visibility(typeObj) -> int` — the reflected Type's declaration
/// visibility byte (0=public/1=private/2=protected/3=internal). Handle-less Types
/// (primitive / array) → 0 (Public).
pub fn builtin_type_visibility(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let vis = type_handle(args).map(|td| td.visibility).unwrap_or(0);
    Ok(Value::I64(vis as i64))
}
