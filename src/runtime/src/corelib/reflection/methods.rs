use super::*;
use crate::vm::runtime::Function;

// ── Method reflection ───────────────────────────────────────────────────────

/// `__type_methods(typeObj) -> MethodInfo[]` — vtable (virtual/inherited) plus
/// declared non-virtual methods, deduped by qualified name.
pub fn type_methods(ctx: &VmContext, ty: &Value) -> Result<Value, BuiltinError> {
    let td = match type_handle_of(ty) {
        Some(t) => t,
        None => return Ok(ctx.heap().alloc_array(Vec::new())),
    };
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    // A constructed Type (`typeof(Box<int>)`) reports signatures with its arguments substituted.
    let subst = member_subst(ty);
    let subst = subst.as_ref();
    // Virtual / inherited methods carry their simple name in the vtable.
    for (simple, qualified) in &td.vtable {
        if seen.insert(qualified.clone()) {
            out.push(build_method_info_ex(ctx, simple, qualified, true, false, subst).map_err(BuiltinError::Legacy)?);
        }
    }
    // Declared non-virtual methods (qualified names only). Constructors are not among
    // them — `build_type_registry` keeps them out of `own_methods` (fix-nested-ctor-key).
    for qualified in td.own_methods() {
        let q = qualified.to_string();
        if seen.insert(q.clone()) {
            let simple = simple_method_name(&q).to_string();
            out.push(build_method_info_ex(ctx, &simple, &q, false, false, subst).map_err(BuiltinError::Legacy)?);
        }
    }
    // add-interface-member-reflection: an interface carries its declared method
    // signatures in `iface_methods` (its vtable/own_methods are empty — interface
    // methods have no backing Function). Build MethodInfo straight from the sigs so
    // `typeof(IFoo).GetMethods()` surfaces the interface contract.
    for sig in td.iface_methods() {
        let q = format!("{}.{}", td.name, sig.name);
        if seen.insert(q) {
            out.push(build_iface_method_info(ctx, &td.name, sig).map_err(BuiltinError::Legacy)?);
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
    if td.class_flags & crate::metadata::ir::CLASS_FLAG_INTERFACE != 0 {
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
                        out.push(build_iface_method_info(ctx, &itd.name, sig).map_err(BuiltinError::Legacy)?);
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

/// `__type_constructors(typeObj) -> ConstructorInfo[]` enumerates the declaration's
/// constructor keys from its runtime descriptor, including lazily loaded root packages.
pub fn type_constructors(ctx: &VmContext, ty: &Value) -> Result<Value, BuiltinError> {
    // Only types registered in the executing VM may use its qualified function lookup.
    // A Type from a collectible assembly can share a name with an unrelated root type.
    let td = type_handle_of(ty).and_then(|td| ctx.types().get(td.id));
    let Some(td) = td else { return Ok(ctx.heap().alloc_array(Vec::new())); };

    let subst = member_subst(ty);
    let owner = type_value_name(ty).unwrap_or_else(|| td.name.clone());
    let mut out = Vec::with_capacity(td.own_constructors().len());
    for key in td.own_constructors() {
        out.push(build_ctor_info(ctx, key, &owner, subst.as_ref()).map_err(BuiltinError::Legacy)?);
    }
    Ok(ctx.heap().alloc_array(out))
}

/// Build a `ConstructorInfo` for the constructor function `qualified`
/// (`<ClassFQN>.<SimpleName>[$N]`). Reuses `build_param_infos`; a constructor is never
/// static, has no return type, and its user-facing `Name` is the class simple name.
pub(super) fn build_ctor_info(
    ctx: &VmContext, qualified: &str, owner: &str, subst: Option<&MemberSubst>,
) -> Result<Value> {
    let simple_full = simple_method_name(qualified);
    let simple = simple_full.split('$').next().unwrap_or(simple_full);
    let (params, _is_static, _ret, _vis, _mf, _sig_found) = build_param_infos_ex(ctx, qualified, false, subst, &[])?;
    let params_arr = ctx.heap().alloc_array(params);
    alloc_named(
        ctx,
        STD_REFLECTION_CONSTRUCTORINFO,
        &[
            ("Name", Value::Str(simple.to_string().into())),
            ("IsStatic", Value::Bool(false)),
            ("__parameters", params_arr),
            ("__qualified", Value::Str(qualified.to_string().into())),
            ("__owner", Value::Str(owner.to_string().into())),
        ],
    )
}

/// Build the logical `ParameterInfo[]` (excludes the implicit `this`) for a function,
/// plus its resolved `(is_static, ret_type, visibility, method_flags, sig_found)`.
/// Shared by `build_method_info` and `build_ctor_info`.

/// `receiverless`: the caller knows the function takes no receiver (a free / top-level
/// function — `Assembly.GetFunctions`). Its `Function.is_static` is not set (the flag is
/// written for static *class* methods only), so without this the first parameter would be
/// mistaken for `this` and the MethodInfo would report `IsStatic == false`.
fn build_param_infos_ex(
    ctx: &VmContext,
    qualified: &str,
    receiverless: bool,
    subst: Option<&MemberSubst>,
    shadowed: &[String],
) -> Result<(Vec<Value>, bool, String, u8, u8, bool)> {
    match resolve_reflected_function(ctx, qualified) {
        Some(resolved) => {
            let f = resolved.as_ref();
            let fn_is_static = f.is_static || receiverless;
            // Instance methods / constructors carry `this` at param 0 — skip it.
            let start = if fn_is_static { 0 } else { 1 };
            // PR6: parameter defaults now persist as the `$Default` param attr-ref
            // sentinel (a ConstBlob), superseding the retired SIGS `default_kind` tuple
            // (z42c now writes kind 0 for all params). Read it per physical param; the
            // legacy `param_defaults` match stays as a fallback for zpkgs built by an
            // older z42c (which wrote kinds but no `$Default`).
            let attrs_source = resolve_reflected_function(ctx, qualified);
            let param_attrs = attrs_source.as_ref().map_or(&[][..], |entry| entry.as_ref().param_attributes());
            let param_types = f.param_types();
            let sigs_names = f.param_names();
            let use_debug_names = sigs_names.len() != f.param_count
                || sigs_names.iter().all(String::is_empty);
            let debug_names = use_debug_names.then(|| debug_parameter_names(f));
            let param_defaults = f.param_defaults();
            let mut params = Vec::new();
            for i in start..f.param_count {
                let tag = param_types.get(i).map(|s| s.as_str()).unwrap_or("?");
                let pos = (i - start) as i64;   // logical position (0-based, excludes `this`)
                // Keep the logical position as the fallback name.
                let name = match &debug_names {
                    Some(names) => names.get(i).copied(),
                    None => sigs_names.get(i).map(String::as_str),
                };
                let name = name.filter(|name| !name.is_empty())
                    .map_or_else(|| format!("arg{pos}"), str::to_owned);
                // add-param-metadata: IsOptional / IsParams (logical position) + DefaultValue.
                let is_optional = (pos as u16) >= f.min_arg;
                let is_params = f.params_from != 0xFF && pos as u8 == f.params_from;
                let default_value = param_default_from_blob(param_attrs, i).unwrap_or_else(|| {
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
                        ("ParameterType", signature_type(ctx, tag, qualified, subst, shadowed)),
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
            Ok((params, fn_is_static, f.ret_type.clone(), f.visibility, f.method_flags, true))
        }
        None => Ok((Vec::new(), false, "void".to_string(), 0, 0, false)),
    }
}

/// Build a `MethodInfo` by resolving the backing `Function` for its signature.
/// Missing Function (extern/native or unresolved) → name-only MethodInfo.
pub(crate) fn build_method_info(
    ctx: &VmContext,
    simple: &str,
    qualified: &str,
    is_virtual: bool,
) -> Result<Value> {
    build_method_info_ex(ctx, simple, qualified, is_virtual, false, None)
}

/// MethodInfo for a free (top-level) function: no receiver, `IsStatic == true`.
pub(crate) fn build_free_function_info(ctx: &VmContext, simple: &str, qualified: &str) -> Result<Value> {
    build_method_info_ex(ctx, simple, qualified, false, true, None)
}

fn build_method_info_ex(
    ctx: &VmContext,
    simple: &str,
    qualified: &str,
    is_virtual: bool,
    receiverless: bool,
    subst: Option<&MemberSubst>,
) -> Result<Value> {
    // stabilize-dispatch-keys (方案A): dispatch keys now carry a
    // full `$N$types` mangle suffix for every method (vtable slots + own_methods
    // keys included), so the derived `simple` name may be e.g. `Foo$1$int`.
    // Reflection presents the source-level name — strip the mangle suffix for
    // the user-facing `MethodInfo.Name` (dispatch still uses `qualified`).
    let simple = simple.split('$').next().unwrap_or(simple);
    let type_params_source = resolve_reflected_function(ctx, qualified);
    let type_param_names = type_params_source.as_ref().map_or(&[][..], |entry| entry.as_ref().type_params());
    let (params, is_static, ret_tag, visibility, method_flags, sig_found) =
        build_param_infos_ex(ctx, qualified, receiverless, subst, type_param_names)?;
    // add-method-modifiers (unify P1-c): IsVirtual authoritative from the flag
    // (bit0) WHEN a SIGS entry was resolved — z42 lists every method (virtual or
    // not) in the vtable, so vtable-presence alone over-reports. Fall back to the
    // vtable-presence hint only for methods with no resolvable SIGS (synthesized).
    // IsAbstract from bit1.
    let is_virtual_flag = if sig_found {
        (method_flags & crate::metadata::ir::METHOD_FLAG_VIRTUAL) != 0
    } else {
        is_virtual
    };
    let is_abstract_flag = (method_flags & crate::metadata::ir::METHOD_FLAG_ABSTRACT) != 0;
    // impl-sealed-semantics-devirt: MethodInfo.IsSealed from METHOD_FLAG_SEALED (bit2).
    let is_sealed_flag = (method_flags & crate::metadata::ir::METHOD_FLAG_SEALED) != 0;
    // add-reflective-invoke: method-level generic type parameters (definition state).
    // A freshly-reflected MethodInfo is a generic method *definition* iff it declares
    // type params; MakeGenericMethod produces a *constructed* MethodInfo (sets __typeArgs
    // + clears IsGenericMethodDefinition). __typeParamNames backs GetGenericArguments()
    // for the definition (placeholder Types built from the names).
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
            ("ReturnType", signature_type(ctx, &ret_tag, qualified, subst, type_param_names)),
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
    sig: &crate::metadata::ir::IfaceMethodSig,
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

/// Keep the owning handle alive while reflection projects borrowed metadata.
pub(super) enum ReflectedFunction<'a> {
    Entry(&'a Function),
    Loaded(Arc<Function>),
}

impl AsRef<Function> for ReflectedFunction<'_> {
    fn as_ref(&self) -> &Function {
        match self {
            Self::Entry(function) => function,
            Self::Loaded(function) => function,
        }
    }
}

/// Look in the entry module first, then follow the ordinary lazy lookup path.
pub(super) fn resolve_reflected_function<'a>(
    ctx: &'a VmContext,
    qualified: &str,
) -> Option<ReflectedFunction<'a>> {
    if let Some(module) = ctx.module() {
        if let Some(&index) = module.func_index.get(qualified) {
            if let Some(function) = module.functions.get(index) {
                return Some(ReflectedFunction::Entry(function));
            }
        }
    }
    ctx.try_lookup_function(qualified).map(ReflectedFunction::Loaded)
}

/// With no authoritative SIGS names, use the last DBUG name at each argument register.
fn debug_parameter_names(f: &Function) -> Vec<&str> {
    let mut names = vec![""; f.param_count];
    for local in f.local_vars() {
        if let Some(name) = names.get_mut(local.reg as usize) {
            *name = local.name.as_str();
        }
    }
    names
}

/// PR6: read the `$Default` param attr-ref sentinel (a ConstBlob) for physical
/// parameter `idx` and decode it into a reflection `DefaultValue`. Returns `None`
/// when the param has no `$Default` (→ caller falls back to the legacy kind tuple).
fn param_default_from_blob(
    param_attrs: &[Box<[crate::metadata::ir::AttributeRef]>],
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

/// 成员列举迁 builtin ABI v2(P1-8 批次 5):纯合成(FieldInfo / MethodInfo /
/// ConstructorInfo / PropertyInfo 数组),不执行 z42 代码,不压轻帧。
#[z42_macros::builtins]
pub mod typed {
    use super::*;

    #[builtin(name = "__type_methods")]
    pub fn methods_of(ctx: &VmContext, ty: &Value) -> Result<Value, BuiltinError> {
        type_methods(ctx, ty)
    }

    #[builtin(name = "__type_constructors")]
    pub fn ctors_of(ctx: &VmContext, ty: &Value) -> Result<Value, BuiltinError> {
        type_constructors(ctx, ty)
    }
}
