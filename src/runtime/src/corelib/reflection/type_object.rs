use super::*;
use crate::object::{ReflectedTypeKey, ReflectedTypeShape};

// ── Type-object construction ────────────────────────────────────────────────

/// Build a `Std.Type` object backed by the real `Std.Type` class (so its
/// reflection methods dispatch via the class vtable) and carrying `td` as
/// `NativeData::TypeHandle`. Falls back to a handle-less synthetic only if
/// z42.core's `Std.Type` isn't loaded (shouldn't happen in practice).
pub fn make_type_object(ctx: &VmContext, td: Arc<TypeDesc>) -> Value {
    // An instantiation descriptor (`Demo.Box<int>`, the compiler's own name for a closed
    // instantiation) is a constructed type: same Type as `typeof(Box<int>)` — definition handle
    // plus `__typeArgs`, FullName with arguments (see `make_constructed_type`).
    if !td.type_args().is_empty() && td.name.contains('<') {
        return make_type_from_name(ctx, &td.name);
    }
    let full = td.name.clone();
    let simple = type_simple_name(&full);
    build_type(ctx, &simple, &full, NativeData::TypeHandle(td))
}

/// Simple (unqualified) type name for `Type.Name`: strip the namespace (`.`)
/// then, for nested types, the declaring-type prefix (`+`) — so
/// `Nest.Outer+Inner` → `Inner`, mirroring C#. add-nested-types.
/// Instantiation args are FQ (`Demo.Pair<Demo.A.P>`) and get the same treatment → `Pair<P>`.
pub fn type_simple_name(full: &str) -> String {
    crate::metadata::index::type_names::simple(full)
}

