use super::*;

// ── Reflective invocation (add-method-invoke-non-generic, 0.3.12) ────────────

/// make-enum-distinct-type 1.5: unwrap a boxed integer / enum back to its bare scalar
/// for a reflective call argument.
///
/// The reflective boundary is untyped by construction (`object[]`), but the VM's calling
/// convention passes primitive parameters as bare `I64` — a method compiled for `int by`
/// reads reg 1 as a scalar and would otherwise do arithmetic on a heap box. Callers used
/// to get away without this because **assignment never boxed**: `object[] a; a[0] = 5;`
/// stored a bare i64. Now that the array-element store boxes like every other erasure
/// point (the gap this change closed), the unwrap has to happen here — the same call
/// `FieldInfo.SetValue` already makes, for the same reason (`accessors.rs`).
///
/// Uniform rather than driven by the callee's declared parameter types: `param_types` is
/// cold/debug metadata a release build may not carry, and behaviour that changed with
/// debug symbols would be worse than this. The cost is that a genuinely `object`-typed
/// parameter receives the bare scalar instead of the box — exactly what it received
/// before this change, so nothing regresses.
fn unbox_reflective_arg(v: Value) -> Value {
    if let Value::BoxedStruct(gc) = &v {
        if let Some(n) = gc.borrow().boxed_prim_i64() {
            return Value::I64(n);
        }
    }
    v
}

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

/// 反射重入类 builtin 的统一错误出口(P1-8 批次 6):被反射调用的 z42 代码抛了
/// (`invoke_qualified` / `exec_function` 已 `set_pending_thrown` + bail 魔法串)→ `Thrown`
/// 原样传播(raise 侧再 set 回 pending,异常类型与栈不变);其余 anyhow 错误 →
/// `Legacy`(`Std.Exception`,与旧表语义一致)。
pub(crate) fn reentry<T>(ctx: &VmContext, r: anyhow::Result<T>) -> Result<T, BuiltinError> {
    r.map_err(|e| match ctx.take_pending_thrown() {
        Some(v) => BuiltinError::Thrown(v),
        None => BuiltinError::Legacy(e),
    })
}

/// `__type_get_type(fqn: str) -> Type` — FQN string → `Std.Type` (null if unknown).
///
/// fix-emit-zbc-swallows-diagnostics: honour the documented "null if unknown"
/// contract (`Type.z42`: "Resolve a Type by fully-qualified name …; null if
/// unknown"). `make_type_from_name` never returns Null — on a total miss it
/// synthesises a handle-less `Type` carrying just the name, so a typo'd FQN used
/// to come back as an object whose `GetMethods()` is empty and whose `BaseType`
/// is null, indistinguishable from a real-but-memberless type.
///
/// Only *qualified class names* are held to this: a name containing `.` that is
/// neither an array (`[]`) nor a constructed generic (`<…>`) must resolve to a
/// real `TypeHandle`. Arrays, constructed generics, primitives and unqualified
/// simple names keep their existing synthetic-Type semantics — those are load-
/// bearing (e.g. `typeof(int[])`, cross-package generic type-arg names).
pub fn type_get_type(ctx: &VmContext, fqn: &crate::metadata::vstr::Str) -> Value {
    let t = make_type_from_name(ctx, fqn);
    let qualified_class = fqn.contains('.') && !fqn.ends_with("[]") && !fqn.contains('<');
    if qualified_class && type_handle_of(&t).is_none() {
        return Value::Null;
    }
    t
}

/// `__method_invoke(method: MethodInfo, obj: object, args: object[]) -> object`.
/// Reflectively invokes the method named by the MethodInfo's hidden `__qualified`:
/// instance methods take `obj` as receiver (reg 0); static methods ignore it.
/// `args` (an `object[]`) maps to the remaining parameters in order. Returns the
/// method's return value (`void` → null). A `throw` inside the invoked method is
/// propagated with its ORIGINAL type via `ctx.set_pending_thrown` (consumed by
/// `exec_call::builtin`), so callers can `try/catch` the real exception.
pub fn method_invoke(ctx: &VmContext, mi: &Value, this_obj: &Value, args_arr: &Value) -> anyhow::Result<Value> {
    let mi = mi.clone();
    let this_obj = this_obj.clone();
    let args_arr = args_arr.clone();

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
            call_args.push(unbox_reflective_arg(e.clone()));
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
            .map(|t| type_value_arg_name(&t).unwrap_or_default())
            .collect(),
        _ => Vec::new(),
    }
}

