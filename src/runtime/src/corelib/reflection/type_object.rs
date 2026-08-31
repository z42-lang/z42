use super::*;

// ── Type-object construction ────────────────────────────────────────────────

/// Build a `Std.Type` object backed by the real `Std.Type` class (so its
/// reflection methods dispatch via the class vtable) and carrying `td` as
/// `NativeData::TypeHandle`. Falls back to a handle-less synthetic only if
/// z42.core's `Std.Type` isn't loaded (shouldn't happen in practice).
pub fn make_type_object(ctx: &VmContext, td: Arc<TypeDesc>) -> Value {
    let full = td.name.clone();
    let simple = type_simple_name(&full).to_string();
    build_type(ctx, &simple, &full, NativeData::TypeHandle(td))
}

/// Simple (unqualified) type name for `Type.Name`: strip the namespace (`.`)
/// then, for nested types, the declaring-type prefix (`+`) — so
/// `Nest.Outer+Inner` → `Inner`, mirroring C#. add-nested-types.
pub fn type_simple_name(full: &str) -> &str {
    let after_dot = full.rsplit('.').next().unwrap_or(full);
    after_dot.rsplit('+').next().unwrap_or(after_dot)
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
            return make_type_object(ctx, td);
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
                return make_type_object(ctx, td);
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
            return make_type_object(ctx, td);
        }
    }
    // Degraded fallback: z42.core not loaded (no `Std.Int32` to resolve) or a truly
    // unresolved name → a handle-less synthetic carrying a canonical user-facing name.
    let canon = canonical_type_name(name);
    let simple = type_simple_name(&canon).to_string();
    build_type(ctx, &simple, &canon, NativeData::None)
}

/// Map a primitive keyword (`int`) *or* VM tag (`i32`) to its fully-qualified
/// `Std.*` wrapper struct name. `None` for non-primitive names (user/class names,
/// arrays, generics — handled earlier in `make_type_from_name`).
///
/// fix-type-reflection-names: reflection resolves primitives to real handles, so
/// `typeof(int)` ≡ `(5).GetType()`. Covers both vocabularies (field-slot keywords
/// and function-signature tags) — the sibling of `canonical_type_name`, which maps
/// the same inputs to display aliases; this maps them to FQNs.
pub(super) fn primitive_fqn(name: &str) -> Option<&'static str> {
    use crate::metadata::well_known_names::*;
    Some(match name {
        "sbyte" | "i8" => "Std.SByte",
        "byte" | "u8" => "Std.Byte",
        "short" | "i16" => "Std.Int16",
        "ushort" | "u16" => "Std.UInt16",
        "int" | "i32" => STD_INT32,
        "uint" | "u32" => "Std.UInt32",
        "long" | "i64" => STD_INT64,
        "ulong" | "u64" => "Std.UInt64",
        "float" | "f32" => STD_SINGLE,
        "double" | "f64" => STD_DOUBLE,
        "bool" => STD_BOOLEAN,
        "char" => STD_CHAR,
        "string" | "str" => STD_STRING,
        _ => return None,
    })
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
    // fix-type-reflection-names: compose the display FullName with args, e.g.
    // `Std.Collections.List<Std.Int32>`. Each arg's FullName is read off its
    // resolved Type (already `Std.Int32` post primitive-handle resolution); nested
    // generics compose recursively (each arg is itself a constructed Type carrying
    // its own `<…>`). Read arg FullNames before allocating the array (borrow order).
    let arg_fulls: Vec<String> = arg_types
        .iter()
        .map(|t| match read_type_str_slot(std::slice::from_ref(t), "__fullName") {
            Value::Str(s) => s.to_string(),
            _ => String::new(),
        })
        .collect();
    let args_array = ctx.heap().alloc_array(arg_types);
    let base = make_type_from_name(ctx, type_name);
    if let Value::Object(rc) = &base {
        let ti = rc.type_desc().field_index.get("__typeArgs").copied();
        let fi = rc.type_desc().field_index.get("__fullName").copied();
        let base_full = match read_type_str_slot(std::slice::from_ref(&base), "__fullName") {
            Value::Str(s) => s.to_string(),
            _ => type_name.to_string(),
        };
        let composed = format!("{}<{}>", base_full, arg_fulls.join(","));
        let mut obj = rc.borrow_mut();
        if let Some(i) = ti {
            obj.set_field_value(i, &args_array);
        }
        if let Some(i) = fi {
            obj.set_field_value(i, &Value::Str(composed.into()));
        }
    }
    base
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
    build_type_ex(ctx, simple, full, native, false, "")
}

/// add-reflection-array-element-type: like `build_type`, but also records whether
/// this is an array type and (if so) its element type FQ name, written to the
/// `Std.Type` `IsArray` / `__elementName` slots (VM-written, same mechanism as
/// `__name` / `__fullName`). `GetElementType()` reads `__elementName` lazily.
pub(super) fn build_type_ex(
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
/// Resolve a dotless simple type name to a loaded `TypeDesc` by *unique*
/// simple-name match across loaded types (entry module + lazy-loaded). Ambiguous
/// (>1 match) or zero → `None`, so callers fall through to a name-only synthetic
/// and never mis-bind. Shared by `make_type_from_name`'s add-json-serde /
/// add-collection-serde dotless fallback (with an optional force-load retry).
pub(super) fn resolve_dotless_simple(ctx: &VmContext, name: &str) -> Option<Arc<TypeDesc>> {
    let mut hit: Option<String> = None;
    for key in ctx.loaded_type_names() {
        if type_simple_name(&key) == name {
            if hit.is_some() {
                return None; // ambiguous
            }
            hit = Some(key);
        }
    }
    let fq = hit?;
    if let Some(m) = ctx.module() {
        if let Some(td) = m.type_registry.get(&fq) {
            return Some(td.clone());
        }
    }
    ctx.try_lookup_type(&fq)
}

pub(super) fn type_handle(args: &[Value]) -> Option<Arc<TypeDesc>> {
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
    if let Some(Value::Object(rc)) = args.first() {
        // `type_desc()` is the lockless accessor on the GcRef; slots come from
        // the locked guard.
        if let Some(i) = rc.type_desc().field_index.get(field).copied() {
            let obj = rc.borrow();
            return obj.field_value(i);
        }
    }
    Value::Null
}

// align-type-memberinfo-hierarchy (2026-06-11): `__type_name` / `builtin_type_name`
// removed — `Type.Name` now resolves to the inherited `Std.Reflection.MemberInfo`
// `Name` field (populated by `build_type`), no native getter.
