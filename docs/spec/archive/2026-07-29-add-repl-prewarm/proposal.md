# Proposal: REPL 启动即后台预热依赖世界（充分利用打字间隙）

## Why

REPL 启动本身极快（实测 **117.8 ms**），但**首次 eval 仍要干等 ~3.5s**（实测 3.531s，nightly
2026-07-28 / v0.4.0，macOS arm64，hyperfine 8 runs）。这 3.4s 是一次性的
`DepScan.ScanDirsLazy`（扫全 stdlib+编译器 zpkg 世界 + 加载 4 个默认 using 包 + 编译器管线首次
JIT），**与用户输入零相关**，今天却懒到用户敲完第一行回车后才在 `Script.Eval` 里同步跑。

`#46`（2026-07-27）的跨轮缓存已把第 2 轮起打到 ~76ms/轮，但帮不到第一轮（没有可缓存的东西）。
用户从看到提示符（118ms）到敲完第一行通常要 2–4s+——**这段打字间隙完全被浪费**。把这段
input-independent 的预热挪到后台线程、与用户打字并行,首次 eval 即可从「回车后干等 3.5s」变成
「近乎瞬时」。

## What Changes

- **新增 `Script.Prewarm(ScriptState)`**：把 `Eval` 里首轮的「建 `CachedScan` + 加载默认 using
  包」逻辑（[Script.z42:77-83](../../../../src/toolchain/scripting/src/Script.z42)）抽成独立入口，
  **全程操作本地 `DepScanResult`，最后一步才 `state.CachedScan = scan` 原子发布**（保证并发读的
  补全器只见 null-or-complete，无中途可见的半构造态）。
- **REPL 入口后台 spawn 预热**：`interactive_main.Main()` 在 `Script.Create()` 后、进入
  `ReadBlock` 前，用 `Std.Threading.Thread` spawn 一个 worker 跑 `Script.Prewarm(s)`；主线程照常
  阻塞在 `ReadBlock` 等用户输入。`Eval` 首次消费 `CachedScan` 前 `Join` 该 worker（已完成则瞬回）。
- **VM 侧「阻塞原生调用期间 GC-safe park」原语**（`enter/exit_native_parked`）：主线程阻塞在原生
  rustyline `readline` 时**永远不会命中 `check_safepoint`**，若不处理，后台 worker 触发 GC 会
  死等主线程 park（要到用户回车才解）→ 预热在首次 GC 卡死。加一对「进入/离开阻塞原生调用」的
  park 登记，让收集器把「阻塞在原生调用的线程」视为已 park（其 z42 根在原生调用期间冻结、可安全
  扫描）。补全器回调（在 readline 内重入 VM）对称地 `exit → 跑 z42 → enter`。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/gc/safepoint.rs` | MODIFY | 新增 `enter_native_parked` / `exit_native_parked`（或 RAII `NativeParkGuard`）：阻塞原生调用期间把本 ctx 计入 `parked_count`，离开时按 `park_until_idle` 尾部等到安全相位再解除 |
| `src/runtime/src/corelib/repl.rs` | MODIFY | `builtin_repl_readline` / `builtin_repl_readblock` 外层包 enter/exit native-park；`complete_via_callback` 重入 VM 前后对称 exit/enter（补全器作正常 mutator 参与 safepoint） |
| `src/toolchain/scripting/src/Script.z42` | MODIFY | 新增 `public static void Prewarm(ScriptState)`（本地 scan + 末尾原子发布）；`Eval` 首轮改为 `_ensureWarm`（有 worker 则 Join，无则 inline Prewarm 兜底） |
| `src/toolchain/scripting/src/ScriptState.z42` | MODIFY | 新增 `Thread PrewarmThread` 字段（预热 worker 句柄，供 Eval Join）|
| `src/toolchain/scripting/z42.scripting.z42.toml` | MODIFY | `[dependencies]` 加 `z42.threading`（仅使用 Thread，不改该库）|
| `src/toolchain/interactive/core/interactive_main.z42` | MODIFY | `Main()` REPL 分支：`Create()` 后 spawn `Prewarm(s)` worker 存入 `s.PrewarmThread`；`-c` 单次求值分支不预热（无空闲窗口）|
| `docs/design/toolchain/repl.md` | MODIFY | 「实现原理」补预热并发流程（时序 + GC-safe park 决策 + handoff）；更新 Deferred 段 |
| `src/toolchain/scripting/README.md` | MODIFY | 功能索引 + 核心文件登记 Prewarm |
| `src/runtime/src/gc/README.md` | MODIFY | 功能索引登记 native-park 原语（如该目录有 README）|
| `src/toolchain/scripting/tests/repl_prewarm/` | NEW | 回归测试：预热后首轮 eval 结果正确 + 无死锁（管道驱动）|
| `bench/repl/BASELINE.md` | MODIFY | 记录预热后首次-eval 实测（预期 startup+overlap 后近瞬时）|

**只读引用**：

- `src/runtime/src/vm_context.rs` — `parked_count` / `vm_contexts` / `gc_phase` 语义
- `src/runtime/src/corelib/threading.rs` — `__thread_spawn` 的跨线程 VmContext 模型
- `src/libraries/z42.threading/src/Thread.z42` — `Thread.Start(Action)` / `Join()` 契约
- `src/compiler/z42c.pipeline/src/DepScan.z42` — `ScanDirsLazy` / `EnsurePackageLoaded` / `ExtendWithPackage`
- `src/toolchain/scripting/src/Completer.z42` — 补全器读 `CachedScan` 的路径（并发读安全性依据）

## Out of Scope

- **跨会话磁盘持久化 scan**（BASELINE future-work ④）：正交优化，另开 change（首次-ever 运行仍付
  3.4s、需处理 stdlib 变更失效）。本 change 只做「进程内后台预热」。
- **增量编译 / `Vars{N}` O(n) carry**（`repl-future-incremental-compilation`）：不动。
- **让通用阻塞原生调用（文件/网络 I/O）都 GC-safe park**：本 change 只给 REPL readline 接线；把
  native-park 推广到所有阻塞 builtin 是独立的 runtime 治理，另议。

## Open Questions

- [ ] 分支策略：当前工作树在 `jit-inline-fastpaths` 且有不相关的未提交 JIT 改动。本 change 占
      `runtime`+`toolchain`，与 JIT WIP 文件不重叠但同占 `runtime` 子系统 → 需 User 裁决在
      隔离 worktree 还是先安置 JIT WIP 再新分支（见 6.5）。
