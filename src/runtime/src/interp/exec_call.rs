/// Call-related instructions: direct calls, builtins, function references,
/// indirect calls (delegate / closure dispatch), closure construction.
///
/// Helpers that may propagate a user exception from a callee return
/// `Result<Option<Value>>` (Some = thrown). Pure helpers return `Result<()>`.

use crate::metadata::{Function, Module, Value};
use crate::vm_context::VmContext;
use anyhow::{bail, Result};
use std::sync::Arc;

use super::ops::collect_args;
use super::{ExecOutcome, Frame};

/// runtime-jit-tiering Phase 1.5 (mixed-mode): route the callee with JIT id `id`
/// (its `FnId` — entry-module and lazily loaded functions alike) to native code,
/// mirroring `jit_call`: the call is counted and the callee compiles at the tier
/// threshold; a compiled callee runs natively — set `frame.regs[dst]` from the
/// result, or propagate a throw as `Ok(Some(val))`. Returns `None` when no JIT ctx
/// is published (interp-only run) or the callee is cold / untranslatable
/// (`resolve_fn_by_id_tiered` → None) → the caller then stays on the interpreter.
/// This is what lets an interp frame (a JIT cold-tier callee / fallback) route a hot
/// compiled callee back to native instead of interpreting the whole subtree.
#[cfg(feature = "jit")]
fn try_native_static_call(
    ctx: &VmContext, frame: &mut Frame, dst: u32, id: usize, args: &[u32],
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
    let (max_reg, ptr, callee_fn) = {
        let entry = unsafe { (*jit_ctx).resolve_fn_by_id_tiered(id) }?;
        (entry.max_reg, entry.ptr, entry.func)
    };
    ctx.counters().jit_native_from_interp.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let callee = crate::jit::frame::JitFrame::new_args_from(ctx, max_reg, &frame.regs, args);
    let outcome = unsafe { crate::jit::invoke::call_native(ctx, jit_ctx, ptr, callee_fn, callee) };
    Some(Ok(native_result_to_dst(ctx, frame, dst, outcome)))
}

/// Shared tail of the two per-site native diverts (`try_native_static_call` /
/// `exec_vcall::try_native_method_call`): a return lands in `dst` (`Null` for
/// void), a throw is taken off the context and propagated as `Some(exception)`.
#[cfg(feature = "jit")]
pub(super) fn native_result_to_dst(
    ctx: &VmContext, frame: &mut Frame, dst: u32, outcome: crate::jit::invoke::NativeOutcome,
) -> Option<Value> {
    match outcome.into_exec(ctx) {
        ExecOutcome::Returned(ret) => { frame.set(dst, ret.unwrap_or(Value::Null)); None }
        ExecOutcome::Thrown(exc) => Some(exc),
    }
}

#[cfg(not(feature = "jit"))]
#[inline]
fn try_native_static_call(
    _ctx: &VmContext, _frame: &mut Frame, _dst: u32, _id: usize, _args: &[u32],
) -> Option<Result<Option<Value>>> {
    None
}

/// P1-2: the callee a bound `Call` token or a `VCall` PIC payload names. In range of
/// `module.functions` it is that index (for the entry module that *is* the `FnId`); past
/// it, a lazily loaded function's `FnId` (only ever stored when `module` is the
/// `FuncTable`'s entry module, see `ResolvedTokens::method_tokens`). Lock-free either
/// way. `None` cannot happen for an id a call path stored once the storing thread's
/// registration is visible; callers then fall back to binding by name.
#[inline]
pub(crate) fn fn_by_id<'m>(ctx: &'m VmContext, module: &'m Module, id: u32) -> Option<&'m Function> {
    match module.functions.get(id as usize) {
        Some(f) => Some(f),
        None => ctx.funcs().get(crate::metadata::tokens::FnId(id)),
    }
}

