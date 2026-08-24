use super::*;

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

/// `__type_constructors(typeObj) -> ConstructorInfo[]` (add-reflective-invoke).
/// Constructors are ordinary functions named like the class (`<ClassFQN>.<SimpleName>`,
/// with a `$N$types` overload-mangle suffix), living in the module's `func_index`
/// (NOT in the type's `own_methods`/vtable — a single non-overloaded constructor is
/// absent there). Scan `func_index` for keys whose segment after `<ClassFQN>.` and
/// before the first `$` equals the class simple name; dedup by function index (a
/// constructor may be registered under bare + mangled dispatch keys), sorted for a
/// deterministic order.
pub fn builtin_type_constructors(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let td = match type_handle(args) {
        Some(t) => t,
        None => return Ok(ctx.heap().alloc_array(Vec::new())),
    };
    let class_simple = td.name.rsplit('.').next().unwrap_or(td.name.as_str());
    let module_arc = match ctx.core.module.as_ref() {
        Some(m) => m.clone(),
        None => return Ok(ctx.heap().alloc_array(Vec::new())),
    };
    let module = module_arc.as_ref();
    let prefix = format!("{}.", td.name);
    // Collect (key, func-index) for constructor-named entries, sorted by key for a
    // stable order (common-pitfalls §1: never rely on HashMap iteration order).
    let mut ctor_keys: Vec<(&String, usize)> = module
        .func_index
        .iter()
        .filter(|(k, _)| {
            k.strip_prefix(&prefix)
                .and_then(|rest| rest.split('$').next())
                == Some(class_simple)
        })
        .map(|(k, &idx)| (k, idx))
        .collect();
    ctor_keys.sort_by(|a, b| a.0.cmp(b.0));
    let mut seen_idx: HashSet<usize> = HashSet::new();
    let mut out = Vec::new();
    for (key, idx) in ctor_keys {
        if seen_idx.insert(idx) {
            out.push(build_ctor_info(ctx, key)?);
        }
    }
    Ok(ctx.heap().alloc_array(out))
}

/// Build a `ConstructorInfo` for the constructor function `qualified`
/// (`<ClassFQN>.<SimpleName>[$N]`). Reuses `build_param_infos`; a constructor is never
/// static, has no return type, and its user-facing `Name` is the class simple name.
pub(super) fn build_ctor_info(ctx: &VmContext, qualified: &str) -> Result<Value> {
    let simple_full = simple_method_name(qualified);
    let simple = simple_full.split('$').next().unwrap_or(simple_full);
    let (params, _is_static, _ret, _vis, _mf, _sig_found) = build_param_infos(ctx, qualified)?;
    let params_arr = ctx.heap().alloc_array(params);
    alloc_named(
        ctx,
        STD_REFLECTION_CONSTRUCTORINFO,
        &[
            ("Name", Value::Str(simple.to_string().into())),
            ("IsStatic", Value::Bool(false)),
            ("__parameters", params_arr),
            ("__qualified", Value::Str(qualified.to_string().into())),
        ],
    )
}

