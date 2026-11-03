/// Virtual dispatch (`VCall`): the interpreter's **invoke** side.
///
/// unify-vcall-resolution : *what* to call is decided by
/// `vcall_resolve::resolve_vcall` (shared with `jit/helpers/vcall.rs::jit_vcall`);
/// this file only keeps the interpreter-specific pieces — the receiver-kind
/// helpers (`primitive_class_name` / `value_synthetic_type_id`),
/// the mixed-mode divert to an already-compiled native method, and running the
/// resolved target on an interpreter frame.

use crate::vm::runtime::{Function, Module};
use crate::object::Value;
use crate::vm_context::VmContext;
use anyhow::Result;

use crate::vm::exec::ops::vcall::resolve_vcall;
use crate::vm::runtime::dispatch::vcall::vcall_ic_hit;
use crate::vm::exec::ops::vcall::VCallTarget;
use super::{ExecOutcome, Frame};

/// runtime-jit-tiering Phase 1.5 (mixed-mode): receiver-aware analogue of
/// `exec_call::try_native_static_call`. Counts method `id` (its JIT id / `FnId`)
/// toward its tier-up; once it is JIT-compiled, invoke it natively (receiver in reg 0 via `new_method_args_from`) and marshal the
/// result. `None` → no JIT installed / cold / untranslatable → caller stays interp.
#[cfg(feature = "jit")]
fn try_native_method_call(
    ctx: &VmContext, frame: &mut Frame, dst: u32, id: usize,
    receiver: &Value, args: &[u32],
) -> Option<Result<Option<Value>>> {
    let rt = ctx.jit()?;
    // runtime-jit-tiering Phase 1.5 safety: never marshal a `Ref(Stack)` (out/ref
    // address, which only an INTERP frame can hold) into native code — see
    // `exec_call::try_native_static_call` for the full rationale. Guard receiver + args.
    if matches!(receiver, Value::Ref { .. })
        || args.iter().any(|&r| matches!(frame.regs.get(r as usize), Some(Value::Ref { .. }))) {
        return None;
    }
    let (max_reg, ptr, callee_fn) = {
        let entry = rt.resolve_fn_by_id_tiered(ctx, id)?;
        (entry.max_reg, entry.ptr, entry.func)
    };
    ctx.counters().jit_native_from_interp.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let callee = crate::jit::frame::JitFrame::new_method_args_from(
        ctx, max_reg, receiver.clone(), &frame.regs, args);
    let outcome = unsafe { crate::jit::invoke::enter_compiled(ctx, ptr, callee_fn, callee) };
    Some(Ok(super::exec_call::native_result_to_dst(ctx, frame, dst, outcome)))
}

#[cfg(not(feature = "jit"))]
#[inline]
fn try_native_method_call(
    _ctx: &VmContext, _frame: &mut Frame, _dst: u32, _id: usize,
    _receiver: &Value, _args: &[u32],
) -> Option<Result<Option<Value>>> {
    None
}

pub(crate) fn vcall(
    ctx: &VmContext, module: &Module, frame: &mut Frame,
    dst: u32, obj: u32, method: &str, args: &[u32],
    // vcall_ic: per-site polymorphic inline cache (TypeId → callee FnId), populated by
    // `resolve_vcall` on a miss; hits go straight to the callee.
    vcall_ic: Option<&crate::vm::runtime::dispatch::ic::VCallIC>,
    // add-generic-methods: resolved FQ type-arg names for a generic instance-method
    // call (empty for non-generic). Threaded into the callee frame's method_type_args.
    method_type_args: &[String],
) -> Result<Option<Value>> {
    // add-generic-activator: resolve method-type-arg forwarding markers `$mta:N`
    // against the caller frame before threading to the callee (see exec_call::call
    // and interp::resolve_forwarded_mta). No alloc unless a marker is present.
    let fwd_storage;
    let method_type_args: &[String] = if super::has_forwarded_mta(method_type_args) {
        fwd_storage = super::resolve_forwarded_mta(ctx, frame, method_type_args);
        &fwd_storage
    } else {
        method_type_args
    };

    let obj_val = frame.get(obj)?.clone();

    // ── PIC hit: straight to the callee by `FnId` (no name work at all) ─────────────
    // Module-local and lazily loaded callees alike (P1-2 PR 4). `fn_by_id` is `None` only
    // while another thread's registration of the cached id is not yet visible here (see
    // `vcall_ic_hit`) — then the slow path below resolves by name, as on a miss.
    if let Some(id) = vcall_ic_hit(vcall_ic, &obj_val) {
        if let Some(func) = crate::vm::runtime::dispatch::functions::fn_by_id((ctx).funcs(), module, id as u32) {
            crate::vm::exec::ops::vcall::assert_pic_target(ctx, module, &obj_val, method, func);
            return invoke(ctx, module, frame, dst, func, Some(id), &obj_val, args, method_type_args);
        }
    }

    // ── PIC miss: shared resolution ladder (installs the PIC for next time) ─────────
    // perf-vm-iteration Phase 1 (Decision 3): no `collect_args` on any path — every
    // target below is invoked by filling the callee frame directly from the caller's
    // registers (`exec_function_from_receiver_regs`), zero args Vec.
    let resolved = resolve_vcall(ctx, module, &obj_val, method, args.len(), vcall_ic)?;
    match resolved.target {
        VCallTarget::Immediate(v) => { frame.set(dst, v); Ok(None) }
        // fix-call-arity-skew: signature mismatch under the site's key → catchable throw.
        VCallTarget::Thrown(exc) => Ok(Some(exc)),
        VCallTarget::Local { func, id } =>
            invoke(ctx, module, frame, dst, func, id, &resolved.this, args, method_type_args),
    }
}

/// Run `func` with `this` in reg 0. With a `FnId` (module-local and lazily loaded callees
/// alike) it counts toward its tier-up and goes native once compiled (mixed-mode divert) —
/// a generic call too: a callee that could read its `method_type_args` never compiles
/// (`jit::translate::unsupported`'s invariant), so a compiled one does not need them in
/// its frame. Otherwise, or while cold / untranslatable, interp (type args threaded).
#[allow(clippy::too_many_arguments)]
fn invoke(
    ctx: &VmContext, module: &Module, frame: &mut Frame, dst: u32,
    func: &Function, id: Option<usize>,
    this: &Value, args: &[u32], method_type_args: &[String],
) -> Result<Option<Value>> {
    if method_type_args.is_empty() {
        // runtime-audit P1-8: a `[Native]` forwarder (`string.Length`, `s[i]`) runs its
        // builtin directly, the receiver passed alongside the registers.
        if let Some(res) = super::exec_call::try_fold(ctx, module, frame, dst, func, Some(*this), args) {
            return res;
        }
    }
    if let Some(id) = id {
        if let Some(res) = try_native_method_call(ctx, frame, dst, id, this, args) {
            return res;
        }
    }
    let outcome = super::exec_function_from_receiver_regs(
        ctx, module, func, this, &frame.regs, args, method_type_args)?;
    finish(frame, dst, outcome)
}

#[inline]
fn finish(frame: &mut Frame, dst: u32, outcome: ExecOutcome) -> Result<Option<Value>> {
    match outcome {
        ExecOutcome::Returned(ret) => { frame.set(dst, ret.unwrap_or(Value::Null)); Ok(None) }
        ExecOutcome::Thrown(val) => Ok(Some(val)),
    }
}