/// The callee's JIT id / `FnId` when the call path may cache it for `module`: a
/// lazily loaded function only counts when `module` is the `FuncTable`'s entry
/// module and the table holds exactly this function under the id (see
/// `ResolvedTokens::method_tokens`).
#[inline]
pub(crate) fn lazy_call_id(ctx: &VmContext, module: &Module, f: &Function) -> Option<usize> {
    let id = f.id.get()?;
    let funcs = ctx.funcs();
    (funcs.is_entry(module) && funcs.get(id).is_some_and(|g| std::ptr::eq(g, f))).then_some(id.0 as usize)
}

/// The cold half of [`call`]: bind `fname` by name — this module's `func_index` first,
/// then the lazy loader (which may load the defining package) — check the signature,
/// and store the binding into the site's token. Returns the callee and its JIT id
/// (`None` only for a lazily loaded callee that has no id under `module`). `Err`
/// carries the exception to throw. Shared with `jit_call`'s by-name tier.
///
/// fix-call-arity-skew：首次绑定点。resolver 预填时已拒绝过签名对不上的站点，所以它们每次都会
/// 走到这里 —— 判定在此抛，且**不写回**（写回就等于把错的绑定缓存下来）。判定先于 cctor 屏障
/// （fix-crosspkg-static-call-cctor 的顺序）：签名对不上的调用本身非法，不应触发类型初始化。
#[inline(never)]
pub(crate) fn bind_callee<'a>(
    ctx: &'a VmContext, module: &'a Module, fname: &str, argc: usize,
    method_token: Option<&std::sync::atomic::AtomicU32>,
    holder: &'a mut Option<Arc<Function>>,
) -> Result<(&'a Function, Option<usize>), Value> {
    use crate::vm_context::symres::{call_arity, wrong_arity_exception};
    use std::sync::atomic::Ordering;
    if let Some(&idx) = module.func_index.get(fname) {
        if let Some(f) = module.functions.get(idx) {
            if let Some(exc) = wrong_arity_exception(ctx, module, fname, call_arity(f), argc) {
                return Err(exc);
            }
            if let Some(slot) = method_token { slot.store(idx as u32, Ordering::Relaxed); }
            return Ok((f, Some(idx)));
        }
    }
    // fix-silent-symbol-resolution：所有回落（本模块 func_index → 惰性加载器）都穷尽了 ⇒
    // **确定不存在**，抛可 catch 的类型化 MissingSymbolException（不是 `bail!`——那条走
    // anyhow Err，不经 find_handler，用户 `catch` 抓不到）。与 JIT 侧同一场景统一。
    let Some(lazy) = ctx.try_lookup_function(fname) else {
        return Err(crate::exception::make_missing_symbol_exception(
            ctx, module, format!("undefined function `{fname}`")));
    };
    if let Some(exc) = wrong_arity_exception(ctx, module, fname, call_arity(lazy.as_ref()), argc) {
        return Err(exc);
    }
    // Cache the binding as the callee's `FnId` — only when tokens are `FnId`s for this
    // module, and only for the function the table actually holds under that id.
    let id = lazy_call_id(ctx, module, &lazy);
    if let (Some(slot), Some(id)) = (method_token, id) {
        slot.store(id as u32, Ordering::Relaxed);
    }
    Ok((&**holder.insert(lazy), id))
}