/// Build the logical `ParameterInfo[]` (excludes the implicit `this`) for a function,
/// plus its resolved `(is_static, ret_type, visibility, method_flags, sig_found)`.
/// Shared by `build_method_info` and `build_ctor_info`.
pub(super) fn build_param_infos(
    ctx: &VmContext,
    qualified: &str,
) -> Result<(Vec<Value>, bool, String, u8, u8, bool)> {
    match resolve_func_sig(ctx, qualified) {
        Some((param_count, ret_type, fn_is_static, param_types, param_names, vis, mf, min_arg, params_from, param_defaults)) => {
            // Instance methods / constructors carry `this` at param 0 — skip it.
            let start = if fn_is_static { 0 } else { 1 };
            // PR6: parameter defaults now persist as the `$Default` param attr-ref
            // sentinel (a ConstBlob), superseding the retired SIGS `default_kind` tuple
            // (z42c now writes kind 0 for all params). Read it per physical param; the
            // legacy `param_defaults` match stays as a fallback for zpkgs built by an
            // older z42c (which wrote kinds but no `$Default`).
            let param_attrs = resolve_func_param_attrs(ctx, qualified);
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
                let default_value = param_default_from_blob(&param_attrs, i).unwrap_or_else(|| {
                    // Fallback: legacy SIGS default_kind tuple (only nonzero for zpkgs
                    // built by a pre-PR6 z42c; current z42c writes kind 0 everywhere).
                    match param_defaults.get(i) {
                        Some((1, _, _)) => Value::Null,
                        Some((2, iv, _)) => Value::I64(*iv),
                        Some((3, iv, _)) => Value::F64(f64::from_bits(*iv as u64)),
                        Some((4, iv, _)) => Value::Bool(*iv != 0),
                        Some((5, _, sv)) => Value::Str(sv.clone().into()),
                        _ => Value::Null, // kind 0 = no (foldable) default
                    }
                });
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
            Ok((params, fn_is_static, ret_type, vis, mf, true))
        }
        None => Ok((Vec::new(), false, "void".to_string(), 0, 0, false)),
    }
}

