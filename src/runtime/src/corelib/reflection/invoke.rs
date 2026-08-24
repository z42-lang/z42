use super::*;

// ── Reflective invocation (add-method-invoke-non-generic, 0.3.12) ────────────

/// Read a named slot from any ScriptObject `Value` (e.g. a `MethodInfo`'s hidden
/// `__qualified` / `IsStatic`). `Null` if not an object or no such field.
pub(crate) fn read_obj_slot(v: &Value, field: &str) -> Value {
    if let Value::Object(rc) = v {
        if let Some(i) = rc.type_desc().field_index.get(field).copied() {
            return rc.borrow().field_value(i);
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
        for e in rc.borrow().iter_boxed() {
            call_args.push(e.clone());
        }
    }

    // add-reflective-invoke: a *constructed* generic MethodInfo carries `__typeArgs`
    // (Type[] bound by MakeGenericMethod). Convert each to its FQ name string and
    // thread into the callee frame's `method_type_args` slot so the body materializes
    // typeof(T)/new T()/default(T) exactly as a direct `Foo<T>()` call. Definition-state
    // or non-generic MethodInfo → empty → byte-identical to the prior non-generic path.
    let method_type_args = read_type_arg_names(&mi);
    invoke_qualified(ctx, &qualified, &call_args, &method_type_args)
}

/// add-reflective-invoke: read a constructed generic MethodInfo's `__typeArgs`
/// (Type[]) as FQ type-name strings for `frame.method_type_args`. Empty when the
/// MethodInfo is a definition or non-generic (no `__typeArgs` slot / empty array).
pub(super) fn read_type_arg_names(mi: &Value) -> Vec<String> {
    match read_obj_slot(mi, "__typeArgs") {
        Value::Array(rc) => rc
            .borrow()
            .iter_boxed()
            .map(|t| match read_obj_slot(&t, "__fullName") {
                Value::Str(s) => s.to_string(),
                // fall back to the simple Name slot if __fullName is absent
                _ => match read_obj_slot(&t, "Name") {
                    Value::Str(s) => s.to_string(),
                    _ => String::new(),
                },
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// `__method_generic_arguments(mi: MethodInfo) -> Type[]`. Mirrors C#
/// `MethodInfo.GetGenericArguments()`: a *constructed* generic method (has
/// `__typeArgs`) returns its bound type arguments; a *definition* returns
/// placeholder `Std.Type`s built from the declared type-parameter names
/// (`__typeParamNames`, e.g. `T`/`U`); a non-generic method returns an empty array.
pub fn builtin_method_generic_arguments(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let mi = args.first().cloned().unwrap_or(Value::Null);
    // Constructed state: the bound Type[] set by MakeGenericMethod.
    if let Value::Array(rc) = read_obj_slot(&mi, "__typeArgs") {
        let elems: Vec<Value> = rc.borrow().iter_boxed().collect();
        if !elems.is_empty() {
            return Ok(ctx.heap().alloc_array(elems));
        }
    }
    // Definition state: placeholder Types from the declared type-param names.
    if let Value::Array(rc) = read_obj_slot(&mi, "__typeParamNames") {
        let placeholders: Vec<Value> = rc
            .borrow()
            .iter_boxed()
            .map(|v| match v {
                Value::Str(s) => make_type_from_name(ctx, &s),
                _ => Value::Null,
            })
            .collect();
        return Ok(ctx.heap().alloc_array(placeholders));
    }
    Ok(ctx.heap().alloc_array(Vec::new()))
}

/// `__method_make_generic(mi: MethodInfo, typeArgs: Type[]) -> MethodInfo`. Mirrors
/// C# `MethodInfo.MakeGenericMethod`: binds method-level type arguments on a generic
/// method *definition* and returns a *constructed* `MethodInfo` (same type, no
/// separate subtype) carrying `__typeArgs`. Non-generic receiver or an arity mismatch
/// raises a catchable `Std.Exception` (the native `Err` is wrapped by
/// `exec_call::builtin`). `Invoke` on the result threads `__typeArgs` into the frame.
pub fn builtin_method_make_generic(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let mi = args.first().cloned().unwrap_or(Value::Null);
    let type_args = args.get(1).cloned().unwrap_or(Value::Null);
    // Genericness + arity, validated against the declared type-param names.
    let expected = match read_obj_slot(&mi, "__typeParamNames") {
        Value::Array(rc) => rc.borrow().len(),
        _ => 0,
    };
    if expected == 0 {
        bail!("MakeGenericMethod: method is not a generic method definition");
    }
    let given = match &type_args {
        Value::Array(rc) => rc.borrow().len(),
        _ => 0,
    };
    if given != expected {
        bail!("MakeGenericMethod: expected {expected} type argument(s), got {given}");
    }
    // Clone the definition MethodInfo into a constructed one: preserve identity slots,
    // set __typeArgs, and flip IsGenericMethodDefinition off (IsGenericMethod stays on).
    alloc_named(
        ctx,
        STD_REFLECTION_METHODINFO,
        &[
            ("Name", read_obj_slot(&mi, "Name")),
            ("ReturnType", read_obj_slot(&mi, "ReturnType")),
            ("IsStatic", read_obj_slot(&mi, "IsStatic")),
            ("IsVirtual", read_obj_slot(&mi, "IsVirtual")),
            ("IsAbstract", read_obj_slot(&mi, "IsAbstract")),
            ("IsSealed", read_obj_slot(&mi, "IsSealed")),
            ("IsPublic", read_obj_slot(&mi, "IsPublic")),
            ("IsPrivate", read_obj_slot(&mi, "IsPrivate")),
            ("IsGenericMethod", Value::Bool(true)),
            ("IsGenericMethodDefinition", Value::Bool(false)),
            ("__typeParamNames", read_obj_slot(&mi, "__typeParamNames")),
            ("__typeArgs", type_args),
            ("__parameters", read_obj_slot(&mi, "__parameters")),
            ("__qualified", read_obj_slot(&mi, "__qualified")),
        ],
    )
}

/// Shared reflective-invocation core: resolve `qualified` (main module first,
/// then lazy loader), arity-check against the already-assembled `call_args`
/// (receiver-first for instance methods), execute, and normalize the outcome.
/// `method_type_args` (FQ type-name strings) threads a constructed generic method's
/// bound type arguments into the callee frame (empty for non-generic / definition).
/// A `throw` inside the callee propagates with its ORIGINAL type via
/// `ctx.set_pending_thrown` (consumed by `exec_call::builtin`). Shared by
/// `MethodInfo.Invoke` and `PropertyInfo.GetValue` / `SetValue`.
pub(super) fn invoke_qualified(
    ctx: &VmContext,
    qualified: &str,
    call_args: &[Value],
    method_type_args: &[String],
) -> Result<Value> {
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
            exec_function_with_type_args(ctx, module, f, call_args, method_type_args)?
        }
        None => {
            let f = ctx.try_lookup_function(qualified).ok_or_else(|| {
                anyhow::anyhow!("reflective invoke: function `{qualified}` not found")
            })?;
            invoke_arity_check(qualified, f.param_count, call_args.len())?;
            exec_function_with_type_args(ctx, module, f.as_ref(), call_args, method_type_args)?
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

pub(super) fn invoke_arity_check(qualified: &str, expected: usize, got: usize) -> Result<()> {
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
        let names: Vec<String> = rc.borrow().iter_boxed().filter_map(|v| type_arg_name(&v)).collect();
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

/// `__ctor_invoke(ci: ConstructorInfo, args: object[]) -> object` (add-reflective-invoke).
/// Parameterised construction (mirrors C# `ConstructorInfo.Invoke`): resolve the class
/// from the constructor's `__qualified` name (`<ClassFQN>.<SimpleName>[$N]` → strip the
/// last segment), allocate a default-field instance, then run the constructor with the
/// new object as the reg-0 receiver plus the supplied arguments; return the constructed
/// object. Arity mismatch → catchable `Std.Exception`; a ctor `throw` propagates with
/// its original type via `ctx.set_pending_thrown`.
pub fn builtin_ctor_invoke(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let ci = args.first().cloned().unwrap_or(Value::Null);
    let args_arr = args.get(1).cloned().unwrap_or(Value::Null);
    let qualified = match read_obj_slot(&ci, "__qualified") {
        Value::Str(s) => s.to_string(),
        _ => bail!("ConstructorInfo.Invoke: receiver is not a ConstructorInfo (no __qualified)"),
    };
    // Class FQN = ctor func name minus its last `.` segment (the constructor's own name).
    let class_name = match qualified.rfind('.') {
        Some(i) => qualified[..i].to_string(),
        None => bail!("ConstructorInfo.Invoke: malformed constructor name `{qualified}`"),
    };
    let td = ctx
        .module()
        .and_then(|m| m.type_registry.get(&class_name).cloned())
        .or_else(|| ctx.try_lookup_type(&class_name))
        .ok_or_else(|| anyhow::anyhow!("ConstructorInfo.Invoke: type `{class_name}` not found"))?;
    // Allocate with per-field defaults (same as ObjNew / Activator).
    let slots: Vec<Value> = td
        .fields
        .iter()
        .map(|f| crate::metadata::default_value_for(&f.type_tag))
        .collect();
    let obj = ctx.heap().alloc_object(td.clone(), slots, NativeData::None);
    if matches!(obj, Value::Null) {
        bail!("ConstructorInfo.Invoke: allocation failed for `{class_name}`");
    }
    // Assemble call args: new object as reg-0 receiver, then the object[] elements.
    let mut call_args: Vec<Value> = vec![obj.clone()];
    if let Value::Array(rc) = &args_arr {
        for e in rc.borrow().iter_boxed() {
            call_args.push(e);
        }
    }
    let module_arc = ctx
        .core
        .module
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("ConstructorInfo.Invoke: VmCore.module is None"))?
        .clone();
    let module = module_arc.as_ref();
    let outcome = match module.func_index.get(&qualified) {
        Some(&idx) => {
            let f = &module.functions[idx];
            invoke_arity_check(&qualified, f.param_count, call_args.len())?;
            exec_function(ctx, module, f, &call_args)?
        }
        None => {
            let f = ctx.try_lookup_function(&qualified).ok_or_else(|| {
                anyhow::anyhow!("ConstructorInfo.Invoke: constructor `{qualified}` not found")
            })?;
            invoke_arity_check(&qualified, f.param_count, call_args.len())?;
            exec_function(ctx, module, f.as_ref(), &call_args)?
        }
    };
    match outcome {
        // A constructor returns void; the constructed object is the result.
        ExecOutcome::Returned(_) => Ok(obj),
        ExecOutcome::Thrown(val) => {
            ctx.set_pending_thrown(val);
            bail!("__z42_reflected_throw__")
        }
    }
}
