//! Reflection builtins — read-only type introspection backing `Std.Type` and
//! `Std.Reflection.{FieldInfo,MethodInfo,ParameterInfo}` (add-reflection-mvp,
//! 2026-06-08).
//!
//! Design (see docs/spec/.../add-reflection-mvp/design.md):
//!   - `Std.Type` objects carry the real `Arc<TypeDesc>` in
//!     `NativeData::TypeHandle` (set by `__obj_get_type`). Reflection builtins
//!     read it to enumerate members.
//!   - Member/Type objects are populated EAGERLY: each builtin allocates the
//!     real z42 class (`Std.Reflection.FieldInfo`, …) via `try_lookup_type` and
//!     fills slots by name through `field_index`.
//!   - All builtins take the reflected object as `args[0]` and are LENIENT:
//!     a synthetic Type (primitive/array, no handle) yields empty arrays / null,
//!     never `bail!` (mirrors C# returning empty results).
//!   - Method signatures (params/return/static) are read on demand from the
//!     method's `Function` via `ctx.try_lookup_function` — no persisted
//!     per-type method table, no wire-format change.

use crate::interp::{exec_function, ExecOutcome};
use crate::metadata::{well_known_names, NativeData, TypeDesc, Value};
use crate::vm_context::VmContext;
use anyhow::{bail, Result};
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

const STD_OBJECT: &str = "Std.Object";
const STD_REFLECTION_FIELDINFO: &str = "Std.Reflection.FieldInfo";
const STD_REFLECTION_METHODINFO: &str = "Std.Reflection.MethodInfo";
const STD_REFLECTION_PARAMINFO: &str = "Std.Reflection.ParameterInfo";
const STD_REFLECTION_PROPERTYINFO: &str = "Std.Reflection.PropertyInfo";

// ── Type-object construction ────────────────────────────────────────────────

/// Build a `Std.Type` object backed by the real `Std.Type` class (so its
/// reflection methods dispatch via the class vtable) and carrying `td` as
/// `NativeData::TypeHandle`. Falls back to a handle-less synthetic only if
/// z42.core's `Std.Type` isn't loaded (shouldn't happen in practice).
pub fn make_type_object(ctx: &VmContext, td: Arc<TypeDesc>) -> Value {
    let full = td.name.clone();
    let simple = full.rsplit('.').next().unwrap_or(&full).to_string();
    build_type(ctx, &simple, &full, NativeData::TypeHandle(td))
}