/// Build a `MethodInfo` by resolving the backing `Function` for its signature.
/// Missing Function (extern/native or unresolved) → name-only MethodInfo.
pub(super) fn build_method_info(
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
    let (params, is_static, ret_tag, visibility, method_flags, sig_found) =
        build_param_infos(ctx, qualified)?;
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
    // impl-sealed-semantics-devirt: MethodInfo.IsSealed from METHOD_FLAG_SEALED (bit2).
    let is_sealed_flag = (method_flags & crate::metadata::bytecode::METHOD_FLAG_SEALED) != 0;
    // add-reflective-invoke: method-level generic type parameters (definition state).
    // A freshly-reflected MethodInfo is a generic method *definition* iff it declares
    // type params; MakeGenericMethod produces a *constructed* MethodInfo (sets __typeArgs
    // + clears IsGenericMethodDefinition). __typeParamNames backs GetGenericArguments()
    // for the definition (placeholder Types built from the names).
    let type_param_names = resolve_func_type_params(ctx, qualified);
    let is_generic_method = !type_param_names.is_empty();
    let tp_name_values: Vec<Value> = type_param_names
        .iter()
        .map(|n| Value::Str(n.clone().into()))
        .collect();
    let tp_names_arr = ctx.heap().alloc_array(tp_name_values);
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
            // impl-sealed-semantics-devirt: sealed methods (bit2).
            ("IsSealed", Value::Bool(is_sealed_flag)),
            // add-member-visibility (unify P1-b): 0=public / 1=private /
            // 2=protected. `protected` reports neither (mirrors C# IsFamily).
            ("IsPublic", Value::Bool(visibility == 0)),
            ("IsPrivate", Value::Bool(visibility == 1)),
            // add-reflective-invoke: generic-method reflection (definition state).
            ("IsGenericMethod", Value::Bool(is_generic_method)),
            ("IsGenericMethodDefinition", Value::Bool(is_generic_method)),
            ("__typeParamNames", tp_names_arr),
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
pub(super) fn build_iface_method_info(
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
            // impl-sealed-semantics-devirt: interface methods are never sealed.
            ("IsSealed", Value::Bool(false)),
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
pub(super) fn resolve_func_sig(
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

/// PR6 (param-default-representation): the backing function's per-parameter
/// attribute-ref lists (physical param order, incl. `this`). Carries the `$Default`
/// ConstBlob sentinel read by `param_default_from_blob`. Kept separate from the
/// 10-tuple `resolve_func_sig` to avoid bloating a return shared by two callers.
fn resolve_func_param_attrs(
    ctx: &VmContext,
    qualified: &str,
) -> Vec<Box<[crate::metadata::bytecode::AttributeRef]>> {
    if let Some(m) = ctx.module() {
        if let Some(&i) = m.func_index.get(qualified) {
            if let Some(f) = m.functions.get(i) {
                return f.param_attributes().to_vec();
            }
        }
    }
    ctx.try_lookup_function(qualified)
        .map(|f| f.param_attributes().to_vec())
        .unwrap_or_default()
}

/// PR6: read the `$Default` param attr-ref sentinel (a ConstBlob) for physical
/// parameter `idx` and decode it into a reflection `DefaultValue`. Returns `None`
/// when the param has no `$Default` (→ caller falls back to the legacy kind tuple).
fn param_default_from_blob(
    param_attrs: &[Box<[crate::metadata::bytecode::AttributeRef]>],
    idx: usize,
) -> Option<Value> {
    let attrs = param_attrs.get(idx)?;
    attrs
        .iter()
        .find(|a| a.type_name == "$Default")
        .and_then(|a| decode_const_blob_scalar(&a.factory_func))
}

/// Decode a scalar `ConstBlob` (mirrors the z42 `ConstBlob` encoder in
/// `ConstBlob.z42`) into a reflection value. Scalars `n`/`b`/`i`/`c`/`f`/`s` are
/// materialized; aggregates `e` (enum) / `a` (array) / `t` (struct) reflect as
/// `Null` (Deferred — only scalars are surfaced). Malformed → `None`.
fn decode_const_blob_scalar(blob: &str) -> Option<Value> {
    // Char-indexed to match z42's char-based `_seg` length prefix.
    let chars: Vec<char> = blob.chars().collect();
    if chars.is_empty() {
        return None;
    }
    let mut pos = 0usize;
    let tag = chars[pos];
    pos += 1;
    // Read a `<len>:<content>` segment (len = char count of content).
    fn read_seg(chars: &[char], pos: &mut usize) -> Option<String> {
        let mut colon = *pos;
        while colon < chars.len() && chars[colon] != ':' {
            colon += 1;
        }
        if colon >= chars.len() {
            return None;
        }
        let len: usize = chars[*pos..colon].iter().collect::<String>().parse().ok()?;
        *pos = colon + 1;
        if *pos + len > chars.len() {
            return None;
        }
        let content: String = chars[*pos..*pos + len].iter().collect();
        *pos += len;
        Some(content)
    }
    match tag {
        'n' => Some(Value::Null),
        'b' => chars.get(pos).map(|c| Value::Bool(*c == '1')),
        'i' => read_seg(&chars, &mut pos)
            .and_then(|s| s.parse::<i64>().ok())
            .map(Value::I64),
        'c' => read_seg(&chars, &mut pos)
            .and_then(|s| s.parse::<u32>().ok())
            .and_then(char::from_u32)
            .map(Value::Char),
        'f' => read_seg(&chars, &mut pos)
            .and_then(|s| s.parse::<f64>().ok())
            .map(Value::F64),
        's' => read_seg(&chars, &mut pos).map(|s| Value::Str(s.into())),
        // Aggregates: present but Deferred → Null (don't fall through to kind).
        'e' | 'a' | 't' => Some(Value::Null),
        _ => None,
    }
}

/// add-reflective-invoke: the backing function's **method-level** generic type
/// parameter names (e.g. `["T", "U"]` for `Foo<T, U>()`; empty for non-generic).
/// Read from `Function.type_params()` (populated from SIGS via FunctionCold).
/// Kept separate from the 10-tuple `resolve_func_sig` to avoid bloating a shared
/// return used by two callers.
pub(super) fn resolve_func_type_params(ctx: &VmContext, qualified: &str) -> Vec<String> {
    if let Some(m) = ctx.module() {
        if let Some(&i) = m.func_index.get(qualified) {
            if let Some(f) = m.functions.get(i) {
                return f.type_params().to_vec();
            }
        }
    }
    ctx.try_lookup_function(qualified)
        .map(|f| f.type_params().to_vec())
        .unwrap_or_default()
}
