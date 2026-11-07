/// Call-related instructions: direct calls, builtins, function references,
/// indirect calls (delegate / closure dispatch), closure construction.
///
/// Helpers that may propagate a user exception from a callee return
/// `Result<Option<Value>>` (Some = thrown). Pure helpers return `Result<()>`.

use crate::vm::exec::ops::call::{bind_callee, fn_ref_id};
use crate::vm::runtime::dispatch::functions::{fn_by_id, lazy_call_id};
use crate::vm::runtime::{Function, Module};
use crate::object::Value;
use crate::vm::runtime::ids::tokens::FnId;
use crate::vm_context::VmContext;
use anyhow::{bail, Result};
use std::sync::Arc;

use super::collect_args;
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
    method_type_args: &[String],
) -> Option<Result<Option<Value>>> {
    let tier = ctx.tier()?;
    if args.iter().any(|&r| matches!(frame.regs.get(r as usize), Some(Value::Ref { .. }))) { return None; }
    tier.invoke(ctx, crate::vm::exec::engine::TierTarget::Site(id),
        crate::vm::exec::call::args::CallArgs::Registers { regs: &frame.regs, indices: args }, method_type_args)
        .map(|result| result.map(|outcome| native_result_to_dst(frame, dst, outcome)))
}

/// Shared tail of the two per-site native diverts (`try_native_static_call` /
/// `exec_vcall::try_native_method_call`): a return lands in `dst` (`Null` for
/// void), a throw is taken off the context and propagated as `Some(exception)`.
pub(super) fn native_result_to_dst(
    frame: &mut Frame, dst: u32, outcome: ExecOutcome,
) -> Option<Value> {
    match outcome {
        ExecOutcome::Returned(ret) => { frame.set(dst, ret.unwrap_or(Value::Null)); None }
        ExecOutcome::Thrown(exc) => Some(exc),
    }
}

