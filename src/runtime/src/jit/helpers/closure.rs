#![allow(dangerous_implicit_autorefs)]
//! L3 closure JIT helpers — `LoadFn` / `LoadFnCached` / `MkClos` / `CallIndirect`.
//!
//! Behaviour mirrors `interp::exec_call` / `exec_instr` (impl-closure-l3-core);
//! see `docs/design/language/closure.md` §6 + `docs/spec/archive/2026-05-02-impl-closure-l3-jit-complete/`.
//!
//! Convention follows the rest of `jit/helpers/`:
//!   • Every helper takes `frame: *mut JitFrame, ctx: *const JitModuleCtx` first.
//!   • Returns `u8`: 0 on success, 1 on exception (set via `set_exception`).
//!   • Strings / register-index slices are passed as `(ptr, len)` pairs whose
//!     storage lives inside the `Module` bytecode (lifetime ≥ JitModule).

use crate::metadata::Value;

use super::super::frame::{FnEntry, JitFrame, JitModuleCtx};
use super::{set_exception, vm_ctx_ref, JitFn};

// ── LoadFn ────────────────────────────────────────────────────────────────────

/// Push `Value::FuncRef(name)` into `frame.regs[dst]`. No-capture lambdas /
/// local fns lower to this. See closure.md §6 + L3-C-2.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_load_fn(
    frame: *mut JitFrame, _ctx: *const JitModuleCtx,
    dst: u32,
    name_ptr: *const u8, name_len: usize,
) -> u8 {
    let name = std::str::from_utf8(std::slice::from_raw_parts(name_ptr, name_len))
        .unwrap_or("<invalid>");
    (*frame).regs[dst as usize] = Value::FuncRef(name.into());
    0
}

// ── LoadFnCached (D1b add-method-group-conversion) ───────────────────────────

/// 2026-05-02 D1b: cached method group conversion. First execution constructs
/// `Value::FuncRef(name)` and stores it into `vm_ctx.func_ref_slots[slot_id]`;
/// subsequent hits read from slot. Slot allocation guaranteed by `Vm::run`
/// calling `alloc_func_ref_slots` before entry.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_load_fn_cached(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32,
    name_ptr: *const u8, name_len: usize,
    slot_id: u32,
) -> u8 {
    let vm_ctx = vm_ctx_ref(ctx);
    let cached = vm_ctx.func_ref_slot(slot_id);
    let value = if matches!(cached, Value::Null) {
        let name = std::str::from_utf8(std::slice::from_raw_parts(name_ptr, name_len))
            .unwrap_or("<invalid>");
        let v = Value::FuncRef(name.into());
        vm_ctx.set_func_ref_slot(slot_id, v.clone());
        v
    } else {
        cached
    };
    (*frame).regs[dst as usize] = value;
    0
}

// ── MkClos ────────────────────────────────────────────────────────────────────