/// `__method_generic_arguments(mi: MethodInfo) -> Type[]`. Mirrors C#
/// `MethodInfo.GetGenericArguments()`: a *constructed* generic method (has
/// `__typeArgs`) returns its bound type arguments; a *definition* returns
/// placeholder `Std.Type`s built from the declared type-parameter names
/// (`__typeParamNames`, e.g. `T`/`U`); a non-generic method returns an empty array.
pub fn method_generic_arguments(ctx: &VmContext, mi: &Value) -> Result<Value, BuiltinError> {
    let mi = mi.clone();
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
pub fn method_make_generic(ctx: &VmContext, mi: &Value, type_args_arr: &Value) -> Result<Value, BuiltinError> {
    // split-null-sentinel-channels ⑥：显式校验**本 builtin 自身**的 arity 后直接下标。
    // ⚠️ 别与下方的 `invoke_arity_check` 混淆 —— 那个校验的是**被反射调用的 z42 函数**的
    // 形参个数 vs 用户给的实参数组，与本 builtin 收几个参数是两件事。
    let mi = mi.clone();
    let type_args = type_args_arr.clone();
    // Genericness + arity, validated against the declared type-param names.
    let expected = match read_obj_slot(&mi, "__typeParamNames") {
        Value::Array(rc) => rc.borrow().len(),
        _ => 0,
    };
    if expected == 0 {
        return Err(BuiltinError::InvalidOperation("MakeGenericMethod: method is not a generic method definition".into()));
    }
    let given = match &type_args {
        Value::Array(rc) => rc.borrow().len(),
        _ => 0,
    };
    if given != expected {
        return Err(BuiltinError::Argument(format!("MakeGenericMethod: expected {expected} type argument(s), got {given}")));
    }
    // Clone the definition MethodInfo into a constructed one: preserve identity slots,
    // set __typeArgs, and flip IsGenericMethodDefinition off (IsGenericMethod stays on).
    Ok(alloc_named(
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
    ).map_err(BuiltinError::Legacy)?)
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
pub fn invoke_static(ctx: &VmContext, fqn: &crate::metadata::vstr::Str) -> anyhow::Result<Value> {
    let qualified = fqn.to_string();
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

/// What a reflective construction of the type named `name` (canonical) allocates — the same
/// representation `new` gives that type: its instantiation descriptor when a loaded package
/// declares one (with the arguments the descriptor carries), else its definition descriptor
/// plus the arguments as per-instance type args (an imported generic). Non-generic `name` →
/// its descriptor. `None` when nothing by that name is loaded.
pub(super) fn construction_target(
    ctx: &VmContext, name: &str,
) -> Option<(Arc<TypeDesc>, crate::metadata::types::TypeArgs)> {
    let lookup = |n: &str| {
        ctx.module()
            .and_then(|m| m.type_registry.get(n).cloned())
            .or_else(|| ctx.try_lookup_type(n))
    };
    if let Some(td) = lookup(name) {
        let args = td.type_args_handle();
        return Some((td, args));
    }
    let def = crate::metadata::type_names::erased(name);
    if def.len() == name.len() {
        return None;
    }
    let td = lookup(def)?;
    let args = crate::metadata::types::type_args_from_name(name);
    Some((td, crate::metadata::types::TypeArgs::intern(&args)))
}

/// A default-initialised instance of `td` with generic arguments `type_args` — fields zeroed as
/// `ObjNew` zeroes them (type-parameter fields included), arguments installed before the object
/// is published (object model R4). `Null` under strict-OOM refusal.
fn alloc_default(ctx: &VmContext, td: &Arc<TypeDesc>, type_args: crate::metadata::types::TypeArgs) -> Value {
    let mut inst = crate::metadata::types::OwnedObject::of_type(td.clone());
    for (i, f) in td.fields.iter().enumerate() {
        inst.set_field_value(i, &crate::metadata::default_value_for(&f.type_tag));
    }
    for (i, v) in crate::metadata::types::generic_field_zero_overrides(td, &type_args) {
        inst.set_field_value(i, &v);
    }
    if !type_args.is_empty() {
        inst.set_type_args(type_args);
    }
    ctx.heap().alloc_script_object(inst)
}

/// `__activator_create(type: Type) -> object` — no-arg reflective construction.
/// Mirrors the interpreter's `ObjNew`: alloc with per-field defaults, then run the
/// no-arg ctor (`<Class>` bare or `<Class>$0` overload-mangled) if one exists. A
/// ctor `throw` propagates with its original type via `ctx.pending_thrown`. Only
/// no-arg construction (test-class instantiation for the reflective test runner);
/// parameterised CreateInstance is deferred.
pub fn activator_create(ctx: &VmContext, t: &Value) -> anyhow::Result<Value> {
    let args = [*t];
    let args: &[Value] = &args;
    let td = match type_handle(args) {
        Some(td) => td,
        None => bail!("Activator.CreateInstance: type has no runtime handle (primitive/array/synthetic?)"),
    };
    // fix-new-prim-value: 构造一个基元 = 它的零值，不是「一个 0 字段的 ScriptObject」。
    //
    // 这是 `new T()`（T 为方法级型参）在 T 绑到基元时走到的路径——编译期发
    // `MethodTypeArgInsn` + 本 builtin，类型直到运行期才知道，所以折叠只能在这里做
    // （非泛型的 `new int()` 由 `ConstructTyper._bindNew` 在编译期折成同样的零值）。
    //
    // ⚠️ 上面那句 bail 的括注「primitive/array/synthetic?」**说反了**：`Std.Int32` 是真
    // struct 类型、runtime handle 一直在，于是基元从不落那条 bail，而是照常 alloc 出一个
    // 空对象 —— `int`/`double`/`char` 崩在不相干的地方，`bool` 更糟：产出的东西 `if` 判真、
    // 却 `== true` 与 `== false` 同时为假。
    //
    // `default_value_for` 已经认 FQ 包装名（`fix-type-reflection-names` 加的），与
    // `MethodDefaultInsn`（`default(T)`）读的是同一张表 ⇒ `new T()` 与 `default(T)` 对基元
    // 恒等，由构造保证。`Std.String` 是唯一的分歧：`default(string)` 是 null，而
    // `new string()` 是「构造一个」⇒ 空串（User 裁决）。
    if td.name == crate::metadata::well_known_names::STD_STRING {
        return Ok(Value::Str("".into()));
    }
    if crate::metadata::well_known_names::is_scalar_prim_wrapper(&td.name) {
        return Ok(crate::metadata::default_value_for(&td.name));
    }
    // plan-generic-reflection G1: a constructed generic Type (typeof(Box<int>) or
    // MakeGenericType(...)) builds an instance of *that* instantiation, the same one `new`
    // builds (`construction_target`) — so it passes `is Box<int>` and reports its own Type.
    let (td, type_args) = match type_value_name(t).and_then(|n| construction_target(ctx, &n)) {
        Some(target) => target,
        None => (td, crate::metadata::types::TypeArgs::empty()),
    };
    let class_name = td.name.clone();
    let obj = alloc_default(ctx, &td, type_args);
    if matches!(obj, Value::Null) {
        bail!("Activator.CreateInstance: allocation failed for `{class_name}`");
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
    // fix-nested-ctor-key: `TypeDesc::ctor_member_name` is the single spelling of "what is
    // this class's ctor called". The local `rsplit('.')` it replaces left a nested type's
    // `+` declaring-type prefix attached, so neither candidate key existed and
    // `Activator.CreateInstance(typeof(Outer.Inner))` silently fell through to the
    // default-field allocation — object handed back with its constructor never run.
    let simple = crate::metadata::types::TypeDesc::ctor_member_name(class_name.as_str());
    // An instantiation descriptor (`Demo.Box<int>`) has its own constructor only when its layout
    // differs from the definition's; otherwise it shares the definition's (what `new` calls).
    let def_name = crate::metadata::type_names::erased(&class_name);
    let cands = [
        format!("{class_name}.{simple}"), format!("{class_name}.{simple}$0"),
        format!("{def_name}.{simple}"), format!("{def_name}.{simple}$0"),
    ];
    // fix-activator-arity: **the bare candidate is not necessarily parameterless.**
    // A type whose *only* constructor takes arguments registers it under the bare name
    // (single/primary-ctor convention, `OverloadBinder._ctorKey`), so `<Class>.<Simple>`
    // happily resolves to `Vec2(int, int)` — and we would then call it with one argument
    // (`this`), leaving every parameter `Null`. Measured: `new T()` with `T = Vec2` died as
    // `struct field: expected an integer value, got Null` **inside the constructor**, which
    // points at the callee and says nothing about the real cause.
    //
    // The non-generic path already rejects this at compile time (`new Vec2()` → E0426), but
    // `new T()` cannot: `T` is only known here. So replicate the arity requirement — and when
    // nothing satisfies it, the default-field allocation IS the construction (exactly what the
    // comment above says for a type with no constructor at all, and what C# gives a struct).
    run_first_noarg_ctor(ctx, module, &cands, &obj)?;
    Ok(boxed_if_struct(&td, obj))
}


/// [`activator_create`] 的构造函数执行段(fnlines 拆分):按 `<FQ>.<Simple>` →
/// `<FQ>.<Simple>$0` 的次序找第一个**零参**(param_count ≤ 1,含 this)构造函数并跑它;
/// 都不存在则什么也不跑(默认字段分配即构造)。构造器 `throw` 原样传播(pending_thrown)。
fn run_first_noarg_ctor(
    ctx: &VmContext, module: &crate::metadata::Module, cands: &[String], obj: &Value,
) -> anyhow::Result<()> {
    let arity_ok = |f: &crate::metadata::Function| f.param_count <= 1;   // param_count includes `this`
    for cand in cands {
        let cand = cand.as_str();
        let outcome = match module.func_index.get(cand) {
            Some(&idx) if arity_ok(&module.functions[idx]) => {
                Some(exec_function(ctx, module, &module.functions[idx], &[obj.clone()])?)
            }
            Some(_) => None,
            None => match ctx.try_lookup_function(cand) {
                Some(f) if arity_ok(f.as_ref()) => {
                    Some(exec_function(ctx, module, f.as_ref(), &[obj.clone()])?)
                }
                _ => None,
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
    Ok(())
}

/// A reflectively constructed **value type** leaves as a boxed struct: the same object block,
/// tagged `Value::BoxedStruct` instead of `Value::Object`. That is the representation every
/// boxed-struct path keys on — `is` / `as` / `GetType` / the unboxing `AsCast` at a generic
/// call site (`T mk<T>() { return new T(); }` called as `mk<Vec2>()` comes back through it).
fn boxed_if_struct(td: &crate::metadata::types::TypeDesc, obj: Value) -> Value {
    if td.is_struct() {
        if let Value::Object(gc) = obj { return Value::BoxedStruct(gc); }
    }
    obj
}

/// `__ctor_invoke(ci: ConstructorInfo, args: object[]) -> object` (add-reflective-invoke).
/// Parameterised construction (mirrors C# `ConstructorInfo.Invoke`): resolve the class
/// from the constructor's `__qualified` name (`<ClassFQN>.<SimpleName>[$N]` → strip the
/// last segment), allocate a default-field instance, then run the constructor with the
/// new object as the reg-0 receiver plus the supplied arguments; return the constructed
/// object. Arity mismatch → catchable `Std.Exception`; a ctor `throw` propagates with
/// its original type via `ctx.set_pending_thrown`.
pub fn ctor_invoke(ctx: &VmContext, ci: &Value, args_arr: &Value) -> anyhow::Result<Value> {
    let ci = ci.clone();
    let args_arr = args_arr.clone();
    let qualified = match read_obj_slot(&ci, "__qualified") {
        Value::Str(s) => s.to_string(),
        _ => bail!("ConstructorInfo.Invoke: receiver is not a ConstructorInfo (no __qualified)"),
    };
    // Class FQN = ctor func name minus its last `.` segment (the constructor's own name).
    let class_name = match qualified.rfind('.') {
        Some(i) => qualified[..i].to_string(),
        None => bail!("ConstructorInfo.Invoke: malformed constructor name `{qualified}`"),
    };
    // The type the ConstructorInfo was enumerated from (`__owner`): a constructed Type's
    // constructor builds that instantiation, not the bare definition.
    let owner = match read_obj_slot(&ci, "__owner") {
        Value::Str(s) if !s.is_empty() => s.to_string(),
        _ => class_name.clone(),
    };
    let (td, type_args) = construction_target(ctx, &owner)
        .ok_or_else(|| anyhow::anyhow!("ConstructorInfo.Invoke: type `{owner}` not found"))?;
    // An instantiation with its own (layout-specialised) constructor runs that one.
    let qualified = match qualified.strip_prefix(class_name.as_str()) {
        Some(member) if td.name != class_name => {
            let spec = format!("{}{member}", td.name);
            let exists = ctx.module().is_some_and(|m| m.func_index.contains_key(&spec))
                || ctx.try_lookup_function(&spec).is_some();
            if exists { spec } else { qualified }
        }
        _ => qualified,
    };
    let obj = alloc_default(ctx, &td, type_args);
    if matches!(obj, Value::Null) {
        bail!("ConstructorInfo.Invoke: allocation failed for `{class_name}`");
    }
    // Assemble call args: new object as reg-0 receiver, then the object[] elements.
    let mut call_args: Vec<Value> = vec![obj.clone()];
    if let Value::Array(rc) = &args_arr {
        for e in rc.borrow().iter_boxed() {
            call_args.push(unbox_reflective_arg(e));
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
        ExecOutcome::Returned(_) => Ok(boxed_if_struct(&td, obj)),
        ExecOutcome::Thrown(val) => {
            ctx.set_pending_thrown(val);
            bail!("__z42_reflected_throw__")
        }
    }
}

/// 反射合成类迁 builtin ABI v2(P1-8 批次 5):`GetType`(FQN 解析)、
/// `GetGenericArguments`、`MakeGenericMethod` 都是纯合成,不执行 z42 代码。
#[z42_macros::builtins]
pub mod typed {
    use super::*;

    #[builtin(name = "__type_get_type")]
    pub fn get_type(ctx: &VmContext, fqn: &crate::metadata::vstr::Str) -> Value { type_get_type(ctx, fqn) }

    #[builtin(name = "__method_generic_arguments")]
    pub fn generic_args(ctx: &VmContext, mi: &Value) -> Result<Value, BuiltinError> {
        method_generic_arguments(ctx, mi)
    }

    #[builtin(name = "__method_make_generic")]
    pub fn make_generic(ctx: &VmContext, mi: &Value, type_args: &Value) -> Result<Value, BuiltinError> {
        method_make_generic(ctx, mi, type_args)
    }

    /// 重入类(P1-8 批次 6):可能执行用户 z42 代码,标 `reenters` 压轻帧;
    /// 被调代码抛出的 z42 异常经 `BuiltinError::Thrown` 原样传播。
    #[builtin(name = "__method_invoke", reenters)]
    pub fn invoke_method(ctx: &VmContext, this: &Value, obj: &Value, args: &Value) -> Result<Value, BuiltinError> {
        reentry(ctx, method_invoke(ctx, this, obj, args))
    }

    #[builtin(name = "__invoke_static", reenters)]
    pub fn invoke_static_fn(ctx: &VmContext, fqn: &crate::metadata::vstr::Str) -> Result<Value, BuiltinError> {
        reentry(ctx, invoke_static(ctx, fqn))
    }

    #[builtin(name = "__activator_create", reenters)]
    pub fn activator(ctx: &VmContext, t: &Value) -> Result<Value, BuiltinError> {
        reentry(ctx, activator_create(ctx, t))
    }

    #[builtin(name = "__ctor_invoke", reenters)]
    pub fn ctor(ctx: &VmContext, this: &Value, args: &Value) -> Result<Value, BuiltinError> {
        reentry(ctx, ctor_invoke(ctx, this, args))
    }
}
