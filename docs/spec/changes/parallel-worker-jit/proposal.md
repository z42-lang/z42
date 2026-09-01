# Proposal: 并行 worker 线程跑 JIT + 编译码表全局共享

## Why

`--jobs N` 并行编译不但不加速，历史上「越多越慢」。**根因已由代码铁证确定**：`ParallelFor`
的 worker 线程经 `Thread.Start` → `__thread_spawn` → `run_spawned_action` **直接调
`crate::interp::exec_function`**（`corelib/threading.rs:156`），从不 `set_jit_ctx`；而 entry
线程经 `jit::run` → `run_fn` 发布 `jit_ctx`（`jit/mod.rs:143`）跑 JIT。因此开多线程 =

1. **丢 JIT**：每单元工作从原生码退回解释执行（~慢一个量级）；
2. **加锁竞争**：解释器每次 `try_lookup_type/function/string` 砸共享 `lazy_loader` 锁。

双重回归，完全解释「JIT 才是 jobs=1 快的原因」+「jobs=N 反而慢」。不修，`--jobs` 永远是负优化、
并行编译这条路彻底走不通。

真根治 = **让 worker 线程也跑 JIT，且编译码表全线程共享（编一次、N 线程跑）**。有利事实：这套 JIT
结构当初即照可共享设计——`FnEntry` 已 `Send + Sync`（`jit/frame.rs:184`），`JitModuleCtx` 已
`impl Sync`（`:581`），`LazyCompiler` 编译锁注释明写「concurrent first-calls compile each function
exactly once」。唯一 per-thread 的字段是 `vm_ctx`；其余 9 个字段都是「编一次就不变」的共享态。

## What Changes

- **拆 `JitModuleCtx`**：分成 `JitShared`（Arc 共享，含 `fn_entries_by_id` 等 9 个共享字段 +
  拥有 `LazyCompiler`）+ `JitModuleCtx` per-thread 薄壳（`shared: Arc<JitShared>` + `vm_ctx`）。
  薄壳 `Deref` 到 `JitShared`，helper/机器码 ABI 与固定 `vm_ctx` 偏移全部不变。
- **上浮共享表到 `VmCore`**：`VmCore` 加 `jit_shared: OnceLock<Arc<JitShared>>`；entry 的 `jit::run`
  建表后存入，**其存在即「JIT 已激活」信号**。
- **worker 跑 JIT**：`run_spawned_action` 若 `core.jit_shared` 有值 → 建薄壳、发布 `jit_ctx`、经
  run_fn 等价路径把 action 函数跑原生码；否则维持 `exec_function`（interp 模式或未装 JIT 时）。
- **并发正确性测试**：新增 Rust 压力测试，N 线程同时 compile+call 同一/不同函数，兜住「并发编译从
  未真跑过」这一最大风险面。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/jit/frame.rs` | MODIFY | 拆 `JitModuleCtx`→`JitShared`+薄壳；`Deref`；resolve_* 方法留薄壳（读 `self.shared.X` + `self.vm_ctx`）；`JitShared` 的 `unsafe impl Send+Sync` |
| `src/runtime/src/jit/mod.rs` | MODIFY | `JitModule` 持 `Arc<JitShared>`；`setup` 建 `JitShared`；`jit::run` 把 shared 存入 `VmCore`；`run_fn`/静态 init 改跑薄壳；抽出「在给定薄壳上跑一个已解析函数（带 args）」的内部 runner |
| `src/runtime/src/vm_context/types.rs` | MODIFY | `VmCore` 加 `jit_shared: OnceLock<Arc<JitShared>>` 字段 + 访问器 |
| `src/runtime/src/corelib/threading.rs` | MODIFY | `run_spawned_action` 加 JIT 分支：有 `jit_shared` → 建薄壳 + 发布 jit_ctx + 跑原生码；否则 interp（抽 `interp_action_outcome` 共享错误尾） |
| `src/runtime/src/jit/helpers/mod.rs` | MODIFY | 删除随重构变孤儿的 `take_exception_error`（`run_fn_on_shell` 改返回 `ExecOutcome`，不再用它） |
| `src/runtime/src/jit/parallel_tests.rs` | NEW | 并发编译/调用压力测试（N 线程共享 `JitShared`） |
| `src/runtime/src/jit/helpers/control.rs` | MODIFY | 测试 fixture `make_jit_ctx` 改用 `JitModuleCtx::empty_for_test`（拆结构后旧字面量失效） |
| `src/runtime/src/jit/helpers/struct_ops_tests.rs` | MODIFY | 同上，测试 fixture 改用 `empty_for_test` |
| `src/runtime/src/jit/lazy_tests.rs` | MODIFY | `jm.ctx.jit_threshold = N` 改 `jm.ctx.set_jit_threshold(N)`（字段进 `Arc<JitShared>`，加 `#[cfg(test)]` setter） |
| `src/runtime/src/jit/README.md` | MODIFY | 功能索引 + 核心文件登记 `JitShared` / 跨线程共享码表 |
| `docs/book/src/runtime/jit-lazy-compile.md` | MODIFY | 新增「跨线程共享编译码表 + worker 跑 JIT」机制节 |

