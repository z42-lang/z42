# Spec: 并行 JIT 执行（worker 线程 + 共享编译码表）

## ADDED Requirements

### Requirement: worker 线程在 JIT 模式下跑原生码

当 VM 以 `ExecMode::Jit` 运行、且程序经 `Std.Threading` 起 worker 线程（含
`ParallelFor` 的 `--jobs N`）时，worker 线程执行其 action 函数走 JIT 原生码，而非解释执行。

#### Scenario: JIT 模式下 worker 跑原生码
- **WHEN** VM `default_mode == Jit`，entry 经 `jit::run` 启动，随后 `__thread_spawn` 起一个 worker
- **THEN** worker 的 `VmContext` 发布了非零 `jit_ctx`（`jit_ctx_ptr() != 0`），其 action 函数经
  `resolve_fn_by_name` 编译为原生码并调用；mixed-mode 路由（`try_native_static_call`）对 worker 生效

#### Scenario: interp 模式下 worker 仍解释执行
- **WHEN** VM `default_mode == Interp`（`VmCore.jit_shared` 为空），起一个 worker
- **THEN** worker 回落 `crate::interp::exec_function`（`jit_ctx_ptr() == 0`），行为与本 change 前一致

#### Scenario: 未 JIT 译的 action 函数回落解释执行
- **WHEN** JIT 模式下 worker 的 action 函数含 interp-only opcode（不可 JIT 译）
- **THEN** worker 回落 `exec_function` 跑该 action，不 panic、结果正确（复用 entry `run_fn` 现有 fallback）

### Requirement: 编译码表全线程共享，编一次

程序的 JIT 编译码表（`JitShared`）存于 `VmCore`，entry 与所有 worker 共享同一 `Arc<JitShared>`；
任一函数在所有线程间**至多编译一次**。

#### Scenario: 多 worker 调同一函数只编一次
- **WHEN** 多个 worker 并发首次调用同一个此前未编的函数
- **THEN** 该函数只被 cranelift 编译一次（`fn_entries_by_id` 对应 `OnceLock` 只写一次），其余 worker
  读到同一 `FnEntry.ptr` 并调用同一份原生码

#### Scenario: 并发编译不同函数不 UB
- **WHEN** 多个 worker 并发首次调用各自不同的未编函数
- **THEN** 编译经 `lazy` 的 `Mutex` 串行化、各自成功；无数据竞争 / panic / UB；所有调用结果正确

### Requirement: worker JIT 执行结果与串行/解释执行逐字节一致

worker 跑 JIT 不得改变任何可观测行为——尤其 z42c `--jobs N` 自建的产物必须与串行、与 interp 模式
逐字节一致。

#### Scenario: 并行自建 byte-identical
- **WHEN** z42c 以 `--jobs N`（N≥2，worker 跑 JIT）自建，与 `--jobs 1` / interp 模式自建对比
- **THEN** 产出的 `.zpkg` 逐字节相同；自举 gen1==gen2 不动点成立

## Pipeline Steps

受影响的 pipeline 阶段（仅 VM 执行层，不动语言/IR 前端）：
- [ ] Lexer — 无
- [ ] Parser / AST — 无
- [ ] TypeChecker — 无
- [ ] IR Codegen — 无
- [x] VM interp — mixed-mode 路由对 worker 生效（既有机制，随 jit_ctx 发布自动启用）
- [x] VM JIT — 拆 `JitModuleCtx`→`JitShared`+薄壳；worker 建薄壳跑原生码；共享码表