pub(super) fn call(
    ctx: &VmContext, module: &Module, frame: &mut Frame,
    dst: u32, fname: &str, args: &[u32],
    // method_token: this site's `ResolvedTokens.method_tokens` slot. Bound ⇒ the callee
    // directly (`fn_by_id`, no hashing — merged and lazily loaded callees alike);
    // `UNRESOLVED` ⇒ bind by name and store it (`bind_callee`). None: pure name lookup
    // (back-compat, nothing cached).
    method_token: Option<&std::sync::atomic::AtomicU32>,
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

    // Resolve the callee **before** the barriers below: the first call into a not-yet-loaded
    // package is what loads it, and loading is what registers its types' static constructors
    // (`LazyLoader::insert_type`). `callee_id` = the callee's JIT id (= the bound token),
    // which routes it to native code when compiled.
    let token = method_token.map_or(crate::metadata::tokens::UNRESOLVED, |s| s.load(Ordering::Relaxed));
    let mut lazy_holder: Option<Arc<Function>> = None;
    let hit = if token != crate::metadata::tokens::UNRESOLVED { fn_by_id(ctx, module, token) } else { None };
    let (target, callee_id): (&Function, Option<usize>) = match hit {
        Some(f) => (f, Some(token as usize)),
        None => match bind_callee(ctx, module, fname, args.len(), method_token, &mut lazy_holder) {
            Ok(bound) => bound,
            Err(exc) => return Ok(Some(exc)),
        },
    };

    // add-static-constructors：调用该类型的静态方法也是 C# 的类型初始化触发点。
    // 热路径代价 = 一次 relaxed load（`any_cctor_pending()` 在门内短路）。
    // fix-crosspkg-static-call-cctor：必须在**解析之后**——解析前依赖包可能还没加载，其类型未登记，
    // 门读到 0 就会让这第一次调用跳过静态构造器（实测：首次使用是跨包静态方法时 cctor 不跑）。
    // add-module-init-hook：**包级**初始化先于类型初始化 —— 解析上面那个名字可能刚把一个包
    // 拉进来，它的 `[ModuleInit]` 必须在本包任何代码（包括马上要调的这个函数）之前跑完。
    // 稳态代价 = 一次 relaxed load。
    if let Err(msg) = ctx.ensure_module_inits(Some(fname)) {
        return Ok(Some(crate::vm_context::cctor::make_type_init_exception(ctx, module, &msg)));
    }
    if let Err(msg) = ctx.ensure_callee_owner_init(fname, &target.owner_init) {
        return Ok(Some(crate::vm_context::cctor::make_type_init_exception(ctx, module, &msg)));
    }

    // runtime-jit-tiering Phase 1.5 (mixed-mode): count the callee toward its tier-up
    // and route it to native code once compiled, instead of interpreting the whole
    // subtree — lazily loaded callees exactly like entry-module ones. No-op when there
    // is no published JIT ctx (interp-only run) or the callee is cold/untranslatable.
    // add-generic-methods: generic calls carry method_type_args that the native
    // JIT static-call fast path does not thread yet → stay on the interpreter so
    // the callee frame gets its type_args. (JIT generic support: jit_call path.)
    if method_type_args.is_empty() {
        if let Some(id) = callee_id {
            if let Some(res) = try_native_static_call(ctx, frame, dst, id, args) {
                return res;
            }
        }
    }
    // runtime-ambiguous-use-site：**调用**一个被两个已加载 zpkg 各自声明的函数 → 报错。
    // 放在派发前的这一处即可覆盖上面解析出的每条路（token 命中 / 模块内直查 / 惰性回落）——
    // token 可能是解析期按名填好的惰性目标，所以每次调用都判，不能只在绑定时判。
    // 常态代价 = 一次 relaxed 原子读（进程内从没碰撞过时恒 false）。
    if let Some(exc) = crate::vm_context::symres::ambiguous_function_exception(ctx, module, fname) {
        return Ok(Some(exc));
    }
    // perf-vm-iteration Phase 1 (Decision 3): fill the callee frame directly
    // from caller regs + arg indices — no `collect_args` Vec, args cloned once.
    let outcome = super::exec_function_from_regs(ctx, module, target, &frame.regs, args, method_type_args)?;
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
        // split-null-sentinel-channels ④：`None` = 该 builtin **声明为 void**
        // ⇒ 不往 `dst` 存任何东西。此前 void builtin 返回 `Ok(Value::Null)`、
        // 这里无条件 `frame.set(dst, Null)` ⇒ 「无返回值」与「返回 null」在寄存器里
        // 长得一模一样。`Option` 在类型上强制这一格被处理。
        Ok(Some(v)) => {
            frame.set(dst, v);
            Ok(None)
        }
        Ok(None) => Ok(None),
        Err(e) => {
            // A callback builtin (reflection `MethodInfo.Invoke`) that ran z42
            // code which threw stashes the ORIGINAL exception value here so it
            // propagates with its real type, not wrapped into Std.Exception.
            if let Some(thrown) = ctx.take_pending_thrown() {
                return Ok(Some(thrown));
            }
            // A fatal VM error inside a re-entering builtin stays an internal
            // error — it must not become a catchable `Std.Exception`.
            if crate::stack_guard::is_fatal(ctx) { return Err(e); }
            // Typed throw (null receiver → NullReferenceException) or `Std.Exception`;
            // `Err` = the class is not loaded → keep the raw error. Shared with `jit_builtin`.
            crate::corelib::builtin_error_exception(ctx, module, e).map(Some)
        }
    }
}

