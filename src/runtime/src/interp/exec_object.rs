/// Object instructions excluding VCall (which lives in `exec_vcall.rs` due
/// to its size). Covers: ObjNew (allocate + ctor), FieldGet / FieldSet,
/// IsInstance / AsCast (runtime type checks), StaticGet / StaticSet.

use crate::metadata::{Module, NativeData, ScriptObject, Value};
use crate::vm_context::VmContext;
use anyhow::Result;

use super::dispatch::isa_td;
use super::exec_vcall::is_array_isa;
use super::ops::collect_args;
use super::Frame;

/// `ObjNew` dispatch. Class and ctor come from the site cache (`site`, P1-2 PR 5) once the
/// first allocation resolved them — no name hashing, no locks; the resolution itself is
/// shared with the JIT (`obj_new_resolve`).
///
/// Return `Ok(Some(val))` when the ctor `throw`s a user exception (or a symbol check
/// throws), so the caller's `try`/`catch` can match — mirrors the `Call` / `Builtin`
/// propagation pattern in `exec_instr.rs`. `Ok(None)` = success. `Err(...)` = internal
/// anyhow error (separate from user exceptions). A throwing ctor never writes the
/// partially constructed object into `dst`.
pub(super) fn obj_new(
    ctx: &VmContext, module: &Module, frame: &mut Frame,
    dst: u32, class_name: &str, ctor_name: &str, args: &[u32], type_args: &[String],
    // This site's `ResolvedTokens::obj_new` cache (None when the resolver hasn't run).
    site: Option<&crate::metadata::resolver::ObjNewSite>,
    stack_alloc: bool,
    // encode-ctorless-objnew: compile-time positive marker; see `missing_ctor_exception`.
    ctor_known: bool,
) -> Result<Option<Value>> {
    use super::obj_new_resolve::{resolve_class, resolve_ctor, site_for};
    let site = site_for(ctx, module, site);
    let type_desc = match resolve_class(ctx, module, class_name, site) {
        Ok(td) => td,
        Err(exc) => return Ok(Some(exc)),
    };

    // add-static-constructors：创建实例是 C# 的类型初始化触发点之一。此处 TypeDesc
    // 已在手 → 检查代价就是一次 `Option` 判断（没有 cctor 的类型的冷区多半是 None），
    // 不需要 `pending` 门。
    // add-module-init-hook：跨包 `new` 同样可能刚拉进一个包 —— 其包初始化器先跑。
    if let Err(msg) = ctx.ensure_module_inits(Some(type_desc.name.as_str())) {
        return Ok(Some(crate::vm_context::cctor::make_type_init_exception(ctx, module, &msg)));
    }
    if let Err(msg) = ctx.ensure_type_init(&type_desc) {
        return Ok(Some(crate::vm_context::cctor::make_type_init_exception(ctx, module, &msg)));
    }

    // The ctor is bound before the object exists: binding may load a package (and run its
    // initializers), and a fresh object held only in a Rust local is not a GC root.
    let mut lazy_holder = None;
    let ctor = match resolve_ctor(
        ctx, module, class_name, ctor_name, args.len() + 1 /* +this */, ctor_known, site,
        &mut lazy_holder,
    ) {
        Ok(c) => c,
        Err(exc) => return Ok(Some(exc)),
    };

    // unify-object-byte-layout (PR-2): fields default to zero-initialized bytes +
    // `Null` refs (= int→0 / bool→false / '\0' / ref→Null, the old per-field
    // defaults), produced by `object_regions()`. Explicit initializers are written by
    // `FieldSet` at the ctor entry.

    // add-escape-analysis-stack-alloc: when the compiler proved this `new` does
    // not escape its frame AND the ctor does not leak `this`, allocate in the
    // per-context stack arena (no GC region lock / tracking / sweep). The ctor
    // runs on the stack object exactly as on a heap one — `this` is a
    // `Value::StackObject { idx, frame_id }` handle that FieldGet/FieldSet resolve
    // through `ctx.stack_arena`, so the ctor's child frame reaches it fine.
    // `Z42_STACKALLOC=off` bypasses this at runtime (heap) for triage.
    let obj_val = if stack_alloc && crate::interp::stack_alloc::stack_alloc_enabled() {
        let storage = type_desc.object_storage();
        let mut obj = ScriptObject::new(type_desc.clone(), storage);
        // complete-generic-class-identity P1: an instantiation's class name already carries
        // its arguments (`Demo.Box<int>`), so the compiler stops shipping a second copy on
        // the instruction. Fall back to the ones the registry parsed off the name.
        obj.set_type_args(if type_args.is_empty() {
            Box::<[String]>::from(type_desc.type_args())
        } else {
            Box::<[String]>::from(type_args)
        });
        // fix-generic-typeparam-field-zero: a `T`-typed field is a *reference* slot (the
        // layout is computed from the declaration), so layout zero-init leaves it `Null`.
        // Rewrite it to the instantiation's real zero. Must stay in lockstep with the heap
        // branch below — otherwise flipping `Z42_STACKALLOC` changes observable values.
        for (slot, zero) in crate::metadata::types::generic_field_zero_overrides(
            &type_desc, obj.type_args(),
        ) {
            obj.set_field_value(slot, &zero);
        }
        let frame_id = frame.frame_id(ctx);
        let idx = ctx.stack_alloc_obj(frame_id, obj);
        Value::StackObject { idx, frame_id }
    } else {
        let obj_val = ctx.heap().alloc_object(type_desc, Vec::new(), NativeData::None);

        // add-gc-oom-exception: alloc_object returns Null only under strict OOM.
        // make_oom_exception toggles strict OOM off while building the exception
        // object (which itself allocates) and restores it after.
        if matches!(obj_val, Value::Null) {
            return Ok(Some(crate::exception::make_oom_exception(
                ctx, module,
                format!("cannot allocate `{class_name}`: heap limit exceeded"),
            )));
        }

        // 2026-05-07 add-default-generic-typeparam (D-8b-3 Phase 2): populate
        // per-instance type_args from the IR instruction. Read by `DefaultOf`.
        // complete-generic-class-identity P1: fall back to the arguments the registry parsed
        // off the instantiation's own name (see the stack branch).
        let name_args: Box<[String]> = if type_args.is_empty() {
            if let Value::Object(ref rc) = obj_val {
                Box::<[String]>::from(rc.borrow().type_desc.type_args())
            } else { Box::new([]) }
        } else { Box::new([]) };
        let inst_args: &[String] =
            if type_args.is_empty() { &name_args } else { type_args };
        if !inst_args.is_empty() {
            if let Value::Object(ref rc) = obj_val {
                let mut o = rc.borrow_mut();
                o.set_type_args(Box::<[String]>::from(inst_args));
                // fix-generic-typeparam-field-zero: see the stack branch above. Kept in the
                // same `!inst_args.is_empty()` block because a non-generic instance can
                // never need an override.
                let overrides = crate::metadata::types::generic_field_zero_overrides(
                    &o.type_desc, inst_args,
                );
                for (slot, zero) in overrides {
                    o.set_field_value(slot, &zero);
                }
            }
        }
        obj_val
    };

    // `None` = a class without a ctor: the object stays default-initialised.
    let outcome = match ctor {
        Some((ctor, _id)) => {
            let mut ctor_args = vec![obj_val.clone()];
            ctor_args.extend(collect_args(&frame.regs, args)?);
            Some(super::exec_function(ctx, module, ctor, &ctor_args)?)
        }
        None => None,
    };

    // fix-ctor-throw-propagation (2026-05-24): if the ctor threw a user
    // exception, surface it via Ok(Some(val)) so the enclosing try/catch
    // can match. Do NOT write `obj_val` into `dst` — the object is
    // partially constructed and the caller is about to jump to a catch
    // handler that won't read it.
    if let Some(super::ExecOutcome::Thrown(val)) = outcome {
        return Ok(Some(val));
    }

    frame.set(dst, obj_val);
    Ok(None)
}

