#![allow(dangerous_implicit_autorefs)]
//! L3 closure JIT helpers — `LoadFn` / `MkClos` / `CallIndirect`.
//!
//! Behaviour mirrors `interp::exec_call` / `exec_instr` (impl-closure-l3-core);
//! see `docs/internals/src/runtime/escape-analysis.md`（闭包栈分配）+ `docs/spec/archive/2026-05-02-impl-closure-l3-jit-complete/`.
//!
//! Convention follows the rest of `jit/helpers/`:
//!   • Every helper takes `frame: *mut JitFrame, ctx: *const JitModuleCtx` first.
//!   • Returns `u8`: 0 on success, 1 on exception (set via `set_exception`).
//!   • Strings / register-index slices are passed as `(ptr, len)` pairs whose
//!     storage lives inside the `Module` bytecode (lifetime ≥ JitModule).

use crate::metadata::Value;

use super::super::frame::{FnEntry, JitFrame, JitModuleCtx};
use super::super::invoke::call_entry;
use super::{set_exception, vm_ctx_ref};

// ── LoadFn ────────────────────────────────────────────────────────────────────

/// Push `Value::FuncRef(name)` into `frame.regs[dst]`. No-capture lambdas /
/// local fns lower to this. See closure.md §6 + L3-C-2.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_load_fn(
    frame: *mut JitFrame, _ctx: *const JitModuleCtx,
    dst: u32,
    name_ptr: *const u8, name_len: usize,
) -> u8 {
    let name = super::baked_str(name_ptr, name_len);
    (*frame).regs[dst as usize] = Value::FuncRef(name.into());
    0
}

// ── MkClos ────────────────────────────────────────────────────────────────────

/// Allocate a heap env from `captures` registers and write a heap
/// `Value::Closure` to `frame.regs[dst]`. Mirrors `interp::exec_call::mk_clos`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_mk_clos(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32,
    name_ptr: *const u8, name_len: usize,
    caps_ptr: *const u32, caps_len: usize,
) -> u8 {
    // Codegen-baked module string (`TxCtx::str_val`): no per-closure UTF-8 re-check,
    // and no intermediate `String` — the heap path copies it straight into a GC `Str`.
    let name = super::baked_str(name_ptr, name_len);
    let frame_ref = &mut *frame;
    let cap_regs  = std::slice::from_raw_parts(caps_ptr, caps_len);
    let env_vec: Vec<Value> = cap_regs.iter()
        .map(|&r| frame_ref.regs[r as usize].clone())
        .collect();

    // Allocate env via the GC heap so it's tracked as a managed array.
    let env_val = vm_ctx_ref(ctx).heap().alloc_array(env_vec);
    let env = match env_val {
        Value::Array(rc) => rc,
        _ => unreachable!("alloc_array must return Value::Array"),
    };
    // unify-gc-heap PR-2: ClosureData into the GC variable-length region.
    // PR-5: fn_name is a GC `Str`, allocated from the same heap as `env`.
    let fn_name = vm_ctx_ref(ctx).intern_fn_name(name);
    let value = vm_ctx_ref(ctx).heap().alloc_closure(crate::metadata::ClosureData {
        env,
        fn_name,
    });
    frame_ref.regs[dst as usize] = value;
    0
}

// ── CallIndirect ──────────────────────────────────────────────────────────────

/// Invoke whatever callable lives in `frame.regs[callee]`:
///   • `Value::FuncRef(name)` → static call (parameters as-is)
///   • `Value::Closure { env, fn_name }` → prepend env as implicit first arg
/// Anything else → exception. See closure.md §6 + L3-C-6.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_call_indirect(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, callee: u32,
    args_ptr: *const u32, args_len: usize,
    caller_offset: u32, // packed code offset of the call site (jit-stack-trace)
) -> u8 {
    let frame_ref = &mut *frame;
    let ctx_ref   = &*ctx;
    let vm_ctx    = vm_ctx_ref(ctx);

    // 1) Resolve callee Value → (fn_name, optional env-as-Vec).
    // JIT-S3 (perf, mirrors interp exec_call S3): `Value::Closure` 直接复用已有
    // env GcRef（Arc +1），不再 `to_boxed_vec()` 深拷 + `alloc_array` 重分配。
    // 安全性同 interp S3：env 数组 MkClos 时写一次、体内只 array_get 读（编译器
    // _emitAssign 无 BoundCapturedIdent 写回分支）→ 跨调用共享 GcRef 字节等价。
    // The name is borrowed from the callee value (a `Str` handle copy, no `String`
    // alloc per call): the closure stays rooted in the caller's `callee` register for
    // the whole call, so its name block outlives every use below.
    let (fn_name, env_val_opt): (crate::metadata::vstr::Str, Option<Value>) = match &frame_ref.regs[callee as usize] {
        Value::FuncRef(n) => (*n, None),
        Value::Closure(c) => {
            let data = crate::metadata::types::closure_data_of(c);
            (data.fn_name, Some(Value::Array(data.env.clone())))
        }
        // fix-null-delegate-invoke: same as interp's `call_indirect` — a null callee is
        // reachable from ordinary code (an unassigned single-cast `event` field), so it
        // gets a catchable `Std.NullReferenceException` rather than a raw string. Both
        // backends must do this: fixing only one leaves the bug alive under the other.
        Value::Null => {
            let module = &*(*ctx).module;
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

    let fn_name: &str = &fn_name;
    let user_regs = std::slice::from_raw_parts(args_ptr, args_len);

    // 2) Resolve the callee. runtime-jit-tiering Phase 1b: tiered — a cold
    //    (below-threshold) or interp-only lambda resolves to None and is run on the
    //    interpreter with the already-assembled `args` (env prepended for closures,
    //    exactly as the native path receives it). At the threshold it compiles and
    //    subsequent indirect calls take the native path.
    let entry: &FnEntry = match ctx_ref.resolve_fn_by_name_tiered(fn_name) {
        Some(e) => e,
        None => {
            // Cold path only: the interpreter takes the args as one `Vec` (env first).
            let mut args: Vec<Value> = Vec::with_capacity(args_len + env_val_opt.is_some() as usize);
            args.extend(env_val_opt);
            args.extend(user_regs.iter().map(|&r| frame_ref.regs[r as usize].clone()));
            vm_ctx.set_top_frame_pc(caller_offset);
            let module = &*ctx_ref.module;
            let outcome = if let Some(callee) = module.func_index.get(fn_name)
                .and_then(|&idx| module.functions.get(idx))
            {
                crate::interp::exec_function(vm_ctx, module, callee, &args)
            } else if let Some(lazy_fn) = vm_ctx.try_lookup_function(fn_name) {
                crate::interp::exec_function(vm_ctx, module, lazy_fn.as_ref(), &args)
            } else {
                set_exception(vm_ctx,
                    Value::Str(format!("CallIndirect: undefined function `{}`", fn_name).into()));
                return 1;
            };
            return match outcome {
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
    //    and run it (GC-root enrolment + trace row in `call_native`).
    let callee_frame = match env_val_opt {
        Some(env) => JitFrame::new_method_args_from(vm_ctx, entry.max_reg, env, &frame_ref.regs, user_regs),
        None => JitFrame::new_args_from(vm_ctx, entry.max_reg, &frame_ref.regs, user_regs),
    };
    vm_ctx.set_top_frame_pc(caller_offset);
    call_entry(vm_ctx, ctx, entry, callee_frame).store_into(&mut frame_ref.regs, dst)
}
