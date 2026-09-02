/// Virtual dispatch (`VCall`): the interpreter's **invoke** side.
///
/// unify-vcall-resolution (2026-09-03): *what* to call is decided by
/// `vcall_resolve::resolve_vcall` (shared with `jit/helpers/vcall.rs::jit_vcall`);
/// this file only keeps the interpreter-specific pieces — the receiver-kind
/// helpers (`primitive_class_name` / `value_synthetic_type_id` / `is_array_isa`),
/// the mixed-mode divert to an already-compiled native method, and running the
/// resolved target on an interpreter frame.

use crate::metadata::{Module, Value};
use crate::vm_context::VmContext;
use anyhow::{bail, Result};

use super::vcall_resolve::{resolve_vcall, vcall_ic_hit, VCallTarget};
use super::{ExecOutcome, Frame};

/// runtime-jit-tiering Phase 1.5 (mixed-mode): receiver-aware analogue of
/// `exec_call::try_native_static_call`. If method `idx` is already JIT-compiled,
/// invoke it natively (receiver in reg 0 via `new_method_args_from`) and marshal the
/// result. `None` → not published / cold / untranslatable → caller stays interp.
#[cfg(feature = "jit")]
fn try_native_method_call(
    ctx: &VmContext, frame: &mut Frame, dst: u32, idx: usize,
    receiver: &Value, args: &[u32],
) -> Option<Result<Option<Value>>> {
    let p = ctx.jit_ctx_ptr();
    if p == 0 { return None; }
    // runtime-jit-tiering Phase 1.5 safety: never marshal a `Ref(Stack)` (out/ref
    // address, which only an INTERP frame can hold) into native code — see
    // `exec_call::try_native_static_call` for the full rationale. Guard receiver + args.
    if matches!(receiver, Value::Ref { .. })
        || args.iter().any(|&r| matches!(frame.regs.get(r as usize), Some(Value::Ref { .. }))) {
        return None;
    }
    let jit_ctx = p as *const crate::jit::frame::JitModuleCtx;
    let (max_reg, ptr, name, file) = {
        let entry = unsafe { (*jit_ctx).resolve_fn_by_id_tiered(idx) }?;
        (entry.max_reg, entry.ptr, entry.name.clone(), entry.file.clone())
    };
    ctx.counters().jit_native_from_interp.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut callee = crate::jit::frame::JitFrame::new_method_args_from(
        max_reg, receiver.clone(), &frame.regs, args);
    let jit_fn: crate::jit::helpers::JitFn = unsafe { std::mem::transmute(ptr) };
    ctx.push_frame(crate::exception::VmFrame::new(
        name, file, &callee.regs as *const _, &callee.env_arena as *const _));
    let r = unsafe { jit_fn(&mut callee, jit_ctx) };
    ctx.pop_frame();
    if r != 0 {
        callee.recycle();
        return Some(Ok(Some(ctx.take_exception().unwrap_or(Value::Null))));
    }
    let ret = callee.ret.take().unwrap_or(Value::Null);
    callee.recycle();
    frame.set(dst, ret);
    Some(Ok(None))
}

#[cfg(not(feature = "jit"))]
#[inline]
fn try_native_method_call(
    _ctx: &VmContext, _frame: &mut Frame, _dst: u32, _idx: usize,
    _receiver: &Value, _args: &[u32],
) -> Option<Result<Option<Value>>> {
    None
}

/// L3-G4b primitive-as-struct: maps a primitive `Value` variant to its stdlib
/// struct's qualified class name (e.g. `Value::I64` → `"Std.Int32"`). The VM
/// dispatches primitive method calls by constructing `{class}.{method}` and
/// looking up the function in `module.func_index` — replacing the old
/// hardcoded `(Value, method)` → builtin-name switch.
///
/// Returns None for non-primitive values (objects, null, etc.).
pub(crate) fn primitive_class_name(obj: &Value) -> Option<&'static str> {
    use crate::metadata::well_known_names::*;
    match obj {
        // rename-primitives-to-pascal-case (2026-05-24): VM dispatch on
        // Value::I64 routes to Std.Int32 by default (narrow int / long values
        // are tagged with class FQN at compile-time in VCall instructions).
        Value::I64(_)  => Some(STD_INT32),
        Value::F64(_)  => Some(STD_DOUBLE),
        Value::Bool(_) => Some(STD_BOOLEAN),
        Value::Char(_) => Some(STD_CHAR),
        Value::Str(_)  => Some(STD_STRING),
        // 2026-05-07 add-array-base-class: T[] dispatches to Std.Array methods
        // (Clone / GetType / ToString / Equals / GetHashCode). The lookup path
        // tries `Std.Array.<method>` first, then falls through to base
        // `Std.Object.<method>` via the primitive overload retry logic.
        Value::Array(_) => Some(STD_ARRAY),
        _ => None,
    }
}

