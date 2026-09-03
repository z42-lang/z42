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

    // 2. Lazy-loadable init functions (from declared but not-yet-loaded zpkgs).
    //
    // fix-multi-file-static-init (2026-05-15): the compiler now emits
    // `<ns>.<source-stem>.__static_init__` (one per CU), so a single
    // `try_lookup_function("<ns>.__static_init__")` would never resolve. We
    // force-load every declared zpkg, then enumerate ALL `*.__static_init__`
    // functions via the loader and run each.
    let lazy_init_names = ctx.collect_lazy_static_init_names();
    for init_name in lazy_init_names {
        let Some(init_fn) = ctx.try_lookup_function(&init_name) else { continue };
        match exec_function(ctx, module, init_fn.as_ref(), &[])? {
            ExecOutcome::Returned(_) => {}
            ExecOutcome::Thrown(val) =>
                bail!("uncaught exception in static init `{}`: {}", init_name, value_to_str(&val)),
        }
    }
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
