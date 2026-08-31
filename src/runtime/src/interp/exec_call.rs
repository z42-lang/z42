/// Call-related instructions: direct calls, builtins, function references,
/// indirect calls (delegate / closure dispatch), closure construction.
///
/// Helpers that may propagate a user exception from a callee return
/// `Result<Option<Value>>` (Some = thrown). Pure helpers return `Result<()>`.

use crate::metadata::{Function, Module, Value};
use crate::vm_context::VmContext;
use anyhow::{bail, Result};
use std::sync::{Arc, OnceLock};

use super::ops::collect_args;
use super::{ExecOutcome, Frame};

/// runtime-jit-tiering Phase 1.5 (mixed-mode): if merged-module function `idx` has
/// already-compiled native code, invoke it directly (mirroring `jit_call`) — set
/// `frame.regs[dst]` from the result, or propagate a throw as `Ok(Some(val))`.
/// Returns `None` when no JIT ctx is published (interp-only run) or the callee is
/// cold / untranslatable (`resolve_fn_by_id_tiered` → None) → the caller then stays
/// on the interpreter. This is what lets an interp frame (a JIT cold-tier callee /
/// fallback) route a hot compiled callee back to native instead of interpreting the
/// whole subtree.
#[cfg(feature = "jit")]
fn try_native_static_call(
    ctx: &VmContext, frame: &mut Frame, dst: u32, idx: usize, args: &[u32],
) -> Option<Result<Option<Value>>> {
    let p = ctx.jit_ctx_ptr();
    if p == 0 { return None; }
    // runtime-jit-tiering Phase 1.5 safety: an INTERP frame can hold a
    // `Ref(Stack)` (an out/ref-param address from `LoadLocalAddr`) in a register;
    // a JIT frame never can (ref-using functions are untranslatable). Native code
    // treats registers as plain values, so passing a Ref into a native callee
    // corrupts it (later surfaces as "Ref vs I64" in arithmetic). This cannot arise
    // from `jit_call` (JIT callers hold no Refs) — it is mixed-mode-specific. Never
    // route when an arg is a Ref; stay on the interpreter (always correct).
    if args.iter().any(|&r| matches!(frame.regs.get(r as usize), Some(Value::Ref { .. }))) {
        return None;
    }
    let jit_ctx = p as *const crate::jit::frame::JitModuleCtx;
    // SAFETY: `jit_ctx` is valid for the whole `JitModule::run_fn` (set/cleared in
    // lockstep with `vm_ctx`). Copy the small entry fields out immediately so no
    // borrow of `*jit_ctx` is held across the native call. `resolve_fn_by_id_tiered`
    // uses interior mutability (OnceLock/Mutex) and may compile-on-threshold — same
    // as `jit_call`.
    let (max_reg, ptr, name, file) = {
        let entry = unsafe { (*jit_ctx).resolve_fn_by_id_tiered(idx) }?;
        (entry.max_reg, entry.ptr, entry.name.clone(), entry.file.clone())
    };
    ctx.counters().jit_native_from_interp.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut callee = crate::jit::frame::JitFrame::new_args_from(max_reg, &frame.regs, args);
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
fn try_native_static_call(
    _ctx: &VmContext, _frame: &mut Frame, _dst: u32, _idx: usize, _args: &[u32],
) -> Option<Result<Option<Value>>> {
    None
}

pub(super) fn call(
    ctx: &VmContext, module: &Module, frame: &mut Frame,
    dst: u32, fname: &str, args: &[u32],
    // method_token: Pre-resolved cache from Function.resolved.method_tokens[site_idx].
    // Some(slot): hot path checks slot for resolved MethodId; on UNRESOLVED (cross-zpkg),
    // falls back to string lookup + lazy loader, then writes the resolved id back into
    // the slot. None: pure string lookup (back-compat).
    method_token: Option<&std::sync::atomic::AtomicU32>,
    // cross_cell: Pre-resolved cross-zpkg target cache from
    // Function.resolved.cross_module_targets[site_idx] (review.md C7). Only
    // consulted *after* the intra-module fast path misses — a cross-zpkg target
    // lives in the lazy loader, not `module.functions`, so it can't be an
    // index. First cross-zpkg hit stores the resolved `Arc<Function>`; later
    // calls borrow it (no `try_lookup_function` hash). None: back-compat.
    cross_cell: Option<&OnceLock<Arc<Function>>>,
    // add-generic-methods: resolved FQ type-arg names for a generic call (empty for
    // non-generic). Threaded into the callee frame's method_type_args slot.
    method_type_args: &[String],
) -> Result<Option<Value>> {
    use std::sync::atomic::Ordering;

    // add-generic-activator: resolve method-type-arg *forwarding* markers `$mta:N`
    // against the CALLER frame's method_type_args[N] before threading to the callee.
    // Emitted when a generic call's type-arg is a bare method-level type param of the
    // enclosing generic method (`Foo<T>() { Bar<T>() }`). The caller frame's slots are
    // already concrete (each call resolves before setting its callee frame), so nesting
    // works. No alloc unless a marker is actually present.
    let fwd_storage;
    let method_type_args: &[String] = if method_type_args.iter().any(|s| s.starts_with("$mta:")) {
        fwd_storage = super::resolve_forwarded_mta(frame, method_type_args);
        &fwd_storage
    } else {
        method_type_args
    };

    // Hot path: resolve the intra-module callee's index into module.functions.
    let callee_idx: Option<usize> = if let Some(slot) = method_token {
        let cached = slot.load(Ordering::Relaxed);
        if cached != crate::metadata::tokens::UNRESOLVED {
            Some(cached as usize)
        } else {
            // Miss: resolve via func_index + write back.
            match module.func_index.get(fname).copied() {
                Some(idx) => {
                    slot.store(idx as u32, Ordering::Relaxed);
                    Some(idx)
                }
                None => None,
            }
        }
    } else {
        // No token (back-compat): old path.
        module.func_index.get(fname).copied()
    };

    // runtime-jit-tiering Phase 1.5 (mixed-mode): route an already-compiled callee
    // to native code instead of interpreting the whole subtree. No-op when there is
    // no published JIT ctx (interp-only run) or the callee is cold/untranslatable.
    // add-generic-methods: generic calls carry method_type_args that the native
    // JIT static-call fast path does not thread yet → stay on the interpreter so
    // the callee frame gets its type_args. (JIT generic support: jit_call path.)
    if method_type_args.is_empty() {
        if let Some(idx) = callee_idx {
            if let Some(res) = try_native_static_call(ctx, frame, dst, idx, args) {
                return res;
            }
        }
    }
    let callee_fn = callee_idx.and_then(|idx| module.functions.get(idx));

    // perf-vm-iteration Phase 1 (Decision 3): fill the callee frame directly
    // from caller regs + arg indices — no `collect_args` Vec, args cloned once.
    let outcome = if let Some(callee) = callee_fn {
        super::exec_function_from_regs(ctx, module, callee, &frame.regs, args, method_type_args)?
    } else if let Some(cell) = cross_cell {
        // Cross-zpkg: borrow the cached Arc<Function> on hit (zero hash);
        // resolve via the lazy loader once on first miss and backfill the cell.
        let target = match cell.get() {
            Some(arc) => arc,
            None => {
                let resolved = ctx.try_lookup_function(fname)
                    .ok_or_else(|| anyhow::anyhow!("undefined function `{fname}`"))?;
                // set() is idempotent: a concurrent double-fill resolves to the
                // same function, so either winner is correct; get() then returns
                // the stored Arc.
                let _ = cell.set(resolved);
                cell.get().expect("cell was just set")
            }
        };
        super::exec_function_from_regs(ctx, module, target.as_ref(), &frame.regs, args, method_type_args)?
    } else if let Some(lazy_fn) = ctx.try_lookup_function(fname) {
        // No cross cell (back-compat): pure lazy-loader lookup, uncached.
        super::exec_function_from_regs(ctx, module, lazy_fn.as_ref(), &frame.regs, args, method_type_args)?
    } else {
        bail!("undefined function `{fname}`");
    };
    match outcome {
        ExecOutcome::Returned(ret) => {
            frame.set(dst, ret.unwrap_or(Value::Null));
            Ok(None)
        }
        ExecOutcome::Thrown(val) => Ok(Some(val)),
    }
}

/// `Builtin` dispatch. Hot path uses pre-resolved `BuiltinId` to index
/// `BUILTINS[id]` directly (no hash). Falls back to name-based lookup
/// when the resolver hasn't populated a token (e.g. unit tests bypassing
/// `Vm::run`).
///
/// `builtin_id` is the resolved `BuiltinId.0` from
/// `Function.resolved.builtin_tokens[site_idx]`, or `None` when the
/// caller has no resolved token to pass (back-compat path).
///
/// make-corelib-errors-catchable (2026-05-15): when the builtin returns
/// `Err`, we convert the anyhow string into a `Std.Exception` instance and
/// surface it as a thrown value via `Ok(Some(exc))`. This makes
/// `int.Parse("abc")` / `u8.Parse("256")` / `byte.Parse(...)` catchable
/// from z42 code with normal `try / catch (Exception e)` syntax, instead of
/// aborting the VM with an uncaught raw error. If exception construction
/// itself fails (e.g. `Std.Exception` type isn't loaded), we fall back to
/// propagating the original error to avoid masking startup-time corruption.
pub(super) fn builtin(
    ctx: &VmContext, module: &crate::metadata::Module,
    frame: &mut Frame, dst: u32, name: &str, args: &[u32],
    builtin_id: Option<u32>,
) -> Result<Option<Value>> {
    let arg_vals = collect_args(&frame.regs, args)?;
    let result = match builtin_id {
        // fix-jit-builtin-ext-fallback: `UNRESOLVED` means the resolver could not bind
        // this name to a static or ext builtin at resolve time (see
        // `resolver::resolve_function_tokens`) — resolve by name now, which re-checks the
        // ext registry (parity with the `None` back-compat path below).
        Some(id) if id != crate::metadata::tokens::UNRESOLVED => crate::corelib::exec_builtin_by_id(
            ctx,
            crate::metadata::tokens::BuiltinId(id),
            &arg_vals,
        ),
        _ => crate::corelib::exec_builtin(ctx, name, &arg_vals),
    };
    match result {
        Ok(v) => {
            frame.set(dst, v);
            Ok(None)
        }
        Err(e) => {
            // A callback builtin (reflection `MethodInfo.Invoke`) that ran z42
            // code which threw stashes the ORIGINAL exception value here so it
            // propagates with its real type, not wrapped into Std.Exception.
            if let Some(thrown) = ctx.take_pending_thrown() {
                return Ok(Some(thrown));
            }
            let msg = e.to_string();
            match crate::exception::make_stdlib_exception(
                ctx, module, "Std.Exception", msg,
            ) {
                Ok(exc) => Ok(Some(exc)),
                Err(_)  => Err(e),  // Std.Exception not loaded → keep raw error
            }
        }
    }
}

/// L2 no-capture lambda lifting: push a function reference value.
/// See docs/design/language/closure.md §6 + ir.md.
pub(super) fn load_fn(frame: &mut Frame, dst: u32, func: &str) {
    frame.set(dst, Value::FuncRef(func.into()));
}

/// 2026-05-02 add-method-group-conversion (D1b): cached method group
/// conversion. First execution constructs `Value::FuncRef(func)` and
/// stores it into the module-level slot; subsequent hits read the slot.
pub(super) fn load_fn_cached(
    ctx: &VmContext, frame: &mut Frame, dst: u32, func: &str, slot_id: u32,
) {
    let cached = ctx.func_ref_slot(slot_id);
    let value = if matches!(cached, Value::Null) {
        let v = Value::FuncRef(func.into());
        ctx.set_func_ref_slot(slot_id, v.clone());
        v
    } else {
        cached
    };
    frame.set(dst, value);
}

/// Indirect call: dispatch on FuncRef (no-capture) or Closure (capturing).
/// For Closures, env is prepended to the user args as the lifted body's
/// implicit first parameter. See closure.md §6.
pub(super) fn call_indirect(
    ctx: &VmContext, module: &Module, frame: &mut Frame,
    dst: u32, callee: u32, args: &[u32],
) -> Result<Option<Value>> {
    // env 解码：FuncRef → 无 env；Closure → 复用已有 heap GcRef；StackClosure
    // → 从当前 frame.env_arena 物化出新 GcRef（arena 持裸 Vec，非 GcRef；且 callee
    //   lifetime 需独立于 caller frame，避免 caller 弹出 arena 后 use-after-free）。
    //
    // S3 (perf-interp-hot-paths): `Value::Closure` 直接把已有 `c.env` GcRef 交给
    // callee（Arc 引用计数 +1），不再 `elems.clone()` 深拷 + `alloc_array` 重分配。
    // 安全性：env 数组是 MkClos 时**写一次**、体内只 `array_get` **读**（编译器
    // `_emitAssign` 无 BoundCapturedIdent 写回分支 → env 槽永不被 array_set 改写），
    // 故跨调用共享 GcRef 与旧的"每次深拷+新 GcRef"行为字节等价，省 O(env) 拷贝 + 一次 GC 分配。
    let (fname, env_val_opt): (String, Option<Value>) = match frame.get(callee)? {
        Value::FuncRef(name) => (name.to_string(), None),
        Value::Closure(c)    => {
            let data = crate::metadata::types::closure_data_of(&c);
            // unify-gc-heap PR-5: fn_name is a GC `Str`; materialize an owned `String` for `fname`.
            (data.fn_name.to_string(), Some(Value::Array(data.env.clone())))
        }
        &Value::StackClosure { idx: hidx, frame_id } => {
            // make-value-copy: resolve the StackClosure handle → StackClosureData via arena.
            let sc = ctx.transient_arena.lock().stack_closure(hidx, frame_id)?;
            let idx = sc.env_idx as usize;
            if idx >= frame.env_arena.len() {
                bail!("CallIndirect: stack closure env_idx {} out of bounds (arena_len={})",
                      idx, frame.env_arena.len());
            }
            // 升格为 heap GcRef 给 callee 用 —— callee 不区分 stack/heap closure。
            let env_val = ctx.heap().alloc_array(frame.env_arena[idx].clone());
            // add-gc-oom-exception: alloc_array returns Null only under strict OOM
            if matches!(env_val, Value::Null) {
                return Ok(Some(crate::exception::make_oom_exception(
                    ctx, module,
                    "cannot allocate closure env: heap limit exceeded".to_string(),
                )));
            }
            (sc.fn_name.clone(), Some(env_val))
        }
        other => bail!("CallIndirect: expected FuncRef / Closure / StackClosure, got {:?}", other),
    };
    let user_vals = collect_args(&frame.regs, args)?;
    let arg_vals: Vec<Value> = match env_val_opt {
        None          => user_vals,
        Some(env_val) => {
            let mut v = Vec::with_capacity(user_vals.len() + 1);
            v.push(env_val);
            v.extend(user_vals);
            v
        }
    };
    let callee_fn = module.func_index.get(fname.as_str())
        .and_then(|&idx| module.functions.get(idx));
    let outcome = if let Some(cfn) = callee_fn {
        super::exec_function(ctx, module, cfn, &arg_vals)?
    } else if let Some(lazy_fn) = ctx.try_lookup_function(&fname) {
        super::exec_function(ctx, module, lazy_fn.as_ref(), &arg_vals)?
    } else {
        bail!("CallIndirect: undefined function `{fname}`");
    };
    match outcome {
        ExecOutcome::Returned(ret) => {
            frame.set(dst, ret.unwrap_or(Value::Null));
            Ok(None)
        }
        ExecOutcome::Thrown(val) => Ok(Some(val)),
    }
}

/// L3 closure construction. `stack_alloc=true` 走 frame-local arena
///（impl-closure-l3-escape-stack）；否则 heap 路径（原 Tier C）。
///
/// add-gc-oom-exception: returns `Ok(Some(exc))` when heap alloc_array fails
/// under strict OOM mode, propagating Std.OutOfMemoryException to the caller.
pub(super) fn mk_clos(
    ctx: &VmContext, module: &Module, frame: &mut Frame,
    dst: u32, fn_name: &str, captures: &[u32], stack_alloc: bool,
) -> Result<Option<Value>> {
    let mut env_vec: Vec<Value> = Vec::with_capacity(captures.len());
    for r in captures {
        env_vec.push(frame.get(*r)?.clone());
    }
    let value = if stack_alloc {
        let env_idx = frame.env_arena.len() as u32;
        frame.env_arena.push(env_vec);
        // make-value-copy: StackClosure payload → transient arena; Value holds an 8B handle.
        let fid = frame.frame_id;
        let hidx = ctx.transient_alloc(
            fid,
            crate::interp::transient_arena::TransientPayload::StackClos(
                crate::metadata::StackClosureData { env_idx, fn_name: fn_name.to_string() },
            ),
        );
        Value::StackClosure { idx: hidx, frame_id: fid }
    } else {
        let env_val = ctx.heap().alloc_array(env_vec);
        // add-gc-oom-exception: alloc_array returns Null only under strict OOM
        if matches!(env_val, Value::Null) {
            return Ok(Some(crate::exception::make_oom_exception(
                ctx, module,
                format!("cannot allocate closure `{fn_name}` env: heap limit exceeded"),
            )));
        }
        let env = match env_val {
            Value::Array(rc) => rc,
            _ => bail!("mk_clos: alloc_array returned unexpected value"),
        };
        // unify-gc-heap PR-2: ClosureData into the GC variable-length region.
        // PR-5: fn_name is a GC `Str`, allocated from the same heap as `env`.
        let fn_name = ctx.heap().alloc_str(fn_name);
        ctx.heap().alloc_closure(crate::metadata::ClosureData {
            env,
            fn_name,
        })
    };
    frame.set(dst, value);
    Ok(None)
}
