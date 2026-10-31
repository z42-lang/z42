/// Call-related instructions: direct calls, builtins, function references,
/// indirect calls (delegate / closure dispatch), closure construction.
///
/// Helpers that may propagate a user exception from a callee return
/// `Result<Option<Value>>` (Some = thrown). Pure helpers return `Result<()>`.

use crate::metadata::{Function, Module, Value};
use crate::metadata::tokens::FnId;
use crate::vm_context::VmContext;
use anyhow::{bail, Result};
use std::sync::Arc;

use super::ops::collect_args;
use super::{ExecOutcome, Frame};

/// runtime-jit-tiering Phase 1.5 (mixed-mode): route the callee with JIT id `id`
/// (its `FnId` — entry-module and lazily loaded functions alike) to native code,
/// mirroring `jit_call`: the call is counted and the callee compiles at the tier
/// threshold; a compiled callee runs natively — set `frame.regs[dst]` from the
/// result, or propagate a throw as `Ok(Some(val))`. Returns `None` when the VM has
/// no JIT installed (interp-only run) or the callee is cold / untranslatable
/// (`resolve_fn_by_id_tiered` → None) → the caller then stays on the interpreter.
/// This is what lets an interp frame (a JIT cold-tier callee / fallback) route a hot
/// compiled callee back to native instead of interpreting the whole subtree.
#[cfg(feature = "jit")]
fn try_native_static_call(
    ctx: &VmContext, frame: &mut Frame, dst: u32, id: usize, args: &[u32],
) -> Option<Result<Option<Value>>> {
    let rt = ctx.jit()?;
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
    // Copy the small entry fields out so no borrow of the runtime's slot is held
    // across the native call. `resolve_fn_by_id_tiered` uses interior mutability
    // (OnceLock/Mutex) and may compile-on-threshold on this thread — same as
    // `jit_call`.
    let (max_reg, ptr, callee_fn) = {
        let entry = rt.resolve_fn_by_id_tiered(ctx, id)?;
        (entry.max_reg, entry.ptr, entry.func)
    };
    ctx.counters().jit_native_from_interp.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let callee = crate::jit::frame::JitFrame::new_args_from(ctx, max_reg, &frame.regs, args);
    let outcome = unsafe { crate::jit::invoke::enter_compiled(ctx, ptr, callee_fn, callee) };
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
    let method_type_args: &[String] = if super::has_forwarded_mta(method_type_args) {
        fwd_storage = super::resolve_forwarded_mta(ctx, frame, method_type_args);
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
    // A generic call diverts too: a callee that could read its method_type_args never
    // compiles (`jit::translate::unsupported`'s invariant), so a compiled one does not
    // need them; the interpreter path below threads them for the rest.
    // runtime-ambiguous-use-site：**调用**一个被两个已加载 zpkg 各自声明的函数 → 报错。
    // 放在派发前的这一处即可覆盖上面解析出的每条路（token 命中 / 模块内直查 / 惰性回落）——
    // token 可能是解析期按名填好的惰性目标，所以每次调用都判，不能只在绑定时判。
    // 也先于折叠与转入原生码：歧义调用不该因为目标是 `[Native]` 桩或已编译就静默通过。
    // 常态代价 = 一次 relaxed 原子读（进程内从没碰撞过时恒 false）。
    if let Some(exc) = crate::vm_context::symres::ambiguous_function_exception(ctx, module, fname) {
        return Ok(Some(exc));
    }
    // runtime-audit P1-8: a `[Native]` forwarder runs its builtin directly.
    if method_type_args.is_empty() {
        if let Some(res) = try_fold(ctx, module, frame, dst, target, None, args) {
            return res;
        }
    }
    if let Some(id) = callee_id {
        if let Some(res) = try_native_static_call(ctx, frame, dst, id, args) {
            return res;
        }
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

/// `Builtin`: the site's resolved `BuiltinId` (`None` / `UNRESOLVED` → by name, which
/// re-checks the ext registry). The arguments are read in place from this frame's
/// registers (`corelib::invoke_builtin`); a void builtin leaves `dst` alone.
///
/// A failure becomes what `abi::error::raise` says (shared with `jit_builtin`): the
/// builtin's typed exception — catchable, `StackTrace` filled at the builtin —, or, for
/// an internal error or while a fatal error unwinds, the raw error (`Err`), which no z42
/// handler sees.
pub(super) fn builtin(
    ctx: &VmContext, module: &crate::metadata::Module,
    frame: &mut Frame, dst: u32, name: &str, args: &[u32],
    builtin_id: Option<u32>,
) -> Result<Option<Value>> {
    let id = builtin_id.unwrap_or(crate::metadata::tokens::UNRESOLVED);
    match crate::corelib::invoke_builtin(ctx, id, name, None, &frame.regs, args) {
        Ok(Some(v)) => {
            frame.set(dst, v);
            Ok(None)
        }
        Ok(None) => Ok(None),
        Err(e) => raised(crate::corelib::abi::error::raise(ctx, module, name, *e, &[])),
    }
}

/// `raise`'s verdict in the interpreter's channels: a thrown value, or an `Err`.
#[cold]
pub(super) fn raised(r: crate::corelib::abi::error::Raised) -> Result<Option<Value>> {
    use crate::corelib::abi::error::Raised;
    match r {
        Raised::Throw(v) => Ok(Some(v)),
        Raised::Fatal(e) | Raised::Raw(e) => Err(e),
    }
}

/// A call that resolved to a `[Native]` forwarder: run its builtin straight from this
/// frame's registers (`interp::forward`). `None` = not a forwarder (or the argument
/// count differs from the wrapper's parameters) → the ordinary call.
///
/// The not-a-forwarder answer (a count compare + one relaxed load) is inlined into the
/// call path; running the folded builtin is not.
#[inline(always)]
pub(super) fn try_fold(
    ctx: &VmContext, module: &Module, frame: &mut Frame, dst: u32,
    target: &Function, this: Option<Value>, args: &[u32],
) -> Option<Result<Option<Value>>> {
    if args.len() + this.is_some() as usize != target.param_count { return None; }
    let id = super::forward::forward_of(ctx, module, target)?;
    Some(run_fold(ctx, module, frame, dst, target, id, this, args))
}

fn run_fold(
    ctx: &VmContext, module: &Module, frame: &mut Frame, dst: u32,
    target: &Function, id: u32, this: Option<Value>, args: &[u32],
) -> Result<Option<Value>> {
    match super::forward::run_folded(ctx, target, id, this, &frame.regs, args, || module) {
        Ok(Some(v)) => { frame.set(dst, v); Ok(None) }
        Ok(None) => { frame.set(dst, Value::Null); Ok(None) }
        Err(f) => raised(f.raise(ctx, module, target)),
    }
}

// ── Function references (P1-2 PR 8) ─────────────────────────────────────────────

/// The target id a `LoadFn` / `MkClos` site creates its value with: the site's token
/// (`ResolvedTokens::fn_ref_tokens`) when bound — one relaxed load, no hashing — else
/// [`bind_fn_ref`]. `Err` carries the exception to throw.
#[inline]
pub(crate) fn fn_ref_id(
    ctx: &VmContext, module: &Module, name: &str, token: Option<&std::sync::atomic::AtomicU32>,
) -> Result<FnId, Value> {
    if let Some(t) = token {
        let id = t.load(std::sync::atomic::Ordering::Relaxed);
        if id != crate::metadata::tokens::UNRESOLVED { return Ok(FnId(id)); }
    }
    bind_fn_ref(ctx, module, name, token)
}

/// The cold half of [`fn_ref_id`]: resolve a function reference's target by name, once,
/// when the value is created — this module's `func_index` first, then the lazy loader
/// (which loads the defining package) — and store the id into the site's token. The same
/// precedence `CallIndirect` used to apply on every call, and the same id space as `Call`
/// tokens ([`fn_by_id`] reads it back).
///
/// Errors surface here, at creation, instead of at the first invocation (runtime-audit
/// §7 P1-2 decision): an unknown name throws a catchable `MissingSymbolException`
/// (`undefined function `…``, the `Call` path's wording) where `CallIndirect` used to
/// fail with an internal error. A lazily loaded target is only representable when
/// `module` is the `FuncTable`'s entry module (every production run); against any
/// other module (unit tests on a bare `VmContext`) it throws instead.
#[inline(never)]
pub(crate) fn bind_fn_ref(
    ctx: &VmContext, module: &Module, name: &str, token: Option<&std::sync::atomic::AtomicU32>,
) -> Result<FnId, Value> {
    let id = match module.func_index.get(name) {
        Some(&idx) if idx < module.functions.len() => idx as u32,
        _ => {
            let Some(lazy) = ctx.try_lookup_function(name) else {
                return Err(crate::exception::make_missing_symbol_exception(
                    ctx, module, format!("undefined function `{name}`")));
            };
            match lazy_call_id(ctx, module, &lazy) {
                Some(id) => id as u32,
                None => {
                    let msg = format!(
                        "cannot reference `{name}`: a lazily loaded function has no id outside the VM's entry module");
                    return Err(crate::exception::make_stdlib_exception(ctx, module, "Std.Exception", msg.clone())
                        .unwrap_or_else(|_| Value::Str(msg.into())));
                }
            }
        }
    };
    if let Some(t) = token { t.store(id, std::sync::atomic::Ordering::Relaxed); }
    Ok(FnId(id))
}

/// L2 no-capture lambda / free-function method group: `dst = FuncRef(target)`, the target
/// bound by [`fn_ref_id`]. Returns the exception to throw when the target cannot be bound.
// Kept out of line: `exec_instr`'s dispatch match stays the same size as before P1-2 PR 8
// (inlining these bodies into it measurably slowed unrelated hot arms, e.g. VCall).
#[inline(never)]
pub(super) fn load_fn(
    ctx: &VmContext, module: &Module, frame: &mut Frame, dst: u32, func: &str,
    token: Option<&std::sync::atomic::AtomicU32>,
) -> Option<Value> {
    match fn_ref_id(ctx, module, func, token) {
        Ok(id) => { frame.set(dst, Value::FuncRef(id)); None }
        Err(exc) => Some(exc),
    }
}

/// runtime-jit-tiering Phase 1.5 for `CallIndirect`: like [`try_native_static_call`],
/// with a closure's env as the callee's implicit first argument (register 0).
#[cfg(feature = "jit")]
fn try_native_indirect_call(
    ctx: &VmContext, frame: &mut Frame, dst: u32, id: usize, env: Option<Value>, args: &[u32],
) -> Option<Result<Option<Value>>> {
    let rt = ctx.jit()?;
    // Same Ref guard as `try_native_static_call` (an interp frame may hold a `Ref`).
    if args.iter().any(|&r| matches!(frame.regs.get(r as usize), Some(Value::Ref { .. }))) {
        return None;
    }
    let (max_reg, ptr, callee_fn) = {
        let entry = rt.resolve_fn_by_id_tiered(ctx, id)?;
        (entry.max_reg, entry.ptr, entry.func)
    };
    ctx.counters().jit_native_from_interp.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let callee = match env {
        Some(env) => crate::jit::frame::JitFrame::new_method_args_from(ctx, max_reg, env, &frame.regs, args),
        None => crate::jit::frame::JitFrame::new_args_from(ctx, max_reg, &frame.regs, args),
    };
    let outcome = unsafe { crate::jit::invoke::enter_compiled(ctx, ptr, callee_fn, callee) };
    Some(Ok(native_result_to_dst(ctx, frame, dst, outcome)))
}

#[cfg(not(feature = "jit"))]
#[inline]
fn try_native_indirect_call(
    _ctx: &VmContext, _frame: &mut Frame, _dst: u32, _id: usize, _env: Option<Value>, _args: &[u32],
) -> Option<Result<Option<Value>>> {
    None
}

/// Indirect call: dispatch on FuncRef (no-capture) or Closure (capturing).
/// For Closures, env is prepended to the user args as the lifted body's
/// implicit first parameter. See closure.md §6.
///
/// P1-2 PR 8: the value carries its target's id (bound at creation), so dispatch is
/// [`fn_by_id`] — no name, no hashing, lazily loaded targets included — and the callee is
/// counted toward its tier-up and run natively once compiled, like a `Call`.
// Kept out of line: `exec_instr`'s dispatch match stays the same size as before P1-2 PR 8
// (inlining these bodies into it measurably slowed unrelated hot arms, e.g. VCall).
#[inline(never)]
pub(super) fn call_indirect(
    ctx: &VmContext, module: &Module, frame: &mut Frame,
    dst: u32, callee: u32, args: &[u32],
) -> Result<Option<Value>> {
    // env 解码：FuncRef → 无 env；Closure → 复用已有 heap ArrRef。
    //
    // S3 (perf-interp-hot-paths): `Value::Closure` 直接把已有 `c.env` ArrRef 交给
    // callee（Arc 引用计数 +1），不再 `elems.clone()` 深拷 + `alloc_array` 重分配。
    // 安全性：env 数组是 MkClos 时**写一次**、体内只 `array_get` **读**（编译器
    // `_emitAssign` 无 BoundCapturedIdent 写回分支 → env 槽永不被 array_set 改写），
    // 故跨调用共享 ArrRef 与旧的"每次深拷+新 ArrRef"行为字节等价，省 O(env) 拷贝 + 一次 GC 分配。
    let (id, env_val_opt): (FnId, Option<Value>) = match frame.get(callee)? {
        Value::FuncRef(id) => (*id, None),
        Value::Closure(c)    => {
            let data = crate::metadata::types::closure_data_of(&c);
            (data.fn_id, Some(Value::Array(data.env.clone())))
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
    let Some(cfn) = fn_by_id(ctx, module, id.0) else {
        bail!("CallIndirect: function id {} is not registered in this VM", id.0);
    };
    if let Some(res) = try_native_indirect_call(ctx, frame, dst, id.0 as usize, env_val_opt, args) {
        return res;
    }
    let mut arg_vals = collect_args(&frame.regs, args)?;
    if let Some(env_val) = env_val_opt {
        arg_vals.insert(0, env_val);
    }
    let outcome = super::exec_function(ctx, module, cfn, &arg_vals)?;
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
/// `zbc_reader::instr_decode` `OP_MK_CLOS`). The target is bound by [`fn_ref_id`]
/// before anything is allocated.
///
/// add-gc-oom-exception: returns `Ok(Some(exc))` when heap alloc_array fails
/// under strict OOM mode, propagating Std.OutOfMemoryException to the caller.
// Kept out of line: `exec_instr`'s dispatch match stays the same size as before P1-2 PR 8
// (inlining these bodies into it measurably slowed unrelated hot arms, e.g. VCall).
#[inline(never)]
pub(super) fn mk_clos(
    ctx: &VmContext, module: &Module, frame: &mut Frame,
    dst: u32, fn_name: &str, captures: &[u32],
    token: Option<&std::sync::atomic::AtomicU32>,
) -> Result<Option<Value>> {
    let fn_id = match fn_ref_id(ctx, module, fn_name, token) {
        Ok(id) => id,
        Err(exc) => return Ok(Some(exc)),
    };
    let mut env_vec: Vec<Value> = Vec::with_capacity(captures.len());
    for r in captures {
        env_vec.push(frame.get(*r)?.clone());
    }
    let env_val = crate::metadata::types::ArrayObj::alloc_frozen(ctx.heap(), &env_vec);
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
    let value = ctx.heap().alloc_closure(crate::metadata::ClosureData { env, fn_id });
    frame.set(dst, value);
    Ok(None)
}

#[cfg(test)]
#[path = "exec_call_tests.rs"]
mod exec_call_tests;

#[cfg(test)]
#[path = "exec_call_fn_ref_tests.rs"]
mod exec_call_fn_ref_tests;
