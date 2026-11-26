/// Object instructions excluding VCall (which lives in `exec_vcall.rs` due
/// to its size). Covers: ObjNew (allocate + ctor), FieldGet / FieldSet,
/// IsInstance / AsCast (runtime type checks), StaticGet / StaticSet.

use crate::metadata::dispatch::ctor::{site_for, instance_type_args};
use crate::metadata::Module;
use crate::object::Value;
use crate::components::bridge::VmContext;
use anyhow::Result;

use super::collect_args;
use super::Frame;

/// `ObjNew` dispatch. Class and ctor come from the site cache (`site`, P1-2 PR 5) once the
/// first allocation resolved them — no name hashing, no locks; the resolution itself is
/// shared with the JIT (`exec::ops::obj_new`).
///
/// Return `Ok(Some(val))` when the ctor `throw`s a user exception (or a symbol check
/// throws), so the caller's `try`/`catch` can match — mirrors the `Call` / `Builtin`
/// propagation pattern in `exec_instr.rs`. `Ok(None)` = success. `Err(...)` = internal
/// anyhow error (separate from user exceptions). A throwing ctor never writes the
/// partially constructed object into `dst`.
#[inline(never)] // 热循环布局：留在 exec_function_body 之外（见 exec_instr.rs 模块注释）
pub(crate) fn obj_new(
    ctx: &VmContext, module: &Module, frame: &mut Frame,
    dst: u32, class_name: &str, ctor_name: &str, args: &[u32],
    // The site's interned type-arguments (`ObjNewInsn::type_args`).
    type_args: crate::object::TypeArgs,
    // This site's `ResolvedTokens::obj_new` cache (None when the resolver hasn't run).
    site: Option<&crate::metadata::dispatch::ic::ObjNewSite>,
    stack_alloc: bool,
    // encode-ctorless-objnew: compile-time positive marker; see `missing_ctor_exception`.
    ctor_known: bool,
) -> Result<Option<Value>> {
    use crate::exec::ops::obj_new::{prepare_ctor, resolve_class};
    // `new List<T>()` in a shared generic body: the site's arguments carry type-parameter
    // markers, bound per activation (one flag test for every other site).
    let type_args = frame.type_param_env().instantiate(ctx, type_args);
    let site = site_for((ctx).funcs(), module, site);
    let type_desc = match resolve_class(ctx, module, class_name, site) {
        Ok(td) => td,
        Err(exc) => return Ok(Some(exc)),
    };

    let mut lazy_holder = None;
    let ctor = match prepare_ctor(
        ctx, module, &type_desc, class_name, ctor_name, args.len() + 1 /* +this */, ctor_known, site,
        &mut lazy_holder,
    ) {
        Ok(c) => c,
        Err(exc) => return Ok(Some(exc)),
    };

    // unify-object-byte-layout (PR-2): fields default to zero-initialized bytes +
    // `Null` refs (= int→0 / bool→false / '\0' / ref→Null, the old per-field
    // defaults), produced by `object_regions()`. Explicit initializers are written by
    // `FieldSet` at the ctor entry.

    let inst_args = instance_type_args(type_args, &type_desc);

    // add-escape-analysis-stack-alloc: when the compiler proved this `new` does
    // not escape its frame AND the ctor does not leak `this`, allocate in the
    // per-context stack arena (no GC region lock / tracking / sweep). The ctor
    // runs on the stack object exactly as on a heap one — `this` is a
    // `Value::StackObject { idx, frame_id }` handle that FieldGet/FieldSet resolve
    // through the execution state, so the ctor can access it from a child frame.
    // `Z42_STACKALLOC=off` bypasses this at runtime (heap) for triage.
    // The instance is built completely — generic arguments, type-parameter field zeros
    // (`ScriptObject::install_type_args`) — before it is published: a heap object never changes
    // its header after that (object model R4; `alloc_instance` builds it in its own block, R6),
    // and the stack branch (`OwnedObject::new_instance`) must stay in lockstep with the heap one,
    // or flipping `Z42_STACKALLOC` changes observable values.
    let obj_val = if stack_alloc && crate::exec::stack_alloc::stack_alloc_enabled(ctx) {
        let obj = crate::object::OwnedObject::new_instance(type_desc.clone(), inst_args);
        let frame_id = frame.frame_id(ctx);
        ctx.exec().alloc_stack_object(frame_id, obj)
    } else {
        let obj_val = ctx.heap().alloc_instance(type_desc, inst_args);

        // add-gc-oom-exception: the allocation returns Null only under strict OOM.
        // make_oom_exception toggles strict OOM off while building the exception
        // object (which itself allocates) and restores it after.
        if matches!(obj_val, Value::Null) {
            return Ok(Some(crate::exec::exception::make_oom_exception(
                ctx, module,
                format!("cannot allocate `{class_name}`: heap limit exceeded"),
            )));
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

    // fix-ctor-throw-propagation : if the ctor threw a user
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

/// `FieldGet` — adapter over [`crate::exec::ops::object::field::field_get`] (the single
/// implementation shared with the JIT: FieldIC fast path, stack objects, `Length`
/// pseudo-fields, boxed structs; a null receiver throws `NullReferenceException`).
#[inline]
pub(super) fn field_get(
    ctx: &VmContext, module: &Module, frame: &mut Frame, dst: u32, obj: u32, field_name: &str,
    field_ic: Option<&crate::metadata::dispatch::ic::FieldIC>,
) -> Result<Option<Value>> {
    match crate::exec::ops::object::field::field_get(ctx, frame.get(obj)?, field_name, field_ic) {
        Ok(v) => { frame.set(dst, v); Ok(None) }
        Err(e) => super::raise(ctx, module, e),
    }
}

/// `FieldSet` — adapter over [`crate::exec::ops::object::field::field_set`] (write barrier,
/// primitive-slot type check, null receiver → `NullReferenceException`).
#[inline]
pub(super) fn field_set(
    ctx: &VmContext, module: &Module, frame: &mut Frame, obj: u32, field_name: &str, val: u32,
    field_ic: Option<&crate::metadata::dispatch::ic::FieldIC>,
) -> Result<Option<Value>> {
    let v = frame.get(val)?;
    // add-escape-analysis-stack-alloc (diagnostic #2): FieldSet.val is an escape
    // sink — a stack handle stored into a field would outlive its frame.
    debug_assert!(
        !matches!(v, Value::StackObject { .. } | Value::StackArray { .. }),
        "stack-alloc handle stored into a field — escape analysis unsound (FieldSet.val)"
    );
    match crate::exec::ops::object::field::field_set(ctx, frame.get(obj)?, field_name, v, field_ic) {
        Ok(()) => Ok(None),
        Err(e) => super::raise(ctx, module, e),
    }
}

#[path = "exec_object_isa.rs"]
mod isa;
pub(super) use isa::{as_cast, is_instance};

/// `StaticGet` — adapter over [`crate::exec::ops::object::statics::static_get`] (init barrier,
/// lazy zero-init of never-assigned value-type statics, missing-symbol check).
/// `field_id` = resolver-populated `StaticFieldId` (None → by-name fallback).
#[inline(never)] // 热循环布局：留在 exec_function_body 之外（见 exec_instr.rs 模块注释）
pub(super) fn static_get(
    ctx: &VmContext, module: &Module, frame: &mut Frame, dst: u32, field: &str,
    field_id: Option<u32>,
) -> Result<Option<Value>> {
    match crate::exec::ops::object::statics::static_get(ctx, module, field, field_id) {
        Ok(v) => { frame.set(dst, v); Ok(None) }
        Err(e) => super::raise(ctx, module, e),
    }
}

#[inline(never)] // 热循环布局：留在 exec_function_body 之外（见 exec_instr.rs 模块注释）
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
    match crate::exec::ops::object::statics::static_set(ctx, module, field, field_id, v) {
        Ok(()) => Ok(None),
        Err(e) => super::raise(ctx, module, e),
    }
}