/// Split a generic arg list on top-level commas, respecting nested `<>` / `[]`
/// so `Box<int>,string` → `["Box<int>", "string"]` (not split inside the inner
/// `<>`). add-reflection-nested-generic-args.
pub(super) fn split_generic_args(inner: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0usize;
    // add-collection-serde: TRIM each arg. Field/member type_tags use source spelling
    // with a space after commas (`Dictionary<string, int>` — see z42c
    // ClassDescBuilder `_typeSourceName`'s `", "`), so a raw split yields `" int"` with
    // a leading space → make_type_from_name misses → synthetic (no handle). typeof names
    // have no space, so this only bit member-type reflection of multi-arg generics.
    for (i, c) in inner.char_indices() {
        match c {
            '<' | '[' => depth += 1,
            '>' | ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(inner[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(inner[start..].trim().to_string());
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
        // fix-value-type-object-methods ④: array Type carries **full element-path**
        // names — Name = "{elemName}[]", FullName = "{elemFullName}[]" (was the
        // element-erased synthetic "Array"/"Std.Array"). The element Type is resolved
        // recursively so `int[]` → "Int32[]" / "Std.Int32[]" (element name via change A's
        // int→Std.Int32) and `int[][]` → "Int32[][]" / "Std.Int32[][]".
        // Keep the element used to compose these names.
        // Element-less arrays (empty tag) keep the old synthetic name (no element to name).
        if elem.is_empty() {
            return build_type_ex(
                ctx, "Array", well_known_names::STD_ARRAY,
                type_shape(None, ReflectedTypeKey::Array(None)),
                TypeProjection::Array { element_name: elem, element: Value::Null },
            );
        }
        let element = make_type_from_name(ctx, elem);
        return make_array_type(ctx, elem, element);
    }
    // add-reflection-nested-generic-args: a constructed-generic arg name carries angle
    // brackets (`Box<Pair<int,string>>` — z42c `_typeofArgName` emits the full nested
    // name). Parse base + top-level args (bracket-depth aware) and build a constructed
    // Type via `make_constructed_type`, whose per-arg resolution re-enters here → nested
    // generics resolve to arbitrary depth (each arg keeps its own `__typeArgs`).
    // fix-emit-zbc-swallows-diagnostics: only treat this as a constructed generic when
    // there is a non-empty base before the `<`. A name that *starts* with `<` is not a
    // generic — it is an internal sentinel that leaked out of the compiler (the concrete
    // case was `<unknown>`, z42c's spelling for an unresolved type). Parsing it as a
    // generic yielded base "" → a `Type` with empty Name/FullName, zero methods and a
    // null BaseType, i.e. a broken object silently presented as a valid one. Fall through
    // instead, so the name takes the normal "unresolved" path below.
    if let Some(lt) = name.find('<') {
        if lt > 0 && name.ends_with('>') {
            let base = &name[..lt];
            let inner = &name[lt + 1..name.len() - 1];
            let args = split_generic_args(inner);
            return make_constructed_type(ctx, base, &args);
        }
    }
    if let Some(td) = resolve_named_type(ctx, name) {
        return make_type_object(ctx, td);
    }
    let canon = canonical_type_name(name);
    let simple = type_simple_name(&canon);
    let native = type_shape(None, ReflectedTypeKey::Symbol(canon.as_str().into()));
    build_type(ctx, &simple, &canon, native)
}

pub(super) fn make_array_type(ctx: &VmContext, element_name: &str, element: Value) -> Value {
    let name = match read_type_slot_of(&element, "__name") {
        Value::Str(s) => s.to_string(),
        _ => element_name.to_string(),
    };
    let full = match read_type_slot_of(&element, "__fullName") {
        Value::Str(s) => s.to_string(),
        _ => element_name.to_string(),
    };
    let native = type_shape(None, ReflectedTypeKey::Array(reflected_type_key(&element).map(Arc::new)));
    build_type_ex(
        ctx, &format!("{name}[]"), &format!("{full}[]"), native,
        TypeProjection::Array { element_name, element },
    )
}

fn resolve_named_type(ctx: &VmContext, name: &str) -> Option<Arc<TypeDesc>> {
    // Main module's own types first: the user program's classes live in the
    // main module's `type_registry`; the lazy loader below only covers
    // zpkg / stdlib types. (make-typeof-return-type — lets `typeof(UserClass)`
    // resolve to a real handle.)
    if let Some(m) = ctx.module() {
        if let Some(td) = m.type_registry.get(name) {
            return Some(td.clone());
        }
    }
    if let Some(td) = ctx.try_lookup_type(name) {
        return Some(td);
    }
    // An *unqualified* class name (no `.`) that failed the FQ lookups above.
    // Two sources:
    //  (a) add-json-serde: `typeof(T)` in a cross-package generic method emits the
    //      type-arg's *simple* name (imported null-receiver vcall path).
    //  (b) add-collection-serde: a constructed-generic field/member type_tag carries
    //      *source spelling* (`List<int>` field → base `List`), not the FQN.
    // Resolve by unique simple-name match against loaded types (never mis-binds:
    // zero/ambiguous → synthetic, unchanged behaviour).
    if !name.contains('.') {
        if let Some(td) = resolve_dotless_simple(ctx, name) {
            return Some(td);
        }
        // precise-pkg-refs: a statically referenced type's short name maps to its full name
        // (and thus its defining package) through the DEPS symbol lists — load just that.
        if let Some(full) = ctx.referenced_full_name(name) {
            if let Some(td) = ctx.try_lookup_type(&full) {
                return Some(td);
            }
        }
        // Miss: a class-like name (uppercase, not a primitive alias) may live in a
        // not-yet-loaded package whose FQN we can't derive (the loader indexes
        // namespaces, not type names — no simple→FQN map). Force-load all remaining
        // packages ONCE, then retry. Gated on uppercase-first so primitives
        // (`int`/`bool`/`string`…) never trigger the eager load. After the first
        // force-load, `remaining_declared()` is empty → repeat calls are cheap.
        if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
            ctx.force_load_all_packages();
            if let Some(td) = resolve_dotless_simple(ctx, name) {
                return Some(td);
            }
        }
    }
    // fix-type-reflection-names: a primitive keyword / tag (`int` / `i32` / …)
    // resolves to its real `Std.*` wrapper struct handle — so `typeof(int)` is the
    // *same* Type as `(5).GetType()` (C# `typeof(int) == 5.GetType()`): Name
    // `Int32`, FullName `Std.Int32`, IsValueType true, members enumerable. The VM
    // uses two tag vocabularies — field slots carry `"int"`/`"long"`, function
    // signatures carry `"i32"`/`"i64"`/`"str"` — `primitive_fqn` maps both to the
    // one FQN. (Handle-carrying, unlike the former handle-less alias synthetic.)
    if let Some(fqn) = primitive_fqn(name) {
        if let Some(td) = ctx.try_lookup_type(fqn) {
            return Some(td);
        }
    }
    None
}

/// Map any built-in type spelling — keyword (`int`), VM tag (`i32`) or FQN — to its `Std.*`
/// struct / class name. `None` for non-built-in names (user/class names, arrays, generics —
/// handled earlier in `make_type_from_name`). One table with the canonical type-name spelling
/// (`type_names::builtin_fqn`), so `typeof(object)` and `new object().GetType()` agree.
pub(super) fn primitive_fqn(name: &str) -> Option<&'static str> {
    crate::metadata::index::type_names::builtin_fqn(name)
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
    let arg_types = type_args.iter().map(|a| make_type_from_name(ctx, a)).collect();
    make_constructed_type_from_values(ctx, type_name, arg_types)
}

/// Compose a Type from arguments that are already resolved. Their handles may
/// refer to types outside the executing VM's name registry.
pub(super) fn make_constructed_type_from_values(
    ctx: &VmContext, type_name: &str, arg_types: Vec<Value>,
) -> Value {
    let definition = resolve_named_type(ctx, type_name);
    let base_full = definition.as_ref().map_or_else(
        || canonical_type_name(type_name),
        |td| td.name.clone(),
    );
    build_constructed_type(ctx, definition, base_full, arg_types)
}

pub(super) fn build_constructed_type(
    ctx: &VmContext, definition: Option<Arc<TypeDesc>>, base_full: String, arg_types: Vec<Value>,
) -> Value {
    let simple = type_simple_name(&base_full);
    let definition_key = definition.as_ref().map_or_else(
        || ReflectedTypeKey::Symbol(base_full.as_str().into()),
        |td| declaration_key(td),
    );
    let arguments = arg_types.iter().map(|ty| {
        reflected_type_key(ty).unwrap_or_else(|| ReflectedTypeKey::Symbol("".into()))
    }).collect::<Vec<_>>();
    let arg_names = arg_types.iter().map(|ty| {
        match read_type_slot_of(ty, "__fullName") {
            Value::Str(name) => name.to_string(),
            _ => String::new(),
        }
    }).collect::<Vec<_>>();
    let full = format!("{base_full}<{}>", arg_names.join(","));
    let native = type_shape(definition, ReflectedTypeKey::Constructed {
        definition: Arc::new(definition_key),
        arguments: arguments.into(),
    });
    build_type_ex(ctx, &simple, &full, native, TypeProjection::Generic(arg_types))
}

fn type_shape(definition: Option<Arc<TypeDesc>>, key: ReflectedTypeKey) -> NativeData {
    NativeData::TypeShape(Arc::new(ReflectedTypeShape { definition, key }))
}

fn declaration_key(td: &TypeDesc) -> ReflectedTypeKey {
    use crate::vm::runtime::ids::tokens::TypeId;
    if td.id == TypeId::UNRESOLVED {
        return ReflectedTypeKey::Symbol(td.name.as_str().into());
    }
    ReflectedTypeKey::Definition(td.id)
}

pub(super) fn reflected_type_key(ty: &Value) -> Option<ReflectedTypeKey> {
    let Value::Object(rc) = ty else { return None };
    match rc.borrow().native() {
        NativeData::TypeHandle(td) => Some(declaration_key(td)),
        NativeData::TypeShape(shape) => Some(shape.key.clone()),
        _ => None,
    }
}

/// Normalize a VM primitive type tag to its C#-style alias. User/class names
/// (anything not a known primitive tag) pass through unchanged.
pub(super) fn canonical_type_name(tag: &str) -> String {
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
pub(super) fn build_type(ctx: &VmContext, simple: &str, full: &str, native: NativeData) -> Value {
    build_type_ex(ctx, simple, full, native, TypeProjection::Plain)
}

enum TypeProjection<'a> {
    Plain,
    Array { element_name: &'a str, element: Value },
    Generic(Vec<Value>),
}

fn build_type_ex(
    ctx: &VmContext, simple: &str, full: &str, native: NativeData, projection: TypeProjection<'_>,
) -> Value {
    let related_type = match &projection {
        TypeProjection::Array { element, .. } => Some(element),
        TypeProjection::Generic(arguments) => arguments.first(),
        TypeProjection::Plain => None,
    };
    let type_td = match related_type {
        // Reuse the Type descriptor without a lazy lookup while fresh
        // reflection objects are held in native locals.
        Some(Value::Object(rc)) if rc.type_desc().name == well_known_names::STD_TYPE =>
            Some(rc.type_desc_arc().clone()),
        _ => ctx.try_lookup_type(well_known_names::STD_TYPE),
    };
    match type_td {
        Some(type_td) => {
            let mut slots = vec![Value::Null; type_td.fields.len()];
            if let Some(&i) = type_td.field_index.get("IsArray") {
                slots[i] = Value::Bool(matches!(projection, TypeProjection::Array { .. }));
            }
            if let Some(&i) = type_td.field_index.get("__elementName") {
                let name = match &projection {
                    TypeProjection::Array { element_name, .. } => *element_name,
                    _ => "",
                };
                slots[i] = Value::Str(name.into());
            }
            match projection {
                TypeProjection::Array { element, .. } => {
                    if let Some(&i) = type_td.field_index.get("__elementType") {
                        slots[i] = element;
                    }
                }
                TypeProjection::Generic(arguments) => {
                    if let Some(&i) = type_td.field_index.get("__typeArgs") {
                        slots[i] = ctx.heap().alloc_array(arguments);
                    }
                }
                TypeProjection::Plain => {}
            }
            // align-type-memberinfo-hierarchy: `Name` is inherited from
            // `Std.Reflection.MemberInfo` (Type's base) — populate that slot so
            // `typeof(C).Name` / `(MemberInfo)typeof(C)).Name` resolve via the
            // shared base field (same as FieldInfo/MethodInfo). `__name` retained
            // for low-level golden / z42.harness direct reads.
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
/// Resolve a dotless simple type name to a loaded `TypeDesc` by *unique*
/// simple-name match across loaded types (entry module + lazy-loaded). Ambiguous
/// (>1 match) or zero → `None`, so callers fall through to a name-only synthetic
/// and never mis-bind. Shared by `make_type_from_name`'s add-json-serde /
/// add-collection-serde dotless fallback (with an optional force-load retry).
pub(super) fn resolve_dotless_simple(ctx: &VmContext, name: &str) -> Option<Arc<TypeDesc>> {
    let fq = ctx.unique_loaded_full_name(name)?;
    if let Some(m) = ctx.module() {
        if let Some(td) = m.type_registry.get(&fq) {
            return Some(td.clone());
        }
    }
    ctx.try_lookup_type(&fq)
}

/// The canonical runtime name (`type_names::canonical`) of the type a `Std.Type` stands for:
/// `typeof(List<int>)` → `Std.Collections.List<int>`, `typeof(int)` → `Std.Int32`. Derived from
/// `FullName`, which is always the display form of that same name — so a Type built from any
/// spelling (reflection, `typeof`, `GetType`) answers with the one the VM compares by.
pub(crate) fn type_value_name(ty: &Value) -> Option<String> {
    match read_type_slot_of(ty, "__fullName") {
        Value::Str(s) if !s.is_empty() => Some(crate::metadata::index::type_names::canonical(&s)),
        _ => None,
    }
}

/// [`type_value_name`] spelled as a **type argument** (`typeof(int)` → `int`) — what an
/// instance's `type_args` and a frame's `method_type_args` hold.
pub(crate) fn type_value_arg_name(ty: &Value) -> Option<String> {
    type_value_name(ty).map(|n| crate::metadata::index::type_names::canonical_arg(&n))
}

/// The instantiation descriptor of a constructed `Std.Type` (`typeof(Sub<int>)` → the
/// descriptor registered as `Demo.Sub<int>`), when a loaded package carries one. A constructed
/// Type's handle is its generic *definition* (members, constructors and the definition itself
/// are read from it); facts that differ per instantiation — the substituted base class and
/// interfaces — live on this descriptor. `None` for a non-constructed Type, or an
/// instantiation no loaded package declared (an imported generic the main package instantiated:
/// its definition's base / interface names are all there is).
pub(super) fn instantiation_desc(ctx: &VmContext, ty: &Value) -> Option<Arc<TypeDesc>> {
    if !matches!(read_type_slot_of(ty, "__typeArgs"), Value::Array(_)) {
        return None;
    }
    let name = type_value_name(ty)?;
    ctx.module()
        .and_then(|m| m.type_registry.get(&name).cloned())
        .or_else(|| ctx.try_lookup_type(&name))
}

pub(super) fn type_handle(args: &[Value]) -> Option<Arc<TypeDesc>> {
    args.first().and_then(type_handle_of)
}

/// [`type_handle`] for a single receiver — the typed-builtin shape (P1-8 批次 1)。
pub(super) fn type_handle_of(ty: &Value) -> Option<Arc<TypeDesc>> {
    match ty {
        Value::Object(rc) => {
            let obj = rc.borrow();
            match &obj.native() {
                NativeData::TypeHandle(td) => Some(td.clone()),
                NativeData::TypeShape(shape) => shape.definition.clone(),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Allocate a z42 class instance (`type_name`), writing the given named slots by
/// `field_index`. Unlisted slots stay `Null`. Errors only if the class isn't
/// loaded (a hard environment bug, not user-reachable).
pub(super) fn alloc_named(ctx: &VmContext, type_name: &str, named: &[(&str, Value)]) -> Result<Value> {
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
pub(super) fn simple_method_name(qualified: &str) -> &str {
    qualified.rsplit('.').next().unwrap_or(qualified)
}

/// Read a string slot from a `Std.Type` object by `field_index`. Backs the
/// `Name` / `FullName` extern properties (both handle-carrying and synthetic
/// Types have these slots written by `build_type`).
pub(super) fn read_type_str_slot(args: &[Value], field: &str) -> Value {
    match args.first() {
        Some(ty) => read_type_slot_of(ty, field),
        None => Value::Null,
    }
}

/// [`read_type_str_slot`] for a single receiver — the typed-builtin shape (P1-8 批次 1)。
pub(super) fn read_type_slot_of(ty: &Value, field: &str) -> Value {
    if let Value::Object(rc) = ty {
        if let Some(i) = rc.type_desc().field_index.get(field).copied() {
            let obj = rc.borrow();
            return obj.field_value(i);
        }
    }
    Value::Null
}

// align-type-memberinfo-hierarchy : `__type_name` / `builtin_type_name`
// removed — `Type.Name` now resolves to the inherited `Std.Reflection.MemberInfo`
// `Name` field (populated by `build_type`), no native getter.