/// `FieldGet` — adapter over [`crate::objops::field::field_get`] (the single
/// implementation shared with the JIT: FieldIC fast path, stack objects, `Length`
/// pseudo-fields, boxed structs; a null receiver throws `NullReferenceException`).
#[inline]
pub(super) fn field_get(
    ctx: &VmContext, module: &Module, frame: &mut Frame, dst: u32, obj: u32, field_name: &str,
    field_ic: Option<&crate::metadata::resolver::FieldIC>,
) -> Result<Option<Value>> {
    match crate::objops::field::field_get(ctx, frame.get(obj)?, field_name, field_ic) {
        Ok(v) => { frame.set(dst, v); Ok(None) }
        Err(e) => super::ops::raise(ctx, module, e),
    }
}

/// `FieldSet` — adapter over [`crate::objops::field::field_set`] (write barrier,
/// primitive-slot type check, null receiver → `NullReferenceException`).
#[inline]
pub(super) fn field_set(
    ctx: &VmContext, module: &Module, frame: &mut Frame, obj: u32, field_name: &str, val: u32,
    field_ic: Option<&crate::metadata::resolver::FieldIC>,
) -> Result<Option<Value>> {
    let v = frame.get(val)?;
    // add-escape-analysis-stack-alloc (diagnostic #2): FieldSet.val is an escape
    // sink — a stack handle stored into a field would outlive its frame.
    debug_assert!(
        !matches!(v, Value::StackObject { .. } | Value::StackArray { .. }),
        "stack-alloc handle stored into a field — escape analysis unsound (FieldSet.val)"
    );
    match crate::objops::field::field_set(ctx, frame.get(obj)?, field_name, v, field_ic) {
        Ok(()) => Ok(None),
        Err(e) => super::ops::raise(ctx, module, e),
    }
}

