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
/// unify-static-init-into-cctor 之后，本函数**不再执行任何初始化器** —— 静态字段
/// 初始化器已全部并入每类型的类型初始化器，由访问点的屏障在「首次使用前」触发。
/// 这里只剩两件事：清空静态槽（并经 `reset_for_rerun` 递增代际使类型初始化器可重跑），
/// 以及排空「待加载类型」队列让屏障有类型可查。
///
/// 历史（已不适用，留作背景）：曾按三步扫描——扫主模块 `*.__static_init__`、
/// 经 `declared_namespaces()` 触发 lazy load、按 FQN 字母序去重逐一调用。
/// 那条路的问题正是本变更要消灭的：**按名字序而非依赖序**，跨类型依赖静默读到零值。
pub fn init_static_fields(ctx: &VmContext, module: &Module) -> Result<()> {
    ctx.static_fields_clear();

    // unify-static-init-into-cctor（7.4）：此前这里做两件事 ——
    //   ① 扫 `module.functions` 里所有 `*.__static_init__` 并按名字序**急切执行**；
    //   ② 前后两次排空 `pending_static_inits` 队列。
    // 静态字段初始化器已全部并入**每类型的类型初始化器**、按首次使用惰性触发，
    // 两件事都不再需要：没有 `__static_init__` 可扫，也没有那条队列。
    //
    // 仍然要排空「待加载类型」队列（T3 在主模块解析期入队的静态字段所属类）——
    // 那只做**加载**，让屏障在访问点有类型可查；初始化本身不在这里发生。
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