/// L2 no-capture lambda lifting: push a function reference value.
/// See docs/internals/src/runtime/escape-analysis.md (闭包栈分配) + ir.md.
pub(super) fn load_fn(frame: &mut Frame, dst: u32, func: &str) {
    frame.set(dst, Value::FuncRef(func.into()));
}

/// Indirect call: dispatch on FuncRef (no-capture) or Closure (capturing).
/// For Closures, env is prepended to the user args as the lifted body's
/// implicit first parameter. See closure.md §6.
pub(super) fn call_indirect(
    ctx: &VmContext, module: &Module, frame: &mut Frame,
    dst: u32, callee: u32, args: &[u32],
) -> Result<Option<Value>> {
    // env 解码：FuncRef → 无 env；Closure → 复用已有 heap GcRef。
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
        // fix-null-delegate-invoke: a *null* callee is the one case here that ordinary
        // user code reaches — a single-cast `event` field defaults to null, and the
        // reference manual's own trigger pattern is "snapshot, check null, invoke". Hand
        // back a catchable `Std.NullReferenceException` instead of a `bail!`, whose Rust
        // Debug string no `catch` can intercept (the program died on the spot).
        Value::Null => {
            return Ok(Some(crate::exception::make_stdlib_exception(
                ctx, module, crate::semantics::NULL_REF_EXC, crate::semantics::null_invoke_msg(),
            )?));
        }
        other => bail!("CallIndirect: expected FuncRef / Closure, got {:?}", other),
    };
    let mut arg_vals = collect_args(&frame.regs, args)?;
    if let Some(env_val) = env_val_opt {
        arg_vals.insert(0, env_val);
    }
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

/// L3 closure construction: the env is a heap array, the closure a heap
/// `Value::Closure` (the zbc stack-alloc byte is decoded and dropped — see
/// `zbc_reader::instr_decode` `OP_MK_CLOS`).
///
/// add-gc-oom-exception: returns `Ok(Some(exc))` when heap alloc_array fails
/// under strict OOM mode, propagating Std.OutOfMemoryException to the caller.
pub(super) fn mk_clos(
    ctx: &VmContext, module: &Module, frame: &mut Frame,
    dst: u32, fn_name: &str, captures: &[u32],
) -> Result<Option<Value>> {
    let mut env_vec: Vec<Value> = Vec::with_capacity(captures.len());
    for r in captures {
        env_vec.push(frame.get(*r)?.clone());
    }
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
    // PR-5: fn_name is a GC `Str` from the same heap as `env` — interned per site.
    let fn_name = ctx.intern_fn_name(fn_name);
    let value = ctx.heap().alloc_closure(crate::metadata::ClosureData {
        env,
        fn_name,
    });
    frame.set(dst, value);
    Ok(None)
}

#[cfg(test)]
#[path = "exec_call_tests.rs"]
mod exec_call_tests;