/// refactor-vcall-ic-primitives (2026-05-17): synthetic TypeId for IC keying.
/// Primitives don't have a real `TypeDesc.id` (they're built-in runtime values,
/// not user-defined classes). Returning a stable `PRIM_TYPE_*` lets `VCallIC`
/// cache them with the same `cached_type_id` mechanism used for object
/// receivers — no extra slot, no separate cache path.
///
/// Returns None for objects (which use real `type_desc.id.0`) and `Value::Null`.
#[inline]
pub(crate) fn value_synthetic_type_id(obj: &Value) -> Option<u32> {
    use crate::metadata::tokens::*;
    match obj {
        Value::I64(_)   => Some(PRIM_TYPE_I64),
        Value::F64(_)   => Some(PRIM_TYPE_F64),
        Value::Bool(_)  => Some(PRIM_TYPE_BOOL),
        Value::Char(_)  => Some(PRIM_TYPE_CHAR),
        Value::Str(_)   => Some(PRIM_TYPE_STR),
        Value::Array(_) => Some(PRIM_TYPE_ARRAY),
        _ => None,
    }
}

/// 2026-05-07 add-array-base-class: hardcoded is-a check for `Value::Array`.
/// Class name comparison accepts both unqualified and `Std.`-qualified forms
/// because IR-emitted class names depend on TypeChecker's qualification path
/// (imported classes use FQ; bare references unqualified).
pub(super) fn is_array_isa(class_name: &str) -> bool {
    matches!(class_name, "Array" | "Object" | "Std.Array" | "Std.Object")
}

pub(super) fn vcall(
    ctx: &VmContext, module: &Module, frame: &mut Frame,
    dst: u32, obj: u32, method: &str, args: &[u32],
    // vcall_ic: per-site polymorphic inline cache (TypeId, vtable slot, MethodId),
    // populated by `resolve_vcall` on a miss; hits go straight to the callee.
    vcall_ic: Option<&crate::metadata::resolver::VCallIC>,
    // add-generic-methods: resolved FQ type-arg names for a generic instance-method
    // call (empty for non-generic). Threaded into the callee frame's method_type_args.
    method_type_args: &[String],
) -> Result<Option<Value>> {
    // add-generic-activator: resolve method-type-arg forwarding markers `$mta:N`
    // against the caller frame before threading to the callee (see exec_call::call
    // and interp::resolve_forwarded_mta). No alloc unless a marker is present.
    let fwd_storage;
    let method_type_args: &[String] = if method_type_args.iter().any(|s| s.starts_with("$mta:")) {
        fwd_storage = super::resolve_forwarded_mta(frame, method_type_args);
        &fwd_storage
    } else {
        method_type_args
    };

    let obj_val = frame.get(obj)?.clone();

    // ── PIC hit: straight to the module-local callee (no name work at all) ──────────
    if let Some(idx) = vcall_ic_hit(vcall_ic, &obj_val) {
        return invoke_local(ctx, module, frame, dst, idx, &obj_val, args, method_type_args);
    }

    // ── PIC miss: shared resolution ladder (installs the PIC for next time) ─────────
    // perf-vm-iteration Phase 1 (Decision 3): no `collect_args` on any path — every
    // target below is invoked by filling the callee frame directly from the caller's
    // registers (`exec_function_from_receiver_regs`), zero args Vec.
    let resolved = resolve_vcall(ctx, module, &obj_val, method, args.len(), vcall_ic)?;
    match resolved.target {
        VCallTarget::Immediate(v) => { frame.set(dst, v); Ok(None) }
        VCallTarget::Local(idx) =>
            invoke_local(ctx, module, frame, dst, idx, &resolved.this, args, method_type_args),
        VCallTarget::Lazy(f) => {
            let outcome = super::exec_function_from_receiver_regs(
                ctx, module, f.as_ref(), &resolved.this, &frame.regs, args, method_type_args)?;
            finish(frame, dst, outcome)
        }
    }
}

/// Run `module.functions[idx]` with `this` in reg 0: an already-compiled method goes
/// native (mixed-mode divert; generic instance calls stay on interp because the native
/// path does not thread `method_type_args` yet), otherwise interp.
fn invoke_local(
    ctx: &VmContext, module: &Module, frame: &mut Frame, dst: u32, idx: usize,
    this: &Value, args: &[u32], method_type_args: &[String],
) -> Result<Option<Value>> {
    if method_type_args.is_empty() {
        if let Some(res) = try_native_method_call(ctx, frame, dst, idx, this, args) {
            return res;
        }
    }
    let callee = match module.functions.get(idx) {
        Some(f) => f,
        None => bail!("VCall: resolved function index {} out of range", idx),
    };
    let outcome = super::exec_function_from_receiver_regs(
        ctx, module, callee, this, &frame.regs, args, method_type_args)?;
    finish(frame, dst, outcome)
}

#[inline]
fn finish(frame: &mut Frame, dst: u32, outcome: ExecOutcome) -> Result<Option<Value>> {
    match outcome {
        ExecOutcome::Returned(ret) => { frame.set(dst, ret.unwrap_or(Value::Null)); Ok(None) }
        ExecOutcome::Thrown(val) => Ok(Some(val)),
    }
}