> **实施期 Scope 修正（2026-09-01）**：① `jit/frame.rs` 的 `JitModuleCtx` 薄壳加了 `impl Deref<Target=JitShared>`，
> 使 `helpers/{call,object,value}.rs` 的 `ctx.module` 等字段读**透明命中共享字段、无需改动** → 这三个文件
> 由原 MODIFY 降为**只读引用**（实际未改）。② 拆结构令三处测试 fixture（`control.rs` / `struct_ops_tests.rs` /
> `lazy_tests.rs`）失效，必须随本变更机械更新，故补入 Scope。③ `run_fn_on_shell` 改返回 `ExecOutcome` 后
> `helpers/mod.rs::take_exception_error` 成孤儿，一并删除。

**只读引用**（理解上下文必须读，不修改）：

- `src/runtime/src/vm.rs` — `Vm::run` 如何按 `ExecMode` 分派到 `jit::run`
- `src/runtime/src/interp/exec_call.rs` — mixed-mode `try_native_static_call` 经 `jit_ctx_ptr` 路由
- `src/compiler/z42c.semantics/src/ParallelFor.z42` — `--jobs N` 如何 `Thread.Start` 起 worker
- `src/runtime/src/vm_context/statics.rs` — `set_jit_ctx` / `jit_ctx_ptr`
- `src/runtime/src/jit/lazy.rs` — `LazyCompiler` 编译锁的并发语义

## Out of Scope

- **`lazy_loader` 无锁化（arc-swap）**：worker 上 JIT 后残余 per-call `lazy_loader` 命中
  （`cross_zpkg_via_interp` / const-str 溢出 / is-check memo 首次 miss）是 warmup×N 粒度、非 per-call
  bulk。本 change 把 scaling 从「越多越慢」翻正即可；逼近线性的 arc-swap 留独立后续 change。
- **`--jobs` 默认串行 → 并行的策略调整**：`--jobs` 仍是 opt-in（#333 决定默认串行）。本 change 只
  让「显式开 `--jobs N` 时」不再负优化，不改默认值。
- **JIT 版 is-check memo**：另有独立 backlog（见 `jit-type-collision-and-ischeck-cache`），不在本 Scope。

## Open Questions

- [ ] `LazyCompiler`（含 cranelift `JITModule`）是否 auto-`Send`？若否，`JitShared` 用
      `unsafe impl Send+Sync`（理由：所有 `LazyCompiler` 访问都在其 `Mutex` 下）——与现有
      `unsafe impl Sync for JitModuleCtx` 一致的做法。实施首步即验证编译期。
- [ ] scaling 量化受限于本机满载（80+ 并发 z42vm）。GREEN/正确性本地可验；scaling 收益须等机器空闲
      或 `parallel_tests.rs` 内建微基准隔离测。
