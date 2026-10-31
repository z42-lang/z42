#![allow(dangerous_implicit_autorefs)]
//! L3 closure JIT helpers — `LoadFn` / `MkClos` / `CallIndirect`.
//!
//! Behaviour mirrors `interp::exec_call` / `exec_instr` (impl-closure-l3-core);
//! see `docs/internals/src/runtime/escape-analysis.md`（闭包栈分配）+ `变更 impl-closure-l3-jit-complete/`.
//!
//! Convention follows the rest of `jit/helpers/`:
//!   • Every helper takes `frame: *mut JitFrame, ctx: *const VmContext` first.
//!   • Returns `u8`: 0 on success, 1 on exception (set via `set_exception`).
//!   • Strings / register-index slices are passed as `(ptr, len)` pairs whose
//!     storage lives inside the `Module` bytecode (lifetime ≥ the compiled code).
//!   • P1-2 PR 8: `LoadFn` / `MkClos` bind the target's `FnId` when the value is created
//!     (site token, shared with the interpreter); `CallIndirect` dispatches on that id
//!     through the slot table — no name on the call path.

use crate::metadata::tokens::FnId;
use crate::metadata::Value;

use super::super::frame::{FnEntry, JitFrame};
use crate::vm_context::VmContext;
use super::super::invoke::call_entry;
use super::{set_exception, vm_ctx_ref};

// ── LoadFn ────────────────────────────────────────────────────────────────────

/// Bind a function reference's target (P1-2 PR 8): the site token when bound, else
/// `interp::exec_call::bind_fn_ref` (loads the target's package if needed and stores the
/// token). `tok` is the site's `ResolvedTokens::fn_ref_tokens` cell, or null when the
/// function was compiled without a token table (then binding runs on every execution).
/// `Err(())` = the exception is already set.
#[inline]
unsafe fn bind_target(
    ctx: *const VmContext, name_ptr: *const u8, name_len: usize,
    tok: *const std::sync::atomic::AtomicU32,
) -> Result<FnId, ()> {
    let vm_ctx = vm_ctx_ref(ctx);
    let module = super::jit_module(ctx);
    let name = super::baked_str(name_ptr, name_len);
    crate::interp::fn_ref_id(vm_ctx, module, name, tok.as_ref())
        .map_err(|exc| set_exception(vm_ctx, exc))
}

/// `frame.regs[dst] = FuncRef(target)`. No-capture lambdas / free-function method groups
/// lower to this; a site whose token is already bound when the function is translated
/// stores the `FuncRef` inline instead (`translate::call`). Returns 1 when the target
/// cannot be bound (exception set). See closure.md §6 + L3-C-2.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_load_fn(
    frame: *mut JitFrame, ctx: *const VmContext,
    dst: u32,
    name_ptr: *const u8, name_len: usize,
    tok: *const std::sync::atomic::AtomicU32,
) -> u8 {
    match bind_target(ctx, name_ptr, name_len, tok) {
        Ok(id) => { (*frame).regs[dst as usize] = Value::FuncRef(id); 0 }
        Err(()) => 1,
    }
}

// ── MkClos ────────────────────────────────────────────────────────────────────

/// Allocate a heap env from `captures` registers and write a heap
/// `Value::Closure` to `frame.regs[dst]`. Mirrors `interp::exec_call::mk_clos`.
/// `baked` is the target id when the site was already bound at translate time
/// (`UNRESOLVED` otherwise → [`bind_target`]).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_mk_clos(
    frame: *mut JitFrame, ctx: *const VmContext,
    dst: u32,
    baked: u32,
    name_ptr: *const u8, name_len: usize,
    tok: *const std::sync::atomic::AtomicU32,
    caps_ptr: *const u32, caps_len: usize,
) -> u8 {
    let fn_id = if baked != crate::metadata::tokens::UNRESOLVED {
        FnId(baked)
    } else {
        match bind_target(ctx, name_ptr, name_len, tok) {
            Ok(id) => id,
            Err(()) => return 1,
        }
    };
    let frame_ref = &mut *frame;
    let cap_regs  = std::slice::from_raw_parts(caps_ptr, caps_len);
    let env_vec: Vec<Value> = cap_regs.iter()
        .map(|&r| frame_ref.regs[r as usize].clone())
        .collect();

    // Allocate env via the GC heap so it's tracked as a managed array.
    let env_val = crate::metadata::types::ArrayObj::alloc_frozen(vm_ctx_ref(ctx).heap(), &env_vec);
    let env = match env_val {
        Value::Array(rc) => rc,
        _ => unreachable!("alloc_array must return Value::Array"),
    };
    // unify-gc-heap PR-2: ClosureData into the GC variable-length region.
    let value = vm_ctx_ref(ctx).heap().alloc_closure(crate::metadata::ClosureData { env, fn_id });
    frame_ref.regs[dst as usize] = value;
    0
}

// ── CallIndirect ──────────────────────────────────────────────────────────────

