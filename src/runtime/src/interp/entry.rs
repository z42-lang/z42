//! 解释器公开入口：ExecOutcome / run / run_returning / run_outcome / init_static_fields / run_with_static_init。refactor-split-interp-mod（2026-09-03）：自 1155 行的 `interp/mod.rs` 逐行搬出，
//! mod.rs 只留模块表与执行主循环 `exec_function_body`；本模块经 mod.rs 的 `pub(crate) use` 全量再导出，
//! 兄弟模块的 `super::X` 路径不变。

#![allow(unused_imports)]
use super::*;
use crate::metadata::{BranchTargets, Function, Module, Terminator, Value};
use crate::vm_context::VmContext;
use anyhow::{bail, Context, Result};
use std::collections::HashMap;

/// Outcome of executing a function.
/// User exceptions are value-based (no heap allocation), not anyhow errors.
///
/// Public so embedders (test-runner, REPL) can introspect thrown exception
/// values — necessary for [ShouldThrow<E>] type matching and TestFailure /
/// SkipSignal classification (rewrite-z42-test-runner-compile-time S3,
/// 2026-05-10).
#[derive(Debug)]
pub enum ExecOutcome {
    /// Normal return (with optional return value).
    Returned(Option<Value>),
    /// User exception thrown and not caught within this function.
    Thrown(Value),
}

// ── Public entry points ──────────────────────────────────────────────────────

/// Entry point: run a function with the given arguments.
pub fn run(ctx: &VmContext, module: &Module, func: &Function, args: &[Value]) -> Result<()> {
    match exec_function(ctx, module, func, args)? {
        ExecOutcome::Returned(_) => Ok(()),
        ExecOutcome::Thrown(val) => bail!("{}", crate::exception::format_uncaught(&val, module)),
    }
}

/// Variant of [`run`] that returns the function's return value (if any)
/// instead of discarding it. Used by integration tests and by embedders
/// that need the result of a script entry point. Mirrors `run` in every
/// other respect (errors, exception conversion).
pub fn run_returning(
    ctx: &VmContext,
    module: &Module,
    func: &Function,
    args: &[Value],
) -> Result<Option<Value>> {
    match exec_function(ctx, module, func, args)? {
        ExecOutcome::Returned(v) => Ok(v),
        ExecOutcome::Thrown(val) => bail!("{}", crate::exception::format_uncaught(&val, module)),
    }
}

/// Public-API variant of [`run`] that surfaces both the typed thrown
/// exception value (for type introspection / [ShouldThrow<E>] matching)
/// and the optional return value, instead of collapsing thrown into an
/// anyhow string. For embedders that need exception-aware control flow
/// (rewrite-z42-test-runner-compile-time S3, 2026-05-10).
pub fn run_outcome(
    ctx: &VmContext,
    module: &Module,
    func: &Function,
    args: &[Value],
) -> Result<ExecOutcome> {
    exec_function(ctx, module, func, args)
}

/// Initialise static state: clears static fields then runs ALL
/// `*.__static_init__` functions (both eager-loaded in `module.functions`
/// and lazy-loadable from declared zpkgs).
///
/// Extracted from [`run_with_static_init`] (2026-05-10 R3b) so embedders
/// (test-runner, REPL) can do init once + run multiple functions in
/// sequence (Setup → Test → Teardown) without re-initialising between.
///
/// 2026-04-27 fix-static-field-access: 修前只跑 `{module.name}.__static_init__`
/// (主模块)，导入的 zpkg（如 z42.math 的 `Std.Math.__static_init__`）虽然 link 进
/// merged module 但永不被调用 → `Math.PI` 等常量永远 `null`。
///
/// interp 模式下 stdlib 是 lazy-loaded，启动时除 z42.core 外都不在
/// `module.functions`。所以同时需要：
///   1. 扫主模块 functions（拿到 eagerly-loaded 的 init，含 main 自己 + z42.core）
///   2. 通过 `lazy_loader::declared_namespaces()` 拿到所有声明但未加载的命名空间，
///      调用 `try_lookup_function("<ns>.__static_init__")` 触发 lazy load
///   3. 合并 + 按 FQN 字母序去重 + 逐一调用
///
/// 副作用：所有声明的 stdlib zpkg 都会被 eagerly 加载（不再纯 lazy）。
pub fn init_static_fields(ctx: &VmContext, module: &Module) -> Result<()> {
    ctx.static_fields_clear();

    // defer-class-initialization: 先跑依赖包的初始化器（T3 在主模块解析期入队的
    // 「静态字段所属类」），再跑主模块自己的——主模块的初始化器可能读依赖包设置的
    // 静态字段（fix-static-field-access 的顺序依赖）。
    ctx.run_pending_static_inits();

    // 1. Eager-loaded init functions (in main + z42.core).
    let mut eager_inits: Vec<&Function> = module.functions.iter()
        .filter(|f| f.name.ends_with(".__static_init__"))
        .collect();
    eager_inits.sort_by(|a, b| a.name.cmp(&b.name));
    for init_fn in &eager_inits {
        match exec_function(ctx, module, init_fn, &[])? {
            ExecOutcome::Returned(_) => {}
            ExecOutcome::Thrown(val) =>
                bail!("uncaught exception in static init `{}`: {}", init_fn.name, value_to_str(&val)),
        }
    }

    // 2. defer-class-initialization (2026-09-04): 不再 force-load 全部已声明 zpkg。
    //
    // 变更前这里调 `collect_lazy_static_init_names()`，它内部
    // `force_load_all_declared()` 把 libs/ 下每个候选包整包加载再全表扫后缀——
    // 实测 hello world 因此加载 18 个包 2910 个函数、13.6 ms，而真正要跑的 31 个
    // 初始化器合计只要 78 µs（99.4% 的成本是「找」）。
    //
    // 现在改为按需：包被首次触达时加载，其 `__static_init__` 入队，由
    // `run_pending_static_inits` 在锁外执行（触发点 T1 函数查找 / T2 类型查找 /
    // T3 静态字段引用）。这里只需排空 T3 在主模块解析期入队的「所属类」——
    // **必须在步骤 1 之前**，因为主模块的初始化器可能读依赖包的静态字段
    // （2026-04-27 fix-static-field-access 记录过这个顺序依赖）。
    ctx.run_pending_static_inits();
    Ok(())
}

/// Run with static init: convenience wrapper — calls
/// [`init_static_fields`] then runs `func`. Used by `Vm::run`.
pub fn run_with_static_init(ctx: &VmContext, module: &Module, func: &Function) -> Result<()> {
    init_static_fields(ctx, module)?;
    match exec_function(ctx, module, func, &[])? {
        ExecOutcome::Returned(_) => Ok(()),
        ExecOutcome::Thrown(val) => bail!("{}", crate::exception::format_uncaught(&val, module)),
    }
}

// ── Frame ────────────────────────────────────────────────────────────────────