/// Split a generic arg list on top-level commas, respecting nested `<>` / `[]`
/// so `Box<int>,string` → `["Box<int>", "string"]` (not split inside the inner
/// `<>`). add-reflection-nested-generic-args.
fn split_generic_args(inner: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0usize;
    for (i, c) in inner.char_indices() {
        match c {
            '<' | '[' => depth += 1,
            '>' | ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(inner[start..i].to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(inner[start..].to_string());
    out
}

/// Build a `Std.Type` from a type name / type-tag string. Resolves to a real
/// handle when the name is a loaded class; otherwise yields a handle-less Type
/// (primitives like `"int"`, arrays, unresolved). Used for
/// `FieldType` / `ReturnType` / `ParameterType` and `GetType` on
/// primitives/arrays.
pub fn make_type_from_name(ctx: &VmContext, name: &str) -> Value {
    // add-reflection-array-element-type: an array type name carries a `[]` suffix
    // (`typeof(int[])` emits "int[]"; array field/param type tags are "int[]").
    // Build a synthetic array `Type` (Name "Array", FullName "Std.Array" —
    // consistent with `arr.GetType()`) carrying the element type. `int[][]`
    // strips one level → element "int[]" (recursively resolvable).
    if let Some(elem) = name.strip_suffix("[]") {
        return build_type_ex(
            ctx, "Array", well_known_names::STD_ARRAY, NativeData::None, true, elem,
        );
    }
    // add-reflection-nested-generic-args: a constructed-generic arg name carries angle
    // brackets (`Box<Pair<int,string>>` — z42c `_typeofArgName` emits the full nested
    // name). Parse base + top-level args (bracket-depth aware) and build a constructed
    // Type via `make_constructed_type`, whose per-arg resolution re-enters here → nested
    // generics resolve to arbitrary depth (each arg keeps its own `__typeArgs`).
    if let Some(lt) = name.find('<') {
        if name.ends_with('>') {
            let base = &name[..lt];
            let inner = &name[lt + 1..name.len() - 1];
            let args = split_generic_args(inner);
            return make_constructed_type(ctx, base, &args);
        }
    }
    // Main module's own types first: the user program's classes live in the
    // main module's `type_registry`; the lazy loader below only covers
    // zpkg / stdlib types. (make-typeof-return-type — lets `typeof(UserClass)`
    // resolve to a real handle.)
    if let Some(m) = ctx.module() {
        if let Some(td) = m.type_registry.get(name) {
            return make_type_object(ctx, td.clone());
        }
    }
    if let Some(td) = ctx.try_lookup_type(name) {
        return make_type_object(ctx, td);
    }
    // Primitive / unresolved: present a canonical user-facing name. The VM uses
    // two tag vocabularies — field slots carry `"int"`/`"long"`, function
    // signatures carry `"i32"`/`"i64"`/`"str"` — so reflection normalizes both
    // to the C#-style aliases for a consistent surface.
    let canon = canonical_type_name(name);
    let simple = canon.rsplit('.').next().unwrap_or(&canon).to_string();
    build_type(ctx, &simple, &canon, NativeData::None)
}

/// Backs the `Typeof` opcode (add-reflection-generic-type-definition; replaces
/// the former `__typeof` builtin). `type_name` is the reflected type's FQ name
/// (definition name for a generic); resolved to a `Std.Type` (real handle when
/// loaded — user classes via the main module, stdlib via the lazy loader — else
/// a name-only synthetic for primitives / arrays / unbound generics).
///
/// `type_args` are the FQ names of the instantiation arguments (`typeof(Box<int>)`
/// → `["int"]`). When non-empty this is a *constructed* generic type: the
/// resolved arg `Std.Type`s are attached to the `__typeArgs` slot so
/// `GetGenericArguments()` returns them and `IsGenericTypeDefinition` is false.
/// Empty `type_args` → the plain definition / non-generic Type.
pub fn make_constructed_type(ctx: &VmContext, type_name: &str, type_args: &[String]) -> Value {
    if type_args.is_empty() {
        return make_type_from_name(ctx, type_name);
    }
    // Resolve args first and keep them rooted in the Vec while `base` allocates
    // (same alloc ordering as `builtin_type_generic_args`).
    let arg_types: Vec<Value> = type_args.iter().map(|a| make_type_from_name(ctx, a)).collect();
    let args_array = ctx.heap().alloc_array(arg_types);
    let base = make_type_from_name(ctx, type_name);
    if let Value::Object(rc) = &base {
        if let Some(i) = rc.type_desc().field_index.get("__typeArgs").copied() {
            rc.borrow_mut().slots[i] = args_array;
        }
    }
    base
}

/// Normalize a VM primitive type tag to its C#-style alias. User/class names
/// (anything not a known primitive tag) pass through unchanged.
fn canonical_type_name(tag: &str) -> String {
    match tag {
        "i8" => "sbyte",
        "u8" => "byte",
        "i16" => "short",
        "u16" => "ushort",
        "i32" => "int",
        "u32" => "uint",
        "i64" => "long",
        "u64" => "ulong",
        "f32" => "float",
        "f64" => "double",
        "str" => "string",
        other => other,
    }
    .to_string()
}

/// Allocate a `Std.Type` ScriptObject, writing `__name` / `__fullName` slots by
/// `field_index` and attaching `native`. Uses the real `Std.Type` TypeDesc so
/// the object responds to reflection methods.
fn build_type(ctx: &VmContext, simple: &str, full: &str, native: NativeData) -> Value {
    build_type_ex(ctx, simple, full, native, false, "")
}

/// add-reflection-array-element-type: like `build_type`, but also records whether
/// this is an array type and (if so) its element type FQ name, written to the
/// `Std.Type` `IsArray` / `__elementName` slots (VM-written, same mechanism as
/// `__name` / `__fullName`). `GetElementType()` reads `__elementName` lazily.
fn build_type_ex(
    ctx: &VmContext, simple: &str, full: &str, native: NativeData,
    is_array: bool, element: &str,
) -> Value {
    match ctx.try_lookup_type(well_known_names::STD_TYPE) {
        Some(type_td) => {
            let mut slots = vec![Value::Null; type_td.fields.len()];
            if let Some(&i) = type_td.field_index.get("IsArray") {
                slots[i] = Value::Bool(is_array);
            }
            if let Some(&i) = type_td.field_index.get("__elementName") {
                slots[i] = Value::Str(element.to_string().into());
            }
            // align-type-memberinfo-hierarchy: `Name` is inherited from
            // `Std.Reflection.MemberInfo` (Type's base) — populate that slot so
            // `typeof(C).Name` / `(MemberInfo)typeof(C)).Name` resolve via the
            // shared base field (same as FieldInfo/MethodInfo). `__name` retained
            // for low-level golden / z42.test direct reads.
            if let Some(&i) = type_td.field_index.get("Name") {
                slots[i] = Value::Str(simple.to_string().into());
            }
            if let Some(&i) = type_td.field_index.get("__name") {
                slots[i] = Value::Str(simple.to_string().into());
            }
            if let Some(&i) = type_td.field_index.get("__fullName") {
                slots[i] = Value::Str(full.to_string().into());
            }
            ctx.heap().alloc_object(type_td, slots, native)
        }
        // z42.core not loaded — return a bare null Type (degraded). Reflection
        // is meaningless without z42.core, so this path is effectively dead.
        None => Value::Null,
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Pull the real `Arc<TypeDesc>` out of a `Std.Type`'s `NativeData::TypeHandle`.
/// Returns `None` for synthetic Types (primitives/arrays) so callers degrade
/// to empty results.
fn type_handle(args: &[Value]) -> Option<Arc<TypeDesc>> {
    match args.first() {
        Some(Value::Object(rc)) => {
            let obj = rc.borrow();
            match &obj.native {
                NativeData::TypeHandle(td) => Some(td.clone()),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Allocate a z42 class instance (`type_name`), writing the given named slots by
/// `field_index`. Unlisted slots stay `Null`. Errors only if the class isn't
/// loaded (a hard environment bug, not user-reachable).
fn alloc_named(ctx: &VmContext, type_name: &str, named: &[(&str, Value)]) -> Result<Value> {
    let td = ctx
        .try_lookup_type(type_name)
        .ok_or_else(|| anyhow::anyhow!("reflection: {type_name} not loaded (z42.core missing?)"))?;
    let mut slots = vec![Value::Null; td.fields.len()];
    for (k, v) in named {
        if let Some(&i) = td.field_index.get(*k) {
            slots[i] = v.clone();
        }
    }
    Ok(ctx.heap().alloc_object(td, slots, NativeData::None))
}

/// Derive a method's simple name from a qualified function name
/// (`"Demo.Point.Foo"` → `"Foo"`).
fn simple_method_name(qualified: &str) -> &str {
    qualified.rsplit('.').next().unwrap_or(qualified)
}

/// Read a string slot from a `Std.Type` object by `field_index`. Backs the
/// `Name` / `FullName` extern properties (both handle-carrying and synthetic
/// Types have these slots written by `build_type`).
fn read_type_str_slot(args: &[Value], field: &str) -> Value {
    if let Some(Value::Object(rc)) = args.first() {
        // `type_desc()` is the lockless accessor on the GcRef; slots come from
        // the locked guard.
        if let Some(i) = rc.type_desc().field_index.get(field).copied() {
            let obj = rc.borrow();
            return obj.slots.get(i).cloned().unwrap_or(Value::Null);
        }
    }
    Value::Null
}

// align-type-memberinfo-hierarchy (2026-06-11): `__type_name` / `builtin_type_name`
// removed — `Type.Name` now resolves to the inherited `Std.Reflection.MemberInfo`
// `Name` field (populated by `build_type`), no native getter.

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

// ── Field reflection ────────────────────────────────────────────────────────

/// True if `name` is a compiler-synthesized auto-property backing field
/// (`__prop_<PropName>`, emitted by the IrGen auto-property stubs). These are
/// hidden from `GetFields()` — the property surfaces via `GetProperties()`,
/// mirroring C#'s hidden `<Name>k__BackingField`.
fn is_autoprop_backing(name: &str) -> bool {
    name.starts_with("__prop_")
}

/// `__type_fields(typeObj) -> FieldInfo[]` — instance fields (incl. inherited;
/// base-first), each `FieldInfo { Name, FieldType: Type }`. Auto-property
/// backing fields (`__prop_*`) are excluded.
pub fn builtin_type_fields(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let td = match type_handle(args) {
        Some(t) => t,
        None => return Ok(ctx.heap().alloc_array(Vec::new())),
    };
    let mut out = Vec::with_capacity(td.fields.len() + td.static_fields().len());
    // Instance fields (already base-first — cross-zpkg fixup merges inherited
    // instance fields into `td.fields`; IsStatic = false). Auto-property backing
    // fields (compiler-synthesized `__prop_<Name>`, see IrGen auto-prop stubs) are
    // hidden from GetFields, mirroring C# which never surfaces the backing field —
    // the property is visible via GetProperties instead.
    for f in &td.fields {
        if is_autoprop_backing(&f.name) {
            continue;
        }
        out.push(build_field_info(ctx, &td.name, &f.name, &f.type_tag, false, f.visibility)?);
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
                out.push(build_field_info(ctx, &c.name, &f.name, &f.type_tag, true, f.visibility)?);
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
fn build_field_info(
    ctx: &VmContext,
    class: &str,
    field: &str,
    type_tag: &str,
    is_static: bool,
    visibility: u8,
) -> Result<Value> {
    let ftype = make_type_from_name(ctx, type_tag);
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

// ── Attribute reflection (C3 add-attribute-reflection) ──────────────────────

/// `__type_custom_attributes(typeObj) -> Std.Attribute[]` — live attribute
/// instances for this type's user attributes, in application order.
///
/// Each attribute is built by invoking its compiler-synthesized factory
/// `() => new T(args)` (a normal z42 function) via `run_returning`. Attribute
/// construction is thus fully statically known (known class, known constructor,
/// constant args baked into the factory body) — no runtime `Activator`/`Invoke`
/// and no generic instantiation. Re-entering the interpreter here is safe:
/// `exec_function` keeps all per-call state in a stack-local `Frame`.
///
/// z42-level `Type.GetCustomAttributes()` caches the returned array, so repeated
/// calls on the same Type yield the same instances. Empty array for a
/// handle-less Type or a type with no attributes.
pub fn builtin_type_custom_attributes(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    match type_handle(args) {
        Some(td) => call_attribute_factories(ctx, td.custom_attributes()),
        None => Ok(ctx.heap().alloc_array(Vec::new())),
    }
}

/// `__method_custom_attributes(qualified) -> Std.Attribute[]` — live attribute
/// instances for the method with the given qualified function name. C3b: the
/// z42 `MethodInfo` passes its hidden `__qualified` name; the builtin resolves
/// the backing `Function` (main module first, then lazy loader) and calls each
/// of its attribute factories. Same factory-call mechanism as the class path.
pub fn builtin_method_custom_attributes(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    // Args are [receiver MethodInfo, qualified: Str]; pick the string argument.
    let qualified = match args.iter().find_map(|v| match v {
        Value::Str(s) => Some(s.to_string()),
        _ => None,
    }) {
        Some(q) => q,
        None => return Ok(ctx.heap().alloc_array(Vec::new())),
    };
    let attrs: Vec<crate::metadata::bytecode::AttributeRef> = ctx
        .module()
        .and_then(|m| {
            m.func_index
                .get(qualified.as_str())
                .and_then(|&i| m.functions.get(i))
                .map(|f| f.custom_attributes().to_vec())
        })
        .or_else(|| ctx.try_lookup_function(&qualified).map(|f| f.custom_attributes().to_vec()))
        .unwrap_or_default();
    call_attribute_factories(ctx, &attrs)
}

/// `__field_custom_attributes(qualified) -> Std.Attribute[]` — live attribute
/// instances for the field named by "<Class>.<Field>" (FieldInfo passes its
/// hidden `__qualified`). Resolves the class TypeDesc, finds the field in
/// `cold.field_attributes`, and calls each factory. add-field-attribute-reflection.
pub fn builtin_field_custom_attributes(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let qualified = match args.iter().find_map(|v| match v {
        Value::Str(s) => Some(s.to_string()),
        _ => None,
    }) {
        Some(q) => q,
        None => return Ok(ctx.heap().alloc_array(Vec::new())),
    };
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
    let attrs: Vec<crate::metadata::bytecode::AttributeRef> = td
        .as_ref()
        .map(|td| {
            td.field_attributes()
                .iter()
                .find(|(n, _)| n.as_ref() == field)
                .map(|(_, refs)| refs.to_vec())
                .unwrap_or_default()
        })
        .unwrap_or_default();
    call_attribute_factories(ctx, &attrs)
}

/// `__param_custom_attributes(qualified, position) -> Std.Attribute[]` — live
/// attribute instances for parameter `position` (source index, excluding the
/// implicit `this`) of the method/function named by `qualified`. The z42
/// `ParameterInfo` passes its hidden `__qualified` + `__position`. The backing
/// `Function`'s `param_attributes` are SIGS-aligned (include the `this` slot),
/// so the wire index = position + (is_static ? 0 : 1). add-parameter-attribute-reflection.
pub fn builtin_param_custom_attributes(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let qualified = match args.iter().find_map(|v| match v {
        Value::Str(s) => Some(s.to_string()),
        _ => None,
    }) {
        Some(q) => q,
        None => return Ok(ctx.heap().alloc_array(Vec::new())),
    };
    let position = match args.iter().find_map(|v| match v {
        Value::I64(n) if *n >= 0 => Some(*n as usize),
        _ => None,
    }) {
        Some(p) => p,
        None => return Ok(ctx.heap().alloc_array(Vec::new())),
    };
    // Resolve the backing Function (main module first, then lazy loader) and read
    // its SIGS-aligned per-param attrs. wire_index = position + (this offset).
    let lookup = |f: &crate::metadata::bytecode::Function| {
        let wire_index = position + if f.is_static { 0 } else { 1 };
        f.param_attributes().get(wire_index).map(|a| a.to_vec())
    };
    let attrs: Vec<crate::metadata::bytecode::AttributeRef> = ctx
        .module()
        .and_then(|m| {
            m.func_index
                .get(qualified.as_str())
                .and_then(|&i| m.functions.get(i))
                .and_then(lookup)
        })
        .or_else(|| ctx.try_lookup_function(&qualified).and_then(|f| lookup(&f)))
        .unwrap_or_default();
    call_attribute_factories(ctx, &attrs)
}

/// Build live attribute instances by invoking each synthesized factory function
/// (`() => new T(args)`) via `run_returning`. Shared by the class
/// (`__type_custom_attributes`) and method (`__method_custom_attributes`) paths.
/// Cross-zpkg factories resolve via the lazy loader. Re-entering the interpreter
/// here is safe — `exec_function` keeps per-call state in a stack-local `Frame`.
fn call_attribute_factories(
    ctx: &VmContext,
    attrs: &[crate::metadata::bytecode::AttributeRef],
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
        let instance = if let Some(&idx) = module.func_index.get(a.factory_func.as_str()) {
            crate::interp::run_returning(ctx, &module, &module.functions[idx], &[])?
        } else if let Some(func) = ctx.try_lookup_function(&a.factory_func) {
            crate::interp::run_returning(ctx, &module, func.as_ref(), &[])?
        } else {
            None
        };
        out.push(instance.unwrap_or(Value::Null));
    }
    Ok(ctx.heap().alloc_array(out))
}

// ── Method reflection ───────────────────────────────────────────────────────

/// `__type_methods(typeObj) -> MethodInfo[]` — vtable (virtual/inherited) plus
/// declared non-virtual methods, deduped by qualified name.
pub fn builtin_type_methods(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let td = match type_handle(args) {
        Some(t) => t,
        None => return Ok(ctx.heap().alloc_array(Vec::new())),
    };
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    // Virtual / inherited methods carry their simple name in the vtable.
    for (simple, qualified) in &td.vtable {
        if seen.insert(qualified.clone()) {
            out.push(build_method_info(ctx, simple, qualified, true)?);
        }
    }
    // Declared non-virtual methods (qualified names only).
    for qualified in td.own_methods() {
        let q = qualified.to_string();
        if seen.insert(q.clone()) {
            let simple = simple_method_name(&q).to_string();
            out.push(build_method_info(ctx, &simple, &q, false)?);
        }
    }
    // add-interface-member-reflection: an interface carries its declared method
    // signatures in `iface_methods` (its vtable/own_methods are empty — interface
    // methods have no backing Function). Build MethodInfo straight from the sigs so
    // `typeof(IFoo).GetMethods()` surfaces the interface contract.
    for sig in td.iface_methods() {
        let q = format!("{}.{}", td.name, sig.name);
        if seen.insert(q) {
            out.push(build_iface_method_info(ctx, &td.name, sig)?);
        }
    }
    // add-reflection-transitive-interface-methods: for an interface, also surface
    // the methods of its (transitive) base interfaces — `interface IBar : IFoo`
    // makes `typeof(IBar).GetMethods()` include IFoo's methods (mirrors C#, whose
    // GetMethods on an interface returns the members of the whole interface set).
    // Only for interfaces: a class already carries its concrete interface-method
    // impls in the vtable, so it must not pull in the abstract sigs as extra
    // MethodInfos. BFS the base-interface closure (same walk as GetInterfaces),
    // qualifying each method by its *declaring* interface so dedup is per-source.
    if td.class_flags & crate::metadata::bytecode::CLASS_FLAG_INTERFACE != 0 {
        let resolve = |name: &str| {
            ctx.module()
                .and_then(|m| m.type_registry.get(name).cloned())
                .or_else(|| ctx.try_lookup_type(name))
        };
        let mut queue: VecDeque<String> =
            td.interfaces().iter().map(|s| s.to_string()).collect();
        let mut seen_ifaces: HashSet<String> = HashSet::new();
        while let Some(name) = queue.pop_front() {
            if !seen_ifaces.insert(name.clone()) {
                continue;
            }
            if let Some(itd) = resolve(&name) {
                for sig in itd.iface_methods() {
                    let q = format!("{}.{}", itd.name, sig.name);
                    if seen.insert(q) {
                        out.push(build_iface_method_info(ctx, &itd.name, sig)?);
                    }
                }
                for bi in itd.interfaces() {
                    queue.push_back(bi.to_string());
                }
            }
        }
    }
    Ok(ctx.heap().alloc_array(out))
}

/// Build a `MethodInfo` by resolving the backing `Function` for its signature.
/// Missing Function (extern/native or unresolved) → name-only MethodInfo.
fn build_method_info(
    ctx: &VmContext,
    simple: &str,
    qualified: &str,
    is_virtual: bool,
) -> Result<Value> {
    // stabilize-dispatch-keys (方案A, 2026-07-14): dispatch keys now carry a
    // full `$N$types` mangle suffix for every method (vtable slots + own_methods
    // keys included), so the derived `simple` name may be e.g. `Foo$1$int`.
    // Reflection presents the source-level name — strip the mangle suffix for
    // the user-facing `MethodInfo.Name` (dispatch still uses `qualified`).
    let simple = simple.split('$').next().unwrap_or(simple);
    let (ret_tag, is_static, params, visibility, method_flags, sig_found) = match resolve_func_sig(ctx, qualified) {
        Some((param_count, ret_type, fn_is_static, param_types, param_names, vis, mf, min_arg, params_from, param_defaults)) => {
            // Instance methods carry `this` at param 0 — skip it.
            let start = if fn_is_static { 0 } else { 1 };
            let mut params = Vec::new();
            for i in start..param_count {
                let tag = param_types.get(i).map(|s| s.as_str()).unwrap_or("?");
                let pos = (i - start) as i64;   // logical position (0-based, excludes `this`)
                // add-param-metadata (unify P1-d): SIGS name preferred (via resolve_func_sig),
                // `arg{n}` fallback otherwise.
                let name = param_names
                    .get(i)
                    .filter(|n| !n.is_empty())
                    .cloned()
                    .unwrap_or_else(|| format!("arg{pos}"));
                // add-param-metadata: IsOptional / IsParams (logical position) + DefaultValue.
                let is_optional = (pos as u16) >= min_arg;
                let is_params = params_from != 0xFF && pos as u8 == params_from;
                let default_value = match param_defaults.get(i) {
                    Some((1, _, _)) => Value::Null,
                    Some((2, iv, _)) => Value::I64(*iv),
                    Some((3, iv, _)) => Value::F64(f64::from_bits(*iv as u64)),
                    Some((4, iv, _)) => Value::Bool(*iv != 0),
                    Some((5, _, sv)) => Value::Str(sv.clone().into()),
                    _ => Value::Null, // kind 0 = no (foldable) default
                };
                params.push(alloc_named(
                    ctx,
                    STD_REFLECTION_PARAMINFO,
                    &[
                        ("Name", Value::Str(name.into())),
                        ("ParameterType", make_type_from_name(ctx, tag)),
                        ("Position", Value::I64(pos)),
                        ("IsOptional", Value::Bool(is_optional)),
                        ("IsParams", Value::Bool(is_params)),
                        ("DefaultValue", default_value),
                        // add-parameter-attribute-reflection: backing func name so
                        // ParameterInfo.GetCustomAttributes() can resolve the param's
                        // attribute factories (paired with Position).
                        ("__qualified", Value::Str(qualified.to_string().into())),
                    ],
                )?);
            }
            (ret_type, fn_is_static, params, vis, mf, true)
        }
        None => ("void".to_string(), false, Vec::new(), 0, 0, false),
    };
    // add-method-modifiers (unify P1-c): IsVirtual authoritative from the flag
    // (bit0) WHEN a SIGS entry was resolved — z42 lists every method (virtual or
    // not) in the vtable, so vtable-presence alone over-reports. Fall back to the
    // vtable-presence hint only for methods with no resolvable SIGS (synthesized).
    // IsAbstract from bit1.
    let is_virtual_flag = if sig_found {
        (method_flags & crate::metadata::bytecode::METHOD_FLAG_VIRTUAL) != 0
    } else {
        is_virtual
    };
    let is_abstract_flag = (method_flags & crate::metadata::bytecode::METHOD_FLAG_ABSTRACT) != 0;
    let params_arr = ctx.heap().alloc_array(params);
    alloc_named(
        ctx,
        STD_REFLECTION_METHODINFO,
        &[
            ("Name", Value::Str(simple.to_string().into())),
            ("ReturnType", make_type_from_name(ctx, &ret_tag)),
            ("IsStatic", Value::Bool(is_static)),
            ("IsVirtual", Value::Bool(is_virtual_flag)),
            // add-method-modifiers (unify P1-c): abstract methods (bit1).
            ("IsAbstract", Value::Bool(is_abstract_flag)),
            // add-member-visibility (unify P1-b): 0=public / 1=private /
            // 2=protected. `protected` reports neither (mirrors C# IsFamily).
            ("IsPublic", Value::Bool(visibility == 0)),
            ("IsPrivate", Value::Bool(visibility == 1)),
            ("__parameters", params_arr),
            // C3b: qualified func name so MethodInfo.GetCustomAttributes() can
            // resolve the backing Function's attribute factories.
            ("__qualified", Value::Str(qualified.to_string().into())),
        ],
    )
}

/// add-interface-member-reflection: build a `MethodInfo` for an interface-declared
/// method straight from its signature (`IfaceMethodSig`). Interface methods have
/// NO backing `Function` (no body — not in SIGS/FUNC), so unlike `build_method_info`
/// this reads the return/param types from the signature block. Interface methods
/// are implicitly public, instance, abstract, and virtual. Parameters carry types
/// but no source names (`arg{n}`) — the block records types only.
fn build_iface_method_info(
    ctx: &VmContext,
    iface_fqn: &str,
    sig: &crate::metadata::bytecode::IfaceMethodSig,
) -> Result<Value> {
    let simple = sig.name.split('$').next().unwrap_or(&sig.name);
    let qualified = format!("{iface_fqn}.{}", sig.name);
    let mut params = Vec::with_capacity(sig.param_types.len());
    for (i, ptype) in sig.param_types.iter().enumerate() {
        let pos = i as i64;
        params.push(alloc_named(
            ctx,
            STD_REFLECTION_PARAMINFO,
            &[
                ("Name", Value::Str(format!("arg{pos}").into())),
                ("ParameterType", make_type_from_name(ctx, ptype)),
                ("Position", Value::I64(pos)),
                ("IsOptional", Value::Bool(false)),
                ("IsParams", Value::Bool(false)),
                ("DefaultValue", Value::Null),
                ("__qualified", Value::Str(qualified.clone().into())),
            ],
        )?);
    }
    let params_arr = ctx.heap().alloc_array(params);
    alloc_named(
        ctx,
        STD_REFLECTION_METHODINFO,
        &[
            ("Name", Value::Str(simple.to_string().into())),
            ("ReturnType", make_type_from_name(ctx, &sig.ret_type)),
            ("IsStatic", Value::Bool(false)),
            ("IsVirtual", Value::Bool(true)),
            ("IsAbstract", Value::Bool(true)),
            ("IsPublic", Value::Bool(true)),
            ("IsPrivate", Value::Bool(false)),
            ("__parameters", params_arr),
            ("__qualified", Value::Str(qualified.into())),
        ],
    )
}

/// Resolve a function's signature `(param_count, ret_type, is_static, param_types)`
/// by qualified name. Checks the **main module**'s `func_index` first (the
/// user program's own methods), then the lazy loader (stdlib / zpkg methods).
/// `try_lookup_function` alone misses main-module functions.
fn resolve_func_sig(
    ctx: &VmContext,
    qualified: &str,
) -> Option<(usize, String, bool, Vec<String>, Vec<String>, u8, u8, u16, u8, Vec<(u8, i64, String)>)> {
    // reflection-future-parameter-names: parameters occupy registers
    // 0..param_count on entry, so a param's source name is the debug local-var
    // whose `reg` matches its index. Empty string when no debug symbols are
    // present (the builder falls back to `arg{n}`).
    fn extract(
        f: &crate::metadata::bytecode::Function,
    ) -> (usize, String, bool, Vec<String>, Vec<String>, u8, u8, u16, u8, Vec<(u8, i64, String)>) {
        // add-param-metadata (unify P1-d): prefer the authoritative SIGS param
        // names; fall back to the DBUG local-var guess (empty → `arg{n}` later).
        let sigs_names = f.param_names();
        let names = if sigs_names.len() == f.param_count && sigs_names.iter().any(|n| !n.is_empty()) {
            sigs_names.to_vec()
        } else {
            let mut names = vec![String::new(); f.param_count];
            for lv in f.local_vars() {
                let r = lv.reg as usize;
                if r < f.param_count {
                    names[r] = lv.name.clone();
                }
            }
            names
        };
        (f.param_count, f.ret_type.clone(), f.is_static, f.param_types().to_vec(), names,
         f.visibility, f.method_flags, f.min_arg, f.params_from, f.param_defaults().to_vec())
    }
    if let Some(m) = ctx.module() {
        if let Some(&i) = m.func_index.get(qualified) {
            if let Some(f) = m.functions.get(i) {
                return Some(extract(f));
            }
        }
    }
    ctx.try_lookup_function(qualified).map(|f| extract(&f))
}

// ── Property reflection ─────────────────────────────────────────────────────

/// `__type_properties(typeObj) -> PropertyInfo[]` — properties derived from the
/// `get_<X>` / `set_<X>` accessor-method convention (auto-properties desugar to
/// field + get_/set_ methods). No persisted PropertyDesc metadata and no
/// wire-format change: the accessor names already live in the vtable /
/// own_methods, and the property type comes from the accessor signature (same
/// source as MethodInfo). Getter + setter for the same name merge into one
/// PropertyInfo (CanRead && CanWrite). Empty for a handle-less Type.
pub fn builtin_type_properties(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let td = match type_handle(args) {
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
    let mut out = Vec::with_capacity(props.len());
    for p in &props {
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
        out.push(alloc_named(
            ctx,
            STD_REFLECTION_PROPERTYINFO,
            &[
                ("Name", Value::Str(p.name.clone().into())),
                ("PropertyType", make_type_from_name(ctx, type_tag)),
                ("CanRead", Value::Bool(p.getter_type.is_some())),
                ("CanWrite", Value::Bool(p.setter_type.is_some())),
                ("__getterQualified", getter_q),
                ("__setterQualified", setter_q),
            ],
        )?);
    }
    Ok(ctx.heap().alloc_array(out))
}

/// Accumulator for merging a property's getter + setter into one PropertyInfo.
struct PropAccum {
    name: String,
    getter_type: Option<String>,
    setter_type: Option<String>,
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
fn accumulate_property(ctx: &VmContext, simple: &str, qualified: &str, props: &mut Vec<PropAccum>) {
    let (is_get, prop_name) = match (simple.strip_prefix("get_"), simple.strip_prefix("set_")) {
        (Some(n), _) => (true, n),
        (_, Some(n)) => (false, n),
        _ => return,
    };
    if prop_name.is_empty() {
        return;
    }
    let sig = resolve_func_sig(ctx, qualified);
    if is_get {
        let ty = match &sig {
            Some((pc, ret, is_static, _, _, _, _, _, _, _)) => {
                if pc.saturating_sub(if *is_static { 0 } else { 1 }) != 0 {
                    return; // get_X(args) — a regular method, not a property
                }
                ret.clone()
            }
            None => "?".to_string(),
        };
        let p = upsert_prop(props, prop_name);
        p.getter_type = Some(ty);
        p.getter_qualified = Some(qualified.to_string());
    } else {
        let ty = match &sig {
            Some((pc, _, is_static, ptypes, _, _, _, _, _, _)) => {
                let base = if *is_static { 0 } else { 1 };
                if pc.saturating_sub(base) != 1 {
                    return; // set_X with != 1 value param — a regular method
                }
                ptypes.get(base).cloned().unwrap_or_else(|| "?".to_string())
            }
            None => "?".to_string(),
        };
        let p = upsert_prop(props, prop_name);
        p.setter_type = Some(ty);
        p.setter_qualified = Some(qualified.to_string());
    }
}

/// Find-or-insert a property accumulator by name, preserving first-seen order.
fn upsert_prop<'a>(props: &'a mut Vec<PropAccum>, name: &str) -> &'a mut PropAccum {
    if let Some(i) = props.iter().position(|p| p.name == name) {
        return &mut props[i];
    }
    props.push(PropAccum {
        name: name.to_string(),
        getter_type: None,
        setter_type: None,
        getter_qualified: None,
        setter_qualified: None,
    });
    let last = props.len() - 1;
    &mut props[last]
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

// ── Runtime generic instantiation (G1, plan-generic-reflection) ──────────────

/// Extract a resolvable name from a generic-type-argument `Std.Type` value:
/// handle-backed → the TypeDesc FQ name; array → `<elem>[]`; else `__fullName`.
fn type_arg_name(v: &Value) -> Option<String> {
    if let Value::Object(rc) = v {
        if let NativeData::TypeHandle(td) = &rc.borrow().native {
            return Some(td.name.clone());
        }
    }
    // Array Type carries a non-empty `__elementName` → reconstruct `<elem>[]`.
    // (Non-array Types initialize the slot to "" — guard against it, else a
    // primitive like `typeof(int)` would wrongly become `[]` → an Array type.)
    if let Value::Str(elem) = read_obj_slot(v, "__elementName") {
        if !elem.is_empty() {
            return Some(format!("{elem}[]"));
        }
    }
    match read_obj_slot(v, "__fullName") {
        Value::Str(s) => Some(s.to_string()),
        _ => None,
    }
}

/// Resolve a type name to its `TypeDesc` (main module then lazy loader).
fn resolve_td(ctx: &VmContext, name: &str) -> Option<Arc<TypeDesc>> {
    ctx.module()
        .and_then(|m| m.type_registry.get(name).cloned())
        .or_else(|| ctx.try_lookup_type(name))
}

/// True if `derived` is assignable to `target` (subclass / interface impl / self),
/// via the VM's canonical subclass walk. Handle-less names compare by identity.
fn type_name_assignable(ctx: &VmContext, derived: &str, target: &str) -> bool {
    match ctx.module() {
        Some(m) => crate::interp::dispatch::is_subclass_or_eq_td(ctx, &m.type_registry, derived, target),
        None => derived == target,
    }
}

/// True if type `arg_name` satisfies a base-class / interface constraint named
/// `constraint`. First tries the VM's exact-FQ subclass walk; then falls back to
/// a simple-name match down `arg_name`'s transitive base + interface chain —
/// because `where T: IEntity` stores the *source spelling* (`IEntity`, often
/// unqualified) while a type's interface list carries FQ names.
fn constraint_satisfied_by(ctx: &VmContext, arg_name: &str, constraint: &str) -> bool {
    if type_name_assignable(ctx, arg_name, constraint) {
        return true;
    }
    let target_simple = constraint.rsplit('.').next().unwrap_or(constraint);
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut stack = vec![arg_name.to_string()];
    while let Some(cur) = stack.pop() {
        if !seen.insert(cur.clone()) {
            continue;
        }
        if cur.rsplit('.').next().unwrap_or(&cur) == target_simple {
            return true;
        }
        if let Some(td) = resolve_td(ctx, &cur) {
            if let Some(b) = &td.base_name {
                stack.push(b.clone());
            }
            for i in td.interfaces() {
                stack.push(i.to_string());
            }
        }
    }
    false
}

/// Numeric / bool / char primitive names are value types (`struct` constraint
/// satisfiers); `string` / `object` are reference types.
fn is_value_type_primitive(name: &str) -> bool {
    matches!(
        name,
        "int" | "long" | "short" | "sbyte" | "byte" | "ushort" | "uint" | "ulong"
            | "float" | "double" | "bool" | "char"
            | "i8" | "u8" | "i16" | "u16" | "i32" | "u32" | "i64" | "u64" | "f32" | "f64"
    )
}

/// Q3 safety: validate one type argument against its parameter's constraint
/// bundle. Reflection (`MakeGenericType`) is the sole entry that bypasses the
/// compiler's compile-time constraint checking, so it MUST self-police — else a
/// reflectively-constructed type could violate `where` clauses and crash later.
/// Throws a catchable `Std.Exception` on violation.
fn validate_type_arg_constraint(
    ctx: &VmContext,
    param_name: &str,
    cb: &crate::metadata::bytecode::ConstraintBundle,
    arg_name: &str,
    type_params: &[String],
    all_arg_names: &[String],
) -> Result<()> {
    let arg_td = resolve_td(ctx, arg_name);
    let is_value = arg_td
        .as_ref()
        .map(|td| td.class_flags & crate::metadata::bytecode::CLASS_FLAG_STRUCT != 0)
        .unwrap_or_else(|| is_value_type_primitive(arg_name));
    let is_enum = arg_td.as_ref().map(|td| td.is_enum()).unwrap_or(false);

    if cb.requires_class && is_value {
        bail!("MakeGenericType: `{arg_name}` for type parameter `{param_name}` must be a reference type (constraint `class`)");
    }
    if cb.requires_struct && !is_value {
        bail!("MakeGenericType: `{arg_name}` for type parameter `{param_name}` must be a value type (constraint `struct`)");
    }
    if cb.requires_enum && !is_enum {
        bail!("MakeGenericType: `{arg_name}` for type parameter `{param_name}` must be an enum (constraint `enum`)");
    }
    if let Some(base) = &cb.base_class {
        if !constraint_satisfied_by(ctx, arg_name, base) {
            bail!("MakeGenericType: `{arg_name}` for type parameter `{param_name}` must derive from `{base}`");
        }
    }
    for iface in &cb.interfaces {
        if !constraint_satisfied_by(ctx, arg_name, iface) {
            bail!("MakeGenericType: `{arg_name}` for type parameter `{param_name}` must implement `{iface}`");
        }
    }
    if cb.requires_constructor {
        // Primitives satisfy `new()`; a class arg must be non-abstract with a
        // reachable no-arg ctor (or no explicit ctor = default construction).
        if let Some(td) = &arg_td {
            let abstract_ = td.class_flags & crate::metadata::bytecode::CLASS_FLAG_ABSTRACT != 0;
            if abstract_ || !type_has_no_arg_ctor(ctx, &td.name) {
                bail!("MakeGenericType: `{arg_name}` for type parameter `{param_name}` must have a public parameterless constructor (constraint `new()`)");
            }
        }
    }
    // where <param>: <other type parameter> (bare-typeparam L3-G2.5): the arg for
    // `param` must be assignable to the arg bound to `other_tp`.
    if let Some(other_tp) = &cb.type_param_constraint {
        if let Some(idx) = type_params.iter().position(|p| p == other_tp) {
            if let Some(other_arg) = all_arg_names.get(idx) {
                if !type_name_assignable(ctx, arg_name, other_arg) {
                    bail!("MakeGenericType: `{arg_name}` for type parameter `{param_name}` must be assignable to `{other_arg}` (constraint `{other_tp}`)");
                }
            }
        }
    }
    Ok(())
}

/// True if the class `name` has a reachable no-arg constructor (or no explicit
/// ctor at all → default field construction). Mirrors `builtin_activator_create`.
fn type_has_no_arg_ctor(ctx: &VmContext, name: &str) -> bool {
    let simple = name.rsplit('.').next().unwrap_or(name);
    let cand_bare = format!("{name}.{simple}");
    let cand_zero = format!("{name}.{simple}$0");
    let Some(m) = ctx.module() else { return true };
    // Any explicit ctor with args means no-arg construction may be unavailable;
    // but a class with NO ctor at all constructs by default → treat as satisfied.
    let has_bare = m.func_index.contains_key(&cand_bare) || ctx.try_lookup_function(&cand_bare).is_some();
    let has_zero = m.func_index.contains_key(&cand_zero) || ctx.try_lookup_function(&cand_zero).is_some();
    let has_any_ctor = m.func_index.keys().any(|k| k.starts_with(&format!("{name}.{simple}$")));
    has_bare || has_zero || !has_any_ctor
}

/// `__type_make_generic(defType, Type[] argTypes) -> Std.Type` — construct a
/// generic type at runtime (`typeof(List<>).MakeGenericType(typeof(int))` →
/// `List<int>`). Reuses `make_constructed_type` (z42's reified type-erasure means
/// no monomorphization/codegen). Validates arg count + `where` constraints
/// (Q3 safety) before constructing.
pub fn builtin_type_make_generic(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let def_td = type_handle(args)
        .ok_or_else(|| anyhow::anyhow!("MakeGenericType: receiver is not a type handle"))?;
    let type_params: Vec<String> = def_td.type_params().to_vec();
    if type_params.is_empty() {
        bail!("MakeGenericType: `{}` is not a generic type definition", def_td.name);
    }
    let arg_types: Vec<Value> = match args.get(1) {
        Some(Value::Array(rc)) => rc.borrow().iter().cloned().collect(),
        _ => bail!("MakeGenericType: expected a Type[] of type arguments"),
    };
    if arg_types.len() != type_params.len() {
        bail!(
            "MakeGenericType: `{}` expects {} type argument(s), got {}",
            def_td.name,
            type_params.len(),
            arg_types.len()
        );
    }
    let mut arg_names: Vec<String> = Vec::with_capacity(arg_types.len());
    for at in &arg_types {
        let n = type_arg_name(at)
            .ok_or_else(|| anyhow::anyhow!("MakeGenericType: unresolvable type argument"))?;
        arg_names.push(n);
    }
    // Q3: constraint validation before construction.
    let constraints = def_td.type_param_constraints();
    for (i, param) in type_params.iter().enumerate() {
        if let Some(cb) = constraints.get(i) {
            validate_type_arg_constraint(ctx, param, cb, &arg_names[i], &type_params, &arg_names)?;
        }
    }
    Ok(make_constructed_type(ctx, &def_td.name, &arg_names))
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
        out.extend(a.borrow().iter().cloned());
    }
    if let Value::Array(m) = builtin_type_methods(ctx, args)? {
        out.extend(m.borrow().iter().cloned());
    }
    if let Value::Array(p) = builtin_type_properties(ctx, args)? {
        out.extend(p.borrow().iter().cloned());
    }
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
fn is_primitive_type_name(name: &str) -> bool {
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
fn class_flag_set(args: &[Value], bit: u8) -> bool {
    type_handle(args)
        .map(|td| td.class_flags & bit != 0)
        .unwrap_or(false)
}

// ── Reflective invocation (add-method-invoke-non-generic, 0.3.12) ────────────

/// Read a named slot from any ScriptObject `Value` (e.g. a `MethodInfo`'s hidden
/// `__qualified` / `IsStatic`). `Null` if not an object or no such field.
fn read_obj_slot(v: &Value, field: &str) -> Value {
    if let Value::Object(rc) = v {
        if let Some(i) = rc.type_desc().field_index.get(field).copied() {
            return rc.borrow().slots.get(i).cloned().unwrap_or(Value::Null);
        }
    }
    Value::Null
}

/// `__type_get_type(fqn: str) -> Type` — FQN string → `Std.Type` (null if unknown).
/// Thin wrapper over `make_type_from_name` (main registry + lazy loader + synthetic).
pub fn builtin_type_get_type(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    match args.first() {
        Some(Value::Str(s)) => Ok(make_type_from_name(ctx, s)),
        _ => Ok(Value::Null),
    }
}

/// `__method_invoke(method: MethodInfo, obj: object, args: object[]) -> object`.
/// Reflectively invokes the method named by the MethodInfo's hidden `__qualified`:
/// instance methods take `obj` as receiver (reg 0); static methods ignore it.
/// `args` (an `object[]`) maps to the remaining parameters in order. Returns the
/// method's return value (`void` → null). A `throw` inside the invoked method is
/// propagated with its ORIGINAL type via `ctx.set_pending_thrown` (consumed by
/// `exec_call::builtin`), so callers can `try/catch` the real exception.
pub fn builtin_method_invoke(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let mi = args.first().cloned().unwrap_or(Value::Null);
    let this_obj = args.get(1).cloned().unwrap_or(Value::Null);
    let args_arr = args.get(2).cloned().unwrap_or(Value::Null);

    let qualified = match read_obj_slot(&mi, "__qualified") {
        Value::Str(s) => s.to_string(),
        _ => bail!("MethodInfo.Invoke: receiver is not a MethodInfo (no __qualified)"),
    };
    let is_static = matches!(read_obj_slot(&mi, "IsStatic"), Value::Bool(true));

    // Assemble call args: instance receiver first (reg 0), then the object[] elements.
    let mut call_args: Vec<Value> = Vec::new();
    if !is_static {
        call_args.push(this_obj);
    }
    if let Value::Array(rc) = &args_arr {
        for e in rc.borrow().iter() {
            call_args.push(e.clone());
        }
    }

    invoke_qualified(ctx, &qualified, &call_args)
}

/// Shared reflective-invocation core: resolve `qualified` (main module first,
/// then lazy loader), arity-check against the already-assembled `call_args`
/// (receiver-first for instance methods), execute, and normalize the outcome.
/// A `throw` inside the callee propagates with its ORIGINAL type via
/// `ctx.set_pending_thrown` (consumed by `exec_call::builtin`). Shared by
/// `MethodInfo.Invoke` and `PropertyInfo.GetValue` / `SetValue`.
fn invoke_qualified(ctx: &VmContext, qualified: &str, call_args: &[Value]) -> Result<Value> {
    let module_arc = ctx
        .core
        .module
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("reflective invoke: VmCore.module is None"))?
        .clone();
    let module = module_arc.as_ref();

    let outcome = match module.func_index.get(qualified) {
        Some(&idx) => {
            let f = &module.functions[idx];
            invoke_arity_check(qualified, f.param_count, call_args.len())?;
            exec_function(ctx, module, f, call_args)?
        }
        None => {
            let f = ctx.try_lookup_function(qualified).ok_or_else(|| {
                anyhow::anyhow!("reflective invoke: function `{qualified}` not found")
            })?;
            invoke_arity_check(qualified, f.param_count, call_args.len())?;
            exec_function(ctx, module, f.as_ref(), call_args)?
        }
    };

    match outcome {
        ExecOutcome::Returned(Some(v)) => Ok(v),
        ExecOutcome::Returned(None) => Ok(Value::Null),
        ExecOutcome::Thrown(val) => {
            // Propagate the ORIGINAL exception value (preserving its type) through
            // the builtin error channel; exec_call::builtin re-raises it.
            ctx.set_pending_thrown(val);
            bail!("__z42_reflected_throw__")
        }
    }
}

/// `__property_get_value(prop: PropertyInfo, target: object) -> object`.
/// Reflectively reads a property by invoking its `get_<Name>` accessor (whose
/// qualified name the VM stamped onto `__getterQualified` in
/// `builtin_type_properties`). `target` is the receiver (reg 0). A read-only
/// property (no getter) raises a catchable `Std.Exception`.
pub fn builtin_property_get_value(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let pi = args.first().cloned().unwrap_or(Value::Null);
    let target = args.get(1).cloned().unwrap_or(Value::Null);
    let getter = match read_obj_slot(&pi, "__getterQualified") {
        Value::Str(s) => s.to_string(),
        _ => bail!("PropertyInfo.GetValue: property has no getter (write-only)"),
    };
    invoke_qualified(ctx, &getter, &[target])
}

/// `__property_set_value(prop: PropertyInfo, target: object, value: object)`.
/// Reflectively writes a property by invoking its `set_<Name>` accessor (whose
/// qualified name the VM stamped onto `__setterQualified`). `target` is the
/// receiver (reg 0), `value` the assigned value. A read-only property (no
/// setter) raises a catchable `Std.Exception`.
pub fn builtin_property_set_value(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let pi = args.first().cloned().unwrap_or(Value::Null);
    let target = args.get(1).cloned().unwrap_or(Value::Null);
    let value = args.get(2).cloned().unwrap_or(Value::Null);
    let setter = match read_obj_slot(&pi, "__setterQualified") {
        Value::Str(s) => s.to_string(),
        _ => bail!("PropertyInfo.SetValue: property has no setter (read-only)"),
    };
    invoke_qualified(ctx, &setter, &[target, value])
}

/// `__field_get_value(field: FieldInfo, target: object) -> object` — read an
/// instance field's value straight off the target object's slot (by the field's
/// `Name` → the object's own `field_index`). Unlike `PropertyInfo.GetValue`
/// there is no accessor: a field IS a slot. Powers reflective (de)serialization.
pub fn builtin_field_get_value(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let fi = args.first().cloned().unwrap_or(Value::Null);
    let target = args.get(1).cloned().unwrap_or(Value::Null);
    let name = match read_obj_slot(&fi, "Name") {
        Value::Str(s) => s.to_string(),
        _ => bail!("FieldInfo.GetValue: receiver is not a FieldInfo"),
    };
    match &target {
        Value::Object(rc) => match rc.type_desc().field_index.get(&name).copied() {
            Some(i) => Ok(rc.borrow().slots.get(i).cloned().unwrap_or(Value::Null)),
            None => bail!("FieldInfo.GetValue: field `{name}` not present on target instance"),
        },
        _ => bail!("FieldInfo.GetValue: target is not an object instance"),
    }
}

/// `__field_set_value(field: FieldInfo, target: object, value: object)` — write
/// an instance field's slot directly (by `Name` → `field_index`). Powers
/// reflective deserialization (binding JSON members onto plain public fields).
pub fn builtin_field_set_value(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let fi = args.first().cloned().unwrap_or(Value::Null);
    let target = args.get(1).cloned().unwrap_or(Value::Null);
    let value = args.get(2).cloned().unwrap_or(Value::Null);
    let name = match read_obj_slot(&fi, "Name") {
        Value::Str(s) => s.to_string(),
        _ => bail!("FieldInfo.SetValue: receiver is not a FieldInfo"),
    };
    match &target {
        Value::Object(rc) => match rc.type_desc().field_index.get(&name).copied() {
            Some(i) => {
                rc.borrow_mut().slots[i] = value;
                Ok(Value::Null)
            }
            None => bail!("FieldInfo.SetValue: field `{name}` not present on target instance"),
        },
        _ => bail!("FieldInfo.SetValue: target is not an object instance"),
    }
}

/// `__invoke_static(fqn: str) -> object` — invoke a free / static function by its
/// fully-qualified name with NO arguments (no receiver). This is the path the
/// reflective test runner uses for `[Test]` / `[Benchmark]` / `[Setup]` /
/// `[Teardown]` methods, which the compiler emits as zero-arg free functions
/// (`<Namespace>.<func>`) — not class instance methods. A `throw` inside the
/// invoked function is propagated with its ORIGINAL type via
/// `ctx.set_pending_thrown` (consumed by `exec_call::builtin` / `jit_builtin`),
/// so the runner can `try/catch` the real exception. Returns the function's
/// return value (`void` → null).
pub fn builtin_invoke_static(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let qualified = match args.first() {
        Some(Value::Str(s)) => s.to_string(),
        _ => bail!("__invoke_static: expected a fully-qualified function name string"),
    };
    let module_arc = ctx
        .core
        .module
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("__invoke_static: VmCore.module is None"))?
        .clone();
    let module = module_arc.as_ref();

    let outcome = match module.func_index.get(&qualified) {
        Some(&idx) => {
            let f = &module.functions[idx];
            invoke_arity_check(&qualified, f.param_count, 0)?;
            exec_function(ctx, module, f, &[])?
        }
        None => {
            let f = ctx.try_lookup_function(&qualified).ok_or_else(|| {
                anyhow::anyhow!("__invoke_static: function `{qualified}` not found")
            })?;
            invoke_arity_check(&qualified, f.param_count, 0)?;
            exec_function(ctx, module, f.as_ref(), &[])?
        }
    };

    match outcome {
        ExecOutcome::Returned(Some(v)) => Ok(v),
        ExecOutcome::Returned(None) => Ok(Value::Null),
        ExecOutcome::Thrown(val) => {
            ctx.set_pending_thrown(val);
            bail!("__z42_reflected_throw__")
        }
    }
}

fn invoke_arity_check(qualified: &str, expected: usize, got: usize) -> Result<()> {
    if expected != got {
        bail!("MethodInfo.Invoke: `{qualified}` expects {expected} argument(s) (incl. receiver), got {got}");
    }
    Ok(())
}

/// `__activator_create(type: Type) -> object` — no-arg reflective construction.
/// Mirrors the interpreter's `ObjNew`: alloc with per-field defaults, then run the
/// no-arg ctor (`<Class>` bare or `<Class>$0` overload-mangled) if one exists. A
/// ctor `throw` propagates with its original type via `ctx.pending_thrown`. Only
/// no-arg construction (test-class instantiation for the reflective test runner);
/// parameterised CreateInstance is deferred.
pub fn builtin_activator_create(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let td = match type_handle(args) {
        Some(td) => td,
        None => bail!("Activator.CreateInstance: type has no runtime handle (primitive/array/synthetic?)"),
    };
    let class_name = td.name.clone();
    let slots: Vec<Value> = td
        .fields
        .iter()
        .map(|f| crate::metadata::default_value_for(&f.type_tag))
        .collect();
    let obj = ctx.heap().alloc_object(td.clone(), slots, NativeData::None);
    if matches!(obj, Value::Null) {
        bail!("Activator.CreateInstance: allocation failed for `{class_name}`");
    }

    // plan-generic-reflection G1: a constructed generic Type (typeof(Box<int>) or
    // MakeGenericType(...)) carries its argument Types in `__typeArgs`; reify them
    // onto the new instance's `type_args` (mirrors ObjNew) so reflection survives —
    // `Activator.CreateInstance(typeof(Box<>).MakeGenericType(t)).GetType()
    // .GetGenericArguments()` returns the args. Non-generic Type → slot absent → no-op.
    if let Value::Array(rc) = read_type_str_slot(args, "__typeArgs") {
        let names: Vec<String> = rc.borrow().iter().filter_map(type_arg_name).collect();
        if !names.is_empty() {
            if let Value::Object(orc) = &obj {
                orc.borrow_mut().type_args = names.into_boxed_slice();
            }
        }
    }

    let module_arc = ctx
        .core
        .module
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Activator.CreateInstance: VmCore.module is None"))?
        .clone();
    let module = module_arc.as_ref();

    // No-arg ctor function name = "<FQClass>.<SimpleName>" (ctors are named like
    // the class, same scheme as methods: Demo.Counter.Counter), with "$0" when the
    // ctor is overload-mangled. A class with no explicit ctor has neither → the
    // default-field alloc IS construction.
    let simple = class_name.rsplit('.').next().unwrap_or(class_name.as_str());
    let cand_bare = format!("{class_name}.{simple}");
    let cand_zero = format!("{class_name}.{simple}$0");
    let mut i = 0;
    while i < 2 {
        let cand = if i == 0 { cand_bare.as_str() } else { cand_zero.as_str() };
        i += 1;
        let outcome = match module.func_index.get(cand) {
            Some(&idx) => Some(exec_function(ctx, module, &module.functions[idx], &[obj.clone()])?),
            None => match ctx.try_lookup_function(cand) {
                Some(f) => Some(exec_function(ctx, module, f.as_ref(), &[obj.clone()])?),
                None => None,
            },
        };
        if let Some(o) = outcome {
            if let ExecOutcome::Thrown(val) = o {
                ctx.set_pending_thrown(val);
                bail!("__z42_reflected_throw__");
            }
            break; // ran the first matching ctor
        }
    }
    Ok(obj)
}

/// `__load_module(path: str) -> Std.Test.TestEntry[]` — load a compiled test
/// module at `path` into the live VM (its functions / types become callable +
/// reflectable) and return its TIDX entries as z42 `Std.Test.TestEntry` objects.
/// Powers `Std.Test.ModuleLoader.Load` so a z42 test runner can load a compiled
/// test module and discover + `Invoke` its `[Test]` methods. (retire-test-runner)
pub fn builtin_load_module(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let path = match args.first() {
        Some(Value::Str(s)) => s.to_string(),
        _ => bail!("ModuleLoader.Load: expected a path string"),
    };
    let entries = ctx.load_module_into_vm(&path)?;
    // Run static-field initializers for the freshly-loaded test module + its
    // dependency closure: their `*.__static_init__` functions weren't present
    // when the VM ran its startup static-init pass (the module is loaded now,
    // mid-run), so e.g. `Std.Math.Pi` would read Null. Re-running init is
    // idempotent for value-init statics; `__load_module` runs once per artifact
    // (before any test executes), so this clears no meaningful test state.
    if let Some(m) = ctx.module().cloned() {
        crate::interp::init_static_fields(ctx, &m)?;
    }
    let mut objs: Vec<Value> = Vec::with_capacity(entries.len());
    for e in &entries {
        let obj = alloc_named(
            ctx,
            "Std.Test.TestEntry",
            &[
                ("Qualified", Value::Str(e.qualified.clone().into())),
                ("Kind", Value::I64(e.kind as i64)),
                ("Flags", Value::I64(e.flags as i64)),
                ("SkipReason", load_module_opt(&e.skip_reason)),
                ("SkipPlatform", load_module_opt(&e.skip_platform)),
                ("SkipFeature", load_module_opt(&e.skip_feature)),
                ("ShouldThrow", load_module_opt(&e.expected_throw)),
            ],
        )?;
        objs.push(obj);
    }
    Ok(ctx.heap().alloc_array(objs))
}

fn load_module_opt(o: &Option<String>) -> Value {
    match o {
        Some(s) => Value::Str(s.clone().into()),
        None => Value::Null,
    }
}

/// `__load_bytecode_in_memory(bytes: byte[]) -> bool` — load a compiled
/// artifact (packed zpkg / bare zbc) from an in-memory byte array into the live
/// VM registries, so its functions become reflectively invocable with zero disk
/// I/O. Backs `z42.scripting`'s per-eval load (REPL): `PackageCompile` emits the
/// session package's bytes in memory, this registers them, then `$Eval_N()` is
/// called via `MethodInfo.Invoke`. Idempotent per module name (first-wins merge,
/// like `__load_module`). Returns `true` on success; a malformed/empty buffer or
/// missing lazy loader throws. (add-z42-repl)
pub fn builtin_load_bytecode_in_memory(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let bytes = match args.first() {
        Some(Value::Array(rc)) => {
            let borrowed = rc.borrow();
            let mut out = Vec::with_capacity(borrowed.len());
            for (i, v) in borrowed.iter().enumerate() {
                match v {
                    Value::I64(n) if (0..=255).contains(n) => out.push(*n as u8),
                    other => bail!(
                        "__load_bytecode_in_memory: byte {} not u8 in 0..=255: {:?}", i, other
                    ),
                }
            }
            out
        }
        Some(other) => bail!(
            "__load_bytecode_in_memory: expected a byte[] argument, got {:?}", other
        ),
        None => bail!("__load_bytecode_in_memory: missing byte[] argument"),
    };
    let static_inits = ctx.load_module_bytes_into_vm(&bytes)?;
    // Run ONLY the freshly-loaded module's own `__static_init__` functions — NOT the
    // full `init_static_fields` (which clears ALL static fields then reruns every
    // module's init). A full clear+rerun would wipe prior REPL rounds' mutated static
    // state (e.g. a `List` a user `.Add`ed to), breaking carry-forward. Running just
    // this round's init sets the new round's `Vars{N}` from the still-live prior round.
    // (add-z42-repl)
    if !static_inits.is_empty() {
        let module_arc = ctx.core.module.as_ref()
            .ok_or_else(|| anyhow::anyhow!("__load_bytecode_in_memory: VmCore.module is None"))?
            .clone();
        let module = module_arc.as_ref();
        for name in &static_inits {
            if let Some(f) = ctx.try_lookup_function(name) {
                match exec_function(ctx, module, f.as_ref(), &[])? {
                    ExecOutcome::Returned(_) => {}
                    ExecOutcome::Thrown(val) => {
                        ctx.set_pending_thrown(val);
                        bail!("__z42_reflected_throw__");
                    }
                }
            }
        }
    }
    Ok(Value::Bool(true))
}

#[cfg(test)]
#[path = "reflection_tests.rs"]
mod reflection_tests;