/// Invoke whatever callable lives in `frame.regs[callee]`:
///   • `Value::FuncRef(id)` → static call (parameters as-is)
///   • `Value::Closure { env, fn_id }` → prepend env as implicit first arg
/// Anything else → exception. See closure.md §6 + L3-C-6.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_call_indirect(
    frame: *mut JitFrame, ctx: *const VmContext,
    dst: u32, callee: u32,
    args_ptr: *const u32, args_len: usize,
    caller_offset: u32, // packed code offset of the call site (jit-stack-trace)
) -> u8 {
    let frame_ref = &mut *frame;
    let rt        = super::jit_rt(ctx);
    let vm_ctx    = vm_ctx_ref(ctx);

    // 1) Callee Value → (target id, optional env).
    // JIT-S3 (perf, mirrors interp exec_call S3): `Value::Closure` 直接复用已有
    // env ArrRef（Arc +1），不再 `to_boxed_vec()` 深拷 + `alloc_array` 重分配。
    // 安全性同 interp S3：env 数组 MkClos 时写一次、体内只 array_get 读（编译器
    // _emitAssign 无 BoundCapturedIdent 写回分支）→ 跨调用共享 ArrRef 字节等价。
    // P1-2 PR 8: the value carries its target's id (bound at creation) — a JIT id, the
    // index of the JIT runtime's slot table, for entry and lazily loaded targets alike.
    let (id, env_val_opt): (FnId, Option<Value>) = match &frame_ref.regs[callee as usize] {
        Value::FuncRef(id) => (*id, None),
        Value::Closure(c) => {
            let data = crate::metadata::types::closure_data_of(c);
            (data.fn_id, Some(Value::Array(data.env.clone())))
        }
        // fix-null-delegate-invoke: same as interp's `call_indirect` — a null callee is
        // reachable from ordinary code (an unassigned single-cast `event` field), so it
        // gets a catchable `Std.NullReferenceException` rather than a raw string. Both
        // backends must do this: fixing only one leaves the bug alive under the other.
        Value::Null => {
            let module = super::jit_module(ctx);
            let exc = crate::exception::make_stdlib_exception(
                vm_ctx, module, crate::semantics::NULL_REF_EXC,
                crate::semantics::null_invoke_msg(),
            ).unwrap_or_else(|e| Value::Str(format!("{e}").into()));
            set_exception(vm_ctx, exc);
            return 1;
        }
        other => {
            set_exception(vm_ctx, Value::Str(format!(
                "CallIndirect: expected FuncRef / Closure, got {:?}", other).into()));
            return 1;
        }
    };

    let user_regs = std::slice::from_raw_parts(args_ptr, args_len);

    // 2) The slot table, by id: no hashing, no name. runtime-jit-tiering Phase 1b: tiered —
    //    a cold (below-threshold) or interp-only target resolves to None and is run on the
    //    interpreter with the already-assembled `args` (env prepended for closures, exactly
    //    as the native path receives it). At the threshold it compiles and subsequent
    //    indirect calls take the native path.
    let entry: &FnEntry = match rt.resolve_fn_by_id_tiered(&*ctx, id.0 as usize) {
        Some(e) => e,
        None => {
            let module = super::jit_module(ctx);
            let Some(callee) = rt.fn_of(id.0 as usize) else {
                set_exception(vm_ctx, Value::Str(format!(
                    "CallIndirect: function id {} is not registered in this VM", id.0).into()));
                return 1;
            };
            // Cold path only: the interpreter takes the args as one `Vec` (env first).
            let mut args: Vec<Value> = Vec::with_capacity(args_len + env_val_opt.is_some() as usize);
            args.extend(env_val_opt);
            args.extend(user_regs.iter().map(|&r| frame_ref.regs[r as usize].clone()));
            vm_ctx.set_top_frame_pc(caller_offset);
            return match crate::interp::exec_function(vm_ctx, module, callee, &args) {
                Ok(crate::interp::ExecOutcome::Returned(ret)) => {
                    frame_ref.regs[dst as usize] = ret.unwrap_or(Value::Null); 0
                }
                Ok(crate::interp::ExecOutcome::Thrown(val)) => { set_exception(vm_ctx, val); 1 }
                Err(e) => { set_exception(vm_ctx, Value::Str(e.to_string().into())); 1 }
            };
        }
    };

    // 3) Build the callee frame straight from the caller's registers (env, when a
    //    closure was invoked, is the implicit first parameter — reg 0 like a receiver)
    //    and run it (GC-root enrolment + trace row in `enter_compiled`).
    let callee_frame = match env_val_opt {
        Some(env) => JitFrame::new_method_args_from(vm_ctx, entry.max_reg, env, &frame_ref.regs, user_regs),
        None => JitFrame::new_args_from(vm_ctx, entry.max_reg, &frame_ref.regs, user_regs),
    };
    vm_ctx.set_top_frame_pc(caller_offset);
    call_entry(vm_ctx, entry, callee_frame).store_into(&mut frame_ref.regs, dst)
}

#[cfg(test)]
#[path = "closure_tests.rs"]
mod closure_tests;
