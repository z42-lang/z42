use super::*;

/// `__type_full_name(typeObj) -> string` — fully-qualified name (`Type.FullName`).
pub fn type_full_name(ty: &Value) -> Value {
    read_type_slot_of(ty, "__fullName")
}

/// `__type_element(typeObj) -> Type | null` — the element type of an array Type
/// (`Type.GetElementType()`), or null for a non-array Type. add-reflection-array-
/// element-type: reads the VM-written `__elementName` slot and resolves it.
pub fn type_element(ctx: &VmContext, ty: &Value) -> Value {
    match read_type_slot_of(ty, "__elementName") {
        Value::Str(s) if !s.is_empty() => make_type_from_name(ctx, &s),
        _ => Value::Null,
    }
}

// ── Base type & generic arguments ───────────────────────────────────────────

/// `__type_base(typeObj) -> Type | null` — base class Type; `Std.Object` for
/// classes with no explicit base; `null` for `Std.Object` itself / no handle.
pub fn type_base(ctx: &VmContext, ty: &Value) -> Value {
    // A constructed Type's handle is its definition, whose base names no arguments; the
    // instantiation descriptor carries the substituted one (`Sub<int>` → `Box<int>`).
    let td = match instantiation_desc(ctx, ty).or_else(|| type_handle_of(ty)) {
        Some(t) => t,
        None => return Value::Null,
    };
    match &td.base_name {
        Some(b) => make_type_from_name(ctx, b),
        None => {
            if td.name == STD_OBJECT {
                Value::Null
            } else {
                make_type_from_name(ctx, STD_OBJECT)
            }
        }
    }
}

/// `__type_generic_args(typeObj) -> Type[]` — instantiated generic type args
/// (`Box<int>` → `[typeof(int)]`); empty for non-generic / open types.
pub fn type_generic_args(ctx: &VmContext, ty: &Value) -> Value {
    // add-reflection-generic-type-definition: a *constructed* type built from
    // `typeof(Box<int>)` carries the resolved arg `Std.Type`s in its `__typeArgs`
    // slot — return them directly. (Fixes `typeof(Box<int>).GetGenericArguments()`
    // which previously returned empty because the typeof resolves to the
    // definition TypeDesc whose `type_args` is empty.)
    let slot = read_type_slot_of(ty, "__typeArgs");
    if matches!(slot, Value::Array(_)) {
        return slot;
    }
    // Fallback: `TypeDesc.type_args` — the `new Box<int>()` instance path
    // (`obj.GetType().GetGenericArguments()`), unchanged.
    let td = match type_handle_of(ty) {
        Some(t) => t,
        None => return ctx.heap().alloc_array(Vec::new()),
    };
    let out: Vec<Value> = td
        .type_args()
        .iter()
        .map(|tag| make_type_from_name(ctx, tag))
        .collect();
    ctx.heap().alloc_array(out)
}

/// `__type_is_generic_definition(typeObj) -> bool` — true iff the type is a
/// generic type (has type params) AND is not a constructed instantiation (no
/// attached `__typeArgs`). Mirrors C# `Type.IsGenericTypeDefinition`:
/// `typeof(Box<int>)` → false; its `GetGenericTypeDefinition()` → true.
pub fn type_is_generic_definition(ty: &Value) -> bool {
    let is_generic = type_handle_of(ty)
        .map(|td| !td.type_params().is_empty() || !td.type_args().is_empty())
        .unwrap_or(false);
    let constructed = matches!(read_type_slot_of(ty, "__typeArgs"), Value::Array(_));
    is_generic && !constructed
}

/// `__type_generic_definition(typeObj) -> Std.Type` — the open generic definition
/// of a constructed type (`typeof(Box<int>)` → `Box<>`). Mirrors C#
/// `Type.GetGenericTypeDefinition()`. The handle already points at the definition
/// TypeDesc (the compiler emits the definition name), so a fresh handle-backed
/// Type without `__typeArgs` IS the open definition. Throws
/// `InvalidOperationException` for non-generic types, matching C#.
pub fn type_generic_definition(ctx: &VmContext, ty: &Value) -> Result<Value, BuiltinError> {
    match type_handle_of(ty) {
        Some(td) if !td.type_params().is_empty() => Ok(make_type_object(ctx, td)),
        _ => Err(BuiltinError::InvalidOperation(
            "GetGenericTypeDefinition: type is not a generic type".to_string())),
    }
}