#[cfg(not(feature = "jit"))]
#[inline]
fn try_native_static_call(
    _ctx: &VmContext, _frame: &mut Frame, _dst: u32, _id: usize, _args: &[u32],
    _method_type_args: &[String],
) -> Option<Result<Option<Value>>> {
    None
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
    let token = method_token.map_or(crate::vm::runtime::ids::tokens::UNRESOLVED, |s| s.load(Ordering::Relaxed));
    let mut lazy_holder: Option<Arc<Function>> = None;
    let hit = if token != crate::vm::runtime::ids::tokens::UNRESOLVED { fn_by_id((ctx).funcs(), module, token) } else { None };
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
        return Ok(Some(crate::vm::exec::init::make_type_init_exception(ctx, module, &msg)));
    }
    if let Err(msg) = ctx.ensure_callee_owner_init(fname, &target.owner_init) {
        return Ok(Some(crate::vm::exec::init::make_type_init_exception(ctx, module, &msg)));
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
        if let Some(value) = crate::vm::exec::call::forward::try_direct_field_read(ctx, target, &frame.regs, args) {
            frame.set(dst, value);
            return Ok(None);
        }
        if let Some(res) = try_fold(ctx, module, frame, dst, target, None, args) {
            return res;
        }
    }
    if let Some(id) = callee_id {
        if let Some(res) = try_native_static_call(ctx, frame, dst, id, args, method_type_args) {
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
/// registers (`vm::exec::call::builtins::invoke_builtin`); a void builtin leaves `dst` alone.
///
/// A failure becomes what `abi::error::raise` says (shared with `jit_builtin`): the
/// builtin's typed exception — catchable, `StackTrace` filled at the builtin —, or, for
/// an internal error or while a fatal error unwinds, the raw error (`Err`), which no z42
/// handler sees.
pub(super) fn builtin(
    ctx: &VmContext, module: &crate::vm::runtime::Module,
    frame: &mut Frame, dst: u32, name: &str, args: &[u32],
    builtin_id: Option<u32>,
) -> Result<Option<Value>> {
    let id = builtin_id.unwrap_or(crate::vm::runtime::ids::tokens::UNRESOLVED);
    match crate::vm::exec::call::builtins::invoke_builtin(ctx, id, name, None, &frame.regs, args) {
        Ok(Some(v)) => {
            frame.set(dst, v);
            Ok(None)
        }
        Ok(None) => Ok(None),
        Err(e) => raised(crate::vm::exec::call::builtin_abi::error::raise(ctx, module, name, *e, &[])),
    }
}

/// `raise`'s verdict in the interpreter's channels: a thrown value, or an `Err`.
#[cold]
pub(super) fn raised(r: crate::vm::exec::call::builtin_abi::error::Raised) -> Result<Option<Value>> {
    use crate::vm::exec::call::builtin_abi::error::Raised;
    match r {
        Raised::Throw(v) => Ok(Some(v)),
        Raised::Fatal(e) | Raised::Raw(e) => Err(e),
    }
}

/// A call that resolved to a `[Native]` forwarder: run its builtin straight from this
/// frame's registers (`vm::exec::call::forward`). `None` = not a forwarder (or the argument
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
    let id = crate::vm::exec::call::forward::forward_of(ctx, module, target)?;
    Some(run_fold(ctx, module, frame, dst, target, id, this, args))
}

fn run_fold(
    ctx: &VmContext, module: &Module, frame: &mut Frame, dst: u32,
    target: &Function, id: u32, this: Option<Value>, args: &[u32],
) -> Result<Option<Value>> {
    match crate::vm::exec::call::forward::run_folded(ctx, target, id, this, &frame.regs, args, || module) {
        Ok(Some(v)) => { frame.set(dst, v); Ok(None) }
        Ok(None) => { frame.set(dst, Value::Null); Ok(None) }
        Err(f) => raised(f.raise(ctx, module, target)),
    }
}

// ── Function references (P1-2 PR 8) ─────────────────────────────────────────────

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
    let tier = ctx.tier()?;
    if args.iter().any(|&r| matches!(frame.regs.get(r as usize), Some(Value::Ref { .. }))) { return None; }
    let view = match env {
        Some(receiver) => crate::vm::exec::call::args::CallArgs::Receiver { receiver, regs: &frame.regs, indices: args },
        None => crate::vm::exec::call::args::CallArgs::Registers { regs: &frame.regs, indices: args },
    };
    tier.invoke(ctx, crate::vm::exec::engine::TierTarget::Site(id), view, &[])
        .map(|result| result.map(|outcome| native_result_to_dst(frame, dst, outcome)))
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
            let data = crate::object::closure_data_of(&c);
            (data.fn_id, Some(Value::Array(data.env.clone())))
        }
        // fix-null-delegate-invoke: a *null* callee is the one case here that ordinary
        // user code reaches — a single-cast `event` field defaults to null, and the
        // reference manual's own trigger pattern is "snapshot, check null, invoke". Hand
        // back a catchable `Std.NullReferenceException` instead of a `bail!`, whose Rust
        // Debug string no `catch` can intercept (the program died on the spot).
        Value::Null => {
            return Ok(Some(crate::exception::make_stdlib_exception(
                ctx, module, crate::vm::exec::ops::scalar::NULL_REF_EXC, crate::vm::exec::ops::scalar::null_invoke_msg(),
            )?));
        }
        other => bail!("CallIndirect: expected FuncRef / Closure, got {:?}", other),
    };
    let Some(cfn) = fn_by_id((ctx).funcs(), module, id.0) else {
        bail!("CallIndirect: function id {} is not registered in this VM", id.0);
    };
    let physical_count = args.len() + usize::from(env_val_opt.is_some());
    if crate::vm::exec::call::indirect_return::needs_boxed_return(cfn, physical_count)
        || crate::vm::exec::call::indirect_return::has_caller_return_slot(cfn, physical_count)
        || args.iter().enumerate().any(|(position, &index)| frame.regs.get(index as usize)
            .is_some_and(|value| crate::vm::exec::call::indirect_return::has_declared_boxed_argument(
                cfn, position + usize::from(env_val_opt.is_some()), value,
            )))
    {
        let mut values = collect_args(&frame.regs, args)?;
        if let Some(environment) = env_val_opt { values.insert(0, environment); }
        let outcome = crate::vm::exec::call::indirect_return::invoke(
            ctx, module, cfn, id.0 as usize, frame.frame_id(ctx), values.into_vec(),
        )?;
        return Ok(native_result_to_dst(frame, dst, outcome));
    }
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
/// `metadata::reader::instr_decode` `OP_MK_CLOS`). The target is bound by [`fn_ref_id`]
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
    let env_val = crate::object::ArrayObj::alloc_frozen(ctx.heap(), &env_vec);
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
    let value = ctx.heap().alloc_closure(crate::object::ClosureData { env, fn_id });
    frame.set(dst, value);
    Ok(None)
}

#[cfg(test)]
#[path = "exec_call_tests.rs"]
mod exec_call_tests;

#[cfg(test)]
#[path = "exec_call_fn_ref_tests.rs"]
mod exec_call_fn_ref_tests;