/// Allocate an env from `captures` registers and write a closure value
/// to `frame.regs[dst]`. `stack_alloc` 决定走 frame-local arena
/// (`Value::StackClosure`) 还是 heap (`Value::Closure`)。详见 closure.md §6
/// + impl-closure-l3-escape-stack。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_mk_clos(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32,
    name_ptr: *const u8, name_len: usize,
    caps_ptr: *const u32, caps_len: usize,
    stack_alloc: u8,
) -> u8 {
    let name = std::str::from_utf8(std::slice::from_raw_parts(name_ptr, name_len))
        .unwrap_or("<invalid>")
        .to_string();
    let frame_ref = &mut *frame;
    let cap_regs  = std::slice::from_raw_parts(caps_ptr, caps_len);
    let env_vec: Vec<Value> = cap_regs.iter()
        .map(|&r| frame_ref.regs[r as usize].clone())
        .collect();

    let value = if stack_alloc != 0 {
        let idx = frame_ref.env_arena.len() as u32;
        frame_ref.env_arena.push(env_vec);
        Value::StackClosure(Box::new(crate::metadata::StackClosureData {
            env_idx: idx,
            fn_name: name,
        }))
    } else {
        // Allocate env via the GC heap so it's tracked as a managed array.
        let env_val = vm_ctx_ref(ctx).heap().alloc_array(env_vec);
        let env = match env_val {
            Value::Array(rc) => rc,
            _ => unreachable!("alloc_array must return Value::Array"),
        };
        // unify-gc-heap PR-2: ClosureData into the GC variable-length region.
        vm_ctx_ref(ctx).heap().alloc_closure(crate::metadata::ClosureData {
            env,
            fn_name: name,
        })
    };
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
    caller_line: u32,   // 2026-05-10 jit-stack-trace
    caller_col:  u32,   // 2026-05-10 span-column-propagate
    caller_offset: u32, // add-offline-symbolication: linearized code offset
) -> u8 {
    let frame_ref = &mut *frame;
    let ctx_ref   = &*ctx;
    let vm_ctx    = vm_ctx_ref(ctx);

    // 1) Resolve callee Value → (fn_name, optional env-as-Vec).
    //    Stack closure 从 caller frame.env_arena 复制内容；callee 内统一拿到
    //    一个新 GcRef Array（避免 callee 持有指向 caller arena 的 lifetime）。
    // JIT-S3 (perf, mirrors interp exec_call S3): `Value::Closure` 直接复用已有
    // env GcRef（Arc +1），不再 `to_boxed_vec()` 深拷 + `alloc_array` 重分配。
    // 安全性同 interp S3：env 数组 MkClos 时写一次、体内只 array_get 读（编译器
    // _emitAssign 无 BoundCapturedIdent 写回分支）→ 跨调用共享 GcRef 字节等价。
    // StackClosure 仍需物化（arena 持裸 Vec，callee lifetime 需独立）。
    let (fn_name, env_val_opt): (String, Option<Value>) = match &frame_ref.regs[callee as usize] {
        Value::FuncRef(n) => (n.to_string(), None),
        Value::Closure(c) => {
            let data = crate::metadata::types::closure_data_of(c);
            (data.fn_name.clone(), Some(Value::Array(data.env.clone())))
        }
        Value::StackClosure(sc) => {
            let idx = sc.env_idx as usize;
            if idx >= frame_ref.env_arena.len() {
                set_exception(vm_ctx, Value::Str(format!(
                    "CallIndirect: stack closure env_idx {} out of bounds (arena_len={})",
                    idx, frame_ref.env_arena.len()).into()));
                return 1;
            }
            (sc.fn_name.clone(), Some(vm_ctx.heap().alloc_array(frame_ref.env_arena[idx].clone())))
        }
        other => {
            set_exception(vm_ctx, Value::Str(format!(
                "CallIndirect: expected FuncRef / Closure / StackClosure, got {:?}", other).into()));
            return 1;
        }
    };

    // 2) Gather args, prepending env when a closure was invoked.
    let user_regs = std::slice::from_raw_parts(args_ptr, args_len);
    let mut args: Vec<Value> = Vec::with_capacity(args_len + env_val_opt.is_some() as usize);
    if let Some(env_val) = env_val_opt {
        args.push(env_val);
    }
    for &r in user_regs {
        args.push(frame_ref.regs[r as usize].clone());
    }

    // 3) Resolve the callee. runtime-jit-tiering Phase 1b: tiered — a cold
    //    (below-threshold) or interp-only lambda resolves to None and is run on the
    //    interpreter with the already-assembled `args` (env prepended for closures,
    //    exactly as the native path receives it). At the threshold it compiles and
    //    subsequent indirect calls take the native path.
    let entry: &FnEntry = match ctx_ref.resolve_fn_by_name_tiered(fn_name.as_str()) {
        Some(e) => e,
        None => {
            vm_ctx.update_top_frame_pos(caller_line, caller_col, caller_offset);
            let module = &*ctx_ref.module;
            let outcome = if let Some(callee) = module.func_index.get(fn_name.as_str())
                .and_then(|&idx| module.functions.get(idx))
            {
                crate::interp::exec_function(vm_ctx, module, callee, &args)
            } else if let Some(lazy_fn) = vm_ctx.try_lookup_function(fn_name.as_str()) {
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

    // 4) Build callee frame, register for GC root scanning, invoke, unregister.
    let mut callee_frame = JitFrame::new(entry.max_reg, &args);
    let jit_fn: JitFn = std::mem::transmute(entry.ptr);

    vm_ctx.update_top_frame_pos(caller_line, caller_col, caller_offset);
    vm_ctx.push_frame(crate::exception::VmFrame::new(
        entry.name.clone(),
        entry.file.clone(),
        &callee_frame.regs as *const _,
        &callee_frame.env_arena as *const _,
    ));
    let result = jit_fn(&mut callee_frame, ctx);
    vm_ctx.pop_frame();
    if result != 0 {
        callee_frame.recycle();
        return 1;
    }
    frame_ref.regs[dst as usize] = callee_frame.ret.take().unwrap_or(Value::Null);
    callee_frame.recycle();
    0
}