/// fix-boxed-primitive-is-as: 基元值是否 is-a `class_name`。z42 不装箱基元，故 `object o = "hi"`
/// 里 o 仍是裸 `Value::Str` —— `is`/`as` 须按其 stdlib 类名（`primitive_class_name`，如
/// `Std.String`）匹配，外加 `Std.Object` 基类（所有基元 is-a object）。编译器 `QualifyTypeName`
/// 发 FQ 形（`Std.String`/`Std.Int32`/`Std.Object`），此处直接比 FQ。
pub(crate) fn prim_isa(val: &Value, class_name: &str) -> bool {
    // 所有基元 is-a object。
    if class_name == "Std.Object" || class_name == "Object" {
        return super::exec_vcall::primitive_class_name(val).is_some();
    }
    match val {
        // 整数宽度不可辨：z42 运行时用单一 Value::I64 表示 int/long/short/byte/…（boxed 后
        // 无宽度信息），故一个 boxed 整数 is-a **任意整数类型**——值的声明类型永不假阴，
        // 跨宽度松匹配是该表示的必然（`9L is long` / `(byte)7 is byte` 才不会误判 false）。
        Value::I64(_) => is_integer_class(class_name),
        // 非整数基元：精确匹配其 stdlib 类名（string/double/bool/char）。
        other => super::exec_vcall::primitive_class_name(other) == Some(class_name),
    }
}

#[path = "exec_object_isa.rs"]
mod isa;
pub(super) use isa::{as_cast, is_instance};
use isa::is_integer_class;

/// `StaticGet` — adapter over [`crate::objops::statics::static_get`] (init barrier,
/// lazy zero-init of never-assigned value-type statics, missing-symbol check).
/// `field_id` = resolver-populated `StaticFieldId` (None → by-name fallback).
pub(super) fn static_get(
    ctx: &VmContext, module: &Module, frame: &mut Frame, dst: u32, field: &str,
    field_id: Option<u32>,
) -> Result<Option<Value>> {
    match crate::objops::statics::static_get(ctx, module, field, field_id) {
        Ok(v) => { frame.set(dst, v); Ok(None) }
        Err(e) => super::ops::raise(ctx, module, e),
    }
}

pub(super) fn static_set(
    ctx: &VmContext, module: &Module, frame: &Frame, field: &str, val: u32,
    field_id: Option<u32>,
) -> Result<Option<Value>> {
    let v = *frame.get(val)?;
    // add-escape-analysis-stack-alloc (diagnostic #2): StaticSet.val is an escape
    // sink — a stack handle stored into a static would outlive its frame.
    debug_assert!(
        !matches!(v, Value::StackObject { .. } | Value::StackArray { .. }),
        "stack-alloc handle stored into a static field — escape analysis unsound (StaticSet.val)"
    );
    match crate::objops::statics::static_set(ctx, module, field, field_id, v) {
        Ok(()) => Ok(None),
        Err(e) => super::ops::raise(ctx, module, e),
    }
}