/// `__type_interfaces(typeObj) -> Type[]` — the interfaces this type implements.
/// add-reflection-get-interfaces: interfaces are stored per declaring class (the
/// zbc TYPE section carries each class's directly-declared interface names), so
/// walk the base chain and aggregate each ancestor's interfaces (most-derived
/// first), matching C# `GetInterfaces()` which includes inherited interfaces.
/// Dedup by name (a class re-declaring a base's interface appears once). Each
/// name becomes a name-only `Std.Type` via `make_type_from_name`. Transitive
/// interface implementation via BFS (add-reflection-transitive-interfaces).
pub fn type_interfaces(ctx: &VmContext, ty: &Value) -> Value {
    // Same as `type_base`: the instantiation descriptor lists its interfaces substituted.
    let td = match instantiation_desc(ctx, ty).or_else(|| type_handle_of(ty)) {
        Some(t) => t,
        None => return ctx.heap().alloc_array(Vec::new()),
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
        for tr in ctx.impl_traits_for(crate::metadata::index::type_names::erased(&c.name)) {
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
    ctx.heap().alloc_array(out)
}

/// `__type_members(typeObj) -> MemberInfo[]` — fields, then methods, then
/// properties. Built in Rust to sidestep z42 array covariance (the mixed array
/// holds FieldInfo + MethodInfo + PropertyInfo, all `MemberInfo` subclasses).
/// Mirrors C# `GetMembers()`, which surfaces properties alongside their backing
/// `get_`/`set_` accessor methods (the accessors remain in the methods slice).
pub fn type_members(ctx: &VmContext, ty: &Value) -> Value {
    let mut out = Vec::new();

    for part in [
        super::fields::type_fields as fn(&VmContext, &Value) -> Result<Value, BuiltinError>,
        super::methods::type_methods,
        super::properties::type_properties,
    ] {
        if let Value::Array(a) = part(ctx, ty).unwrap_or(Value::Null) {
            out.extend(a.borrow().iter_boxed());
        }
    }
    // add-nested-types: C# GetMembers() surfaces nested types (MemberTypes.NestedType)
    // alongside fields/methods/properties (Type : MemberInfo, so covariance holds).
    if let Value::Array(n) = type_nested_types(ctx, ty) {
        out.extend(n.borrow().iter_boxed());
    }
    ctx.heap().alloc_array(out)
}

// ── Nested types (add-nested-types) ─────────────────────────────────────────
// Nested types carry a `+` separator in their FQ name (`Ns.Outer+Inner`, source
// spelling `Outer.Inner`). Relationships are derived purely from the name — no
// zbc/zpkg format field — mirroring the array-`[]` and generic-`<>` conventions.

/// `__type_is_nested(typeObj) -> bool` — true if this type is nested (its FQ
/// name carries a `+` declaring-type separator). Handle-less types → false.
pub fn type_is_nested(ty: &Value) -> bool {
    type_handle_of(ty).map(|td| td.name.contains('+')).unwrap_or(false)
}

/// `__type_declaring_type(typeObj) -> Type | null` — for a nested type, the
/// enclosing type (FQ name minus the trailing `+segment`, resolved back to a
/// real handle); null for a top-level (non-nested) or handle-less type.
pub fn type_declaring_type(ctx: &VmContext, ty: &Value) -> Value {
    let td = match type_handle_of(ty) {
        Some(t) => t,
        None => return Value::Null,
    };
    match crate::metadata::index::type_names::last_top(&td.name, '+') {
        Some(i) => make_type_from_name(ctx, &td.name[..i]),
        None => Value::Null,
    }
}

/// `__type_nested_types(typeObj) -> Type[]` — the types directly nested in this
/// type: loaded types whose FQ name is `<this>+<simple>` with no further `+`
/// (direct children only, mirroring C# `GetNestedTypes()` which excludes deeper
/// and inherited nesting). Deterministic (sorted). Handle-less → empty.
pub fn type_nested_types(ctx: &VmContext, ty: &Value) -> Value {
    let td = match type_handle_of(ty) {
        Some(t) => t,
        None => return ctx.heap().alloc_array(Vec::new()),
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
    ctx.heap().alloc_array(out)
}

// ── Type flags (add-reflection-type-flags, zbc 1.12) ────────────────────────

/// `__type_is_abstract(typeObj) -> bool` — true if the class was declared
/// `abstract`. False for handle-less Types (primitives / arrays).
pub fn type_is_abstract(ty: &Value) -> bool {
    class_flag_set(ty, crate::metadata::ir::CLASS_FLAG_ABSTRACT)
}

/// `__type_is_sealed(typeObj) -> bool` — true if the class was declared
/// `sealed`. False for handle-less Types.
pub fn type_is_sealed(ty: &Value) -> bool {
    class_flag_set(ty, crate::metadata::ir::CLASS_FLAG_SEALED)
}

/// `__type_is_value_type(typeObj) -> bool` — true for a `struct` (value type).
/// Reads the struct bit captured in the TYPE-section flags byte (no new wire).
/// add-reflection-value-record-flags.
pub fn type_is_value_type(ty: &Value) -> bool {
    class_flag_set(ty, crate::metadata::ir::CLASS_FLAG_STRUCT)
}

/// `__type_is_record(typeObj) -> bool` — true if the type was declared `record`.
pub fn type_is_record(ty: &Value) -> bool {
    class_flag_set(ty, crate::metadata::ir::CLASS_FLAG_RECORD)
}

/// `__type_is_interface(typeObj) -> bool` — true if the reflected type is an
/// `interface` (its minimal TYPE entry carries the interface flag). Handle-less
/// Types (primitive / array) → false. add-reflection-interface-class-predicates.
pub fn type_is_interface(ty: &Value) -> bool {
    class_flag_set(ty, crate::metadata::ir::CLASS_FLAG_INTERFACE)
}

/// `__type_is_enum(typeObj) -> bool` — true if the reflected type is an `enum`.
/// Reads CLASS_FLAG_ENUM from the TYPE-section flags byte. add-enum-type-metadata.
pub fn type_is_enum(ty: &Value) -> bool {
    class_flag_set(ty, crate::metadata::ir::CLASS_FLAG_ENUM)
}

/// `__type_is_delegate(typeObj) -> bool` — true if the reflected type is a
/// `delegate` (delegate-as-class TYPE entry). Reads CLASS_FLAG_DELEGATE.
/// add-delegate-metadata (unify P1-e).
pub fn type_is_delegate(ty: &Value) -> bool {
    class_flag_set(ty, crate::metadata::ir::CLASS_FLAG_DELEGATE)
}

/// `__type_is_assignable_from(this, c) -> bool` — true if an instance of `c`
/// can be assigned to a variable of `this` type (mirrors C#
/// `Type.IsAssignableFrom`): `c` is `this`, derives from `this`, or implements
/// interface `this`. Reuses the VM's canonical `is_subclass_or_eq_td` (real
/// TypeDesc FQ-name comparison over `c`'s base chain + interfaces) — no string
/// matching on synthesized Type objects. Handle-less operands (primitive / array
/// synthetic Types) fall back to FullName equality. `null` source → false.
/// add-reflection-assignable-from.
pub fn type_is_assignable_from(ctx: &VmContext, this: &Value, c: &Value) -> bool {
    if matches!(c, Value::Null) {
        return false;
    }
    // Both sides by their canonical runtime name (`type_value_name`): a constructed Type's
    // handle is its generic *definition*, so comparing handle names answered
    // `typeof(Box<string>).IsAssignableFrom(typeof(DInt))` (DInt : Box<int>) with true.
    let (Some(this_name), Some(c_name)) = (type_value_name(this), type_value_name(c)) else {
        return false;
    };
    // C# semantics: everything is assignable to `object` — value types box, all reference
    // types / interfaces / arrays derive from it. add-reflection-object-assignable.
    if this_name == STD_OBJECT {
        return true;
    }
    match (type_handle_of(this), type_handle_of(c)) {
        (Some(_), Some(_)) => match ctx.module() {
            Some(m) => crate::interp::dispatch::is_subclass_or_eq_td(
                ctx, &m.type_registry, &c_name, &this_name,
            ),
            None => c_name == this_name,
        },
        // Handle-less (array / unresolved synthetic): no base chain — same identity only.
        _ => c_name == this_name,
    }
}

/// `__type_is_class(typeObj) -> bool` — true for a reference class type
/// (incl. `record`). Mirrors C# `Type.IsClass`: a type with a real handle that
/// is neither a value type (`struct`) nor an interface. Handle-less Types
/// (primitive / array / enum) → false (z42 arrays are name-only synthetic, so
/// — unlike C# — `typeof(int[]).IsClass == false`; see reflection.md Deferred).
pub fn type_is_class(ty: &Value) -> bool {
    use crate::metadata::ir::{CLASS_FLAG_INTERFACE, CLASS_FLAG_STRUCT};
    type_handle_of(ty)
        .map(|td| td.class_flags & CLASS_FLAG_STRUCT == 0
                  && td.class_flags & CLASS_FLAG_INTERFACE == 0)
        .unwrap_or(false)
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
pub fn type_is_generic(ty: &Value) -> bool {
    type_handle_of(ty)
        .map(|td| !td.type_params().is_empty() || !td.type_args().is_empty())
        .unwrap_or(false)
}

/// `__type_is_primitive(typeObj) -> bool` — true if the reflected type is a
/// primitive (see `is_primitive_type_name`). Primitive Types are name-only (no
/// `TypeDesc` handle), so read the `Name` / `__fullName` slots written by
/// `build_type` rather than going through a handle.
pub fn type_is_primitive(ty: &Value) -> bool {
    matches!(read_type_slot_of(ty, "Name"),
             Value::Str(ref s) if is_primitive_type_name(s))
        || matches!(read_type_slot_of(ty, "__fullName"),
                    Value::Str(ref s) if is_primitive_type_name(s))
}

/// Is the given class-flag bit set on the reflected Type's `TypeDesc`?
/// Handle-less Types (primitive / array, `NativeData::None`) → false (lenient).
pub(super) fn class_flag_set(ty: &Value, bit: u8) -> bool {
    type_handle_of(ty).map(|td| td.class_flags & bit != 0).unwrap_or(false)
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
pub fn type_visibility(ty: &Value) -> i64 {
    type_handle_of(ty).map(|td| td.visibility as i64).unwrap_or(0)
}

/// 类型查询迁 builtin ABI v2(P1-8 批次 1):纯查询、不执行 z42 代码——不压轻帧,
/// 也不标 `reenters`。`__type_get_type`(`GetType` 的 receiver 版)仍走 type_object
/// 的旧表(批次 2)。
#[z42_macros::builtins]
pub mod typed {
    use super::*;

    #[builtin(name = "__type_full_name")]
    pub fn full_name(ty: &Value) -> Value { type_full_name(ty) }

    #[builtin(name = "__type_element")]
    pub fn element(ctx: &VmContext, ty: &Value) -> Value { type_element(ctx, ty) }

    #[builtin(name = "__type_base")]
    pub fn base(ctx: &VmContext, ty: &Value) -> Value { type_base(ctx, ty) }

    #[builtin(name = "__type_generic_args")]
    pub fn generic_args(ctx: &VmContext, ty: &Value) -> Value { type_generic_args(ctx, ty) }

    #[builtin(name = "__type_is_generic_definition")]
    pub fn is_generic_definition(ty: &Value) -> bool { type_is_generic_definition(ty) }

    #[builtin(name = "__type_generic_definition")]
    pub fn generic_definition(ctx: &VmContext, ty: &Value) -> Result<Value, BuiltinError> {
        type_generic_definition(ctx, ty)
    }

    #[builtin(name = "__type_interfaces")]
    pub fn interfaces(ctx: &VmContext, ty: &Value) -> Value { type_interfaces(ctx, ty) }

    #[builtin(name = "__type_members")]
    pub fn members(ctx: &VmContext, ty: &Value) -> Value { type_members(ctx, ty) }

    #[builtin(name = "__type_is_nested")]
    pub fn is_nested(ty: &Value) -> bool { type_is_nested(ty) }

    #[builtin(name = "__type_declaring_type")]
    pub fn declaring_type(ctx: &VmContext, ty: &Value) -> Value { type_declaring_type(ctx, ty) }

    #[builtin(name = "__type_nested_types")]
    pub fn nested_types(ctx: &VmContext, ty: &Value) -> Value { type_nested_types(ctx, ty) }

    #[builtin(name = "__type_is_abstract")]
    pub fn is_abstract(ty: &Value) -> bool { type_is_abstract(ty) }

    #[builtin(name = "__type_is_sealed")]
    pub fn is_sealed(ty: &Value) -> bool { type_is_sealed(ty) }

    #[builtin(name = "__type_is_value_type")]
    pub fn is_value_type(ty: &Value) -> bool { type_is_value_type(ty) }

    #[builtin(name = "__type_is_record")]
    pub fn is_record(ty: &Value) -> bool { type_is_record(ty) }

    #[builtin(name = "__type_is_interface")]
    pub fn is_interface(ty: &Value) -> bool { type_is_interface(ty) }

    #[builtin(name = "__type_is_enum")]
    pub fn is_enum(ty: &Value) -> bool { type_is_enum(ty) }

    #[builtin(name = "__type_is_delegate")]
    pub fn is_delegate(ty: &Value) -> bool { type_is_delegate(ty) }

    #[builtin(name = "__type_is_assignable_from")]
    pub fn is_assignable_from(ctx: &VmContext, this: &Value, c: &Value) -> bool {
        type_is_assignable_from(ctx, this, c)
    }

    #[builtin(name = "__type_is_class")]
    pub fn is_class(ty: &Value) -> bool { type_is_class(ty) }

    #[builtin(name = "__type_is_generic")]
    pub fn is_generic(ty: &Value) -> bool { type_is_generic(ty) }

    #[builtin(name = "__type_is_primitive")]
    pub fn is_primitive(ty: &Value) -> bool { type_is_primitive(ty) }

    #[builtin(name = "__type_visibility")]
    pub fn visibility(ty: &Value) -> i64 { type_visibility(ty) }
}
