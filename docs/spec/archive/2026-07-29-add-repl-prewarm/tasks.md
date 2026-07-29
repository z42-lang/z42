# Tasks: REPL 后台预热依赖世界

> 状态：🟢 已完成 | 创建：2026-07-29 | 完成：2026-07-29
> 类型：perf（含 VM 并发行为变更 → 走完整流程）
> 子系统占用：`runtime` + `toolchain`（归档即释放）

## 进度概览
- [x] 阶段 0: 分支/隔离就位（隔离 worktree `z42-replprewarm-wt` off HEAD 920d223f）
- [x] 阶段 1: VM native-park 原语（runtime）
- [x] 阶段 2: REPL 接线（runtime repl.rs）
- [x] 阶段 3: z42 侧预热 + spawn + 汇合（toolchain）
- [x] 阶段 4: 测试与验证（native-park 单测 2/2 + e2e 424/0 + 端到端重叠实测）
- [x] 阶段 5: 文档同步 + 剩余 GREEN + 归档

## 阶段 0: 分支/隔离
- [x] 0.1 隔离 worktree `z42-replprewarm-wt`（分支同名，off HEAD；与 `jit-inline-fastpaths` 未提交 JIT WIP 物理隔离）
- [x] 0.2 ACTIVE.md 登记 `runtime` + `toolchain` 持有者 = `add-repl-prewarm`

## 阶段 1: VM native-park 原语（runtime）
- [x] 1.1 `gc/safepoint.rs`：新增 `NativeParkGuard`（RAII，exception-safe）/ `NativeUnparkGuard`（补全器反转）+ `native_park_incr/decr`（复用 `parked_count` + `gc_phase_cv`，无新同步原语）；`gc/mod.rs` `pub use` 导出
- [x] 1.2 单测（`safepoint_tests.rs`）：`native_park_guard_satisfies_collector_without_safepoint`（核心：native-parked ctx 满足收集器 target 不死等）+ `native_unpark_guard_round_trips_parked_count`。**2/2 通过**

## 阶段 2: REPL 接线（runtime repl.rs）
- [x] 2.1 `builtin_repl_readline` / `builtin_repl_readblock` 最外层包 `NativeParkGuard::enter`（RAII 覆盖 `?` 早返 + `plain_readline` 回退）
- [x] 2.2 `ReplHelper::complete`（补全器回调）重入 VM 前 `NativeUnparkGuard::exit`（**不放** `complete_via_callback`——与 probe builtin 共用，会 underflow）
- [x] 2.3 `cargo build --release` 无错

## 阶段 3: z42 侧预热 + spawn + 汇合（toolchain）
- [x] 3.1 `z42.scripting.z42.toml` + `z42.interactive.z42.toml`：`[dependencies]` 加 `z42.threading`
- [x] 3.2 `ScriptState.z42`：加 `Thread PrewarmThread` 字段（`using Std.Threading`）+ 构造置 null
- [x] 3.3 `Script.z42`：`public static void Prewarm`——本地 `DepScanResult` + 循环 `EnsurePackageLoaded` + **末尾原子发布** `state.CachedScan = scan`；幂等
- [x] 3.4 `Script.z42`：`Eval` 顶部 `_ensureWarm`（Join worker → 兜底 inline Prewarm）；移除原地首轮 scan 块（放顶部避免 `using` 累积路径与 worker 读 Usings 竞争）
- [x] 3.5 `interactive_main.z42`：REPL 分支 `Create()` 后 `s.PrewarmThread = Thread.Start(() => Script.Prewarm(s))`；`-c` 分支不 spawn
- [x] 3.6 `.reset` 重建 s（PrewarmThread==null）→ inline 兜底路径正确（smoke 验证）

## 阶段 4: 测试与验证
- [x] 4.1 `tests/repl_prewarm/`：驱动 `Script.Prewarm` + Eval 等价 + 幂等（driver.z42 + expected_output.txt）
- [x] 4.2 端到端重叠实测（干净手工装配）：2.5s 打字间隙 → worker +1.99s 完成预热（藏进间隙）、input 命中缓存、post-input eval ~0.44s（vs 无重叠 ~2.3s）；结果正确
- [x] 4.3 `xtask test e2e` 全绿（**goldens 424/0 + cross-zpkg 8/0**，debug VM 含本改动重建）
- [x] 4.4 剩余 GREEN：`xtask test runtime` **865/0**（含 native-park 单测）/ `test compiler` **自举 5/5 gen1==gen2** / `test stdlib` **279 file/23 lib 全绿** / `test vscode-syntax` **grammar 同步**——全 rc=0，零回归

## 阶段 5: 文档 + 归档
- [x] 5.1 `docs/design/toolchain/repl.md`：新增「启动预热」节（时序 + GC-safe park + handoff）+ 更新 Deferred `repl-future-persist-static-scan`（标注预热落地、正交可叠加）
- [x] 5.2 `src/toolchain/scripting/README.md` 功能索引 + 核心文件登记 `Prewarm`
- [x] 5.3 spec scenarios 覆盖：启动预热/间隙重叠/极快退化/首行正确/reset·-c 兜底/worker-GC 不卡死/
  handoff null-or-complete 均已覆盖（实测 + 单测）。「预热窗内 Tab 不死锁」以 native-park 原语单测
  （`native_unpark_guard_round_trips_parked_count`）+ 设计保证覆盖——PTY 交互式 Tab 未在自动门禁复现（需 PTY 夹具）。
- [x] 5.4 归档（mv → archive/2026-07-29-add-repl-prewarm；释放 ACTIVE.md 锁）

## 备注（实施期关键发现）
- **stale xtask.zpkg 陷阱**：隔离 worktree 从主树 `cp` 的 `xtask.zpkg` 早于 `b77d9342`（REPL 引入
  `_buildScriptingLib` 进 build toolchain 编排），导致 `build toolchain` 静默跳过 z42.scripting 构建
  → interactive 编译 `undefined: Script`。**根治**：从 worktree 源重建 `xtask.zpkg` 装回 → build
  toolchain 正常建 scripting（combined 31 zpkgs）+ interactive。教训：warm-copied 工具链 apphost 与
  当前源可能编排漂移；隔离 worktree 若复用主树 xtask 二进制，改了构建编排要重建 xtask。
- **build sdk 不刷新 `.z42`（warm-incremental）**：本地 `.z42/bin/z42vm`+interactive+scripting 停在
  首次冷建（14:03），后续 `build sdk` 未覆盖 → 直接 `./.z42/z42 repl` 跑陈旧 REPL（无 spawn/无
  native-park）。端到端重叠以**干净手工装配**（fresh z42vm + fresh interactive + combined libs）为准；
  冷 CI 无此问题（首次冷 build sdk 即产可用 REPL）。
