# Tasks: 同步原语的值存进 GC 堆

> 状态：🟢 已完成 | 完成：2026-09-14（DRAFT 同日确认）| 创建：2026-09-14 | 分支 / worktree：`store-sync-values-in-heap` @ `../z42-nullflake`

## 进度概览
- [x] 阶段 0: 退回对照基线
- [x] 阶段 1: Monitor 原生原语
- [x] 阶段 2: stdlib 重写
- [x] 阶段 3: 测试与验证
- [x] 阶段 4: 文档与归档

## 阶段 0: 退回对照基线（先证明测试有判别力）
- [x] 0.1 写 `src/libraries/z42.threading/tests/gc_sync_values_rooted.z42`（Channel / Mutex / RwLock 各一条 + Write 存入新值一条），
      在**未改动**的 VM + stdlib 上跑（interp / jit / debug VM），记录失败输出

## 阶段 1: Monitor 原生原语
- [x] 1.1 `src/runtime/src/metadata/types/object.rs`：`NativeData::Monitor(Arc<Monitor>)`
- [x] 1.2 `src/runtime/src/corelib/monitor.rs`：`Monitor { state, cv }` + `enter / try_enter / exit / wait`（按 design D4 的顺序铁律）
      + 合成 TypeDesc `Std.Threading.MonitorHandle` + 五个 builtin
- [x] 1.3 `src/runtime/src/corelib/sync_contention.rs`：`profile-contention` 探针接到 `enter` 慢路径
- [x] 1.4 `src/runtime/src/corelib/mod.rs` 声明模块；`builtin_table_ext.rs` 追加登记
- [x] 1.5 `src/runtime/src/corelib/sync.rs` 模块头注释：阶段 1 种子例外、待删
- [x] 1.5b `src/runtime/src/corelib/native_decl_tests.rs`：19 个旧 builtin 加入豁免清单（全量 `cargo test` 暴露，实施中补入 Scope）
- [x] 1.6 `src/runtime/src/corelib/monitor_tests.rs`：spec「Monitor 原生原语」6 个场景

## 阶段 2: stdlib 重写
- [x] 2.1 `src/libraries/z42.core/src/Native/ThreadingNative.z42`：加 `MonitorNative`，删 `MutexNative` / `RwLockNative` / `ChannelNative`
- [x] 2.2 `src/libraries/z42.threading/src/Mutex.z42`
- [x] 2.3 `src/libraries/z42.threading/src/RwLock.z42`（design D5）
- [x] 2.4 `src/libraries/z42.threading/src/Channel.z42`（无界扩容 / 有界 / 容量 0 会合 / 出队清槽）
- [x] 2.5 `src/libraries/z42.threading/tests/sync_semantics_edges.z42`：重入抛异常、关闭后 Send、会合、扩容保序、跨线程 GC 压力

## 阶段 3: 测试与验证
- [x] 3.1 `cargo build` release + debug；全量 `cargo test`（1361 passed）
- [x] 3.2 `test stdlib z42.threading` interp + `--mode jit`；0.1 的用例由红转绿
- [x] 3.3 debug VM 跑 0.1 用例：无 `use-after-finalize`
- [x] 3.4 `test stdlib z42.net` 工作台串行 12 轮：修复后 **0/12**。⚠️ 修复前同条件没有数据（前一晚高负载下也是 0/10，历史 1/6~1/8），**不据此宣称 flake 已修**；threaded 用例不经过这三个原语，另见 PR #641（park 期间分配）
- [x] 3.5 性能对比：`06_thread_scaling` 只经 Channel 传 4 个值、测不出同步开销，改用专门微基准 `.repro/syncbench.z42`（结果见机制页「性能」）
- [x] 3.6 `xtask test` 全 stage GREEN（最终改动后重跑一次）
- [x] 3.7 spec 场景逐条覆盖确认（`profile-contention` feature 下 `cargo test --features profile-contention` 争用探针两条通过）

## 阶段 4: 文档与归档
- [x] 4.1 `docs/book/src/runtime/sync-primitives.md` + `docs/book/src/SUMMARY.md`（含 Deferred 段：阶段 2 删旧 builtin）
- [x] 4.2 `docs/roadmap.md` Deferred Backlog Index 加索引行
- [x] 4.3 `src/runtime/src/corelib/README.md`、`src/libraries/z42.threading/README.md`
- [x] 4.4 `.claude/rules/runtime-rust.md`：原生层不得在根集之外持有 `Value`
- [x] 4.5 归档到 `docs/spec/archive/`，随 PR 一起提交

## 备注
- **0.1 退回对照（修复前，main `2843abebe`）**：interp 7/8 FAIL（`VCall: expected object, got Null` / `__box_prim: expected integer value, got Null`，
  与 flake 签名同形）；JIT 下测试宿主本身被 `VCall: expected object, got Null` 打崩；唯一通过的是「被丢弃的 Mutex 不保留值」（防泄漏回归，修复前本就成立）。
  修复后 interp / JIT 均 8/8。
- **debug VM 在当前 main 上跑任何 z42b 单元都会崩**（`region_object invariant violation: alive young entry not in young_list`，
  `thread_basic` 同样崩）⇒ 与本变更无关的既有问题；3.3 改用不经 z42b 的单文件复现（`.repro/chan.z42` / `mtx.z42`）：
  修复前 `use-after-finalize`，修复后值完好。
- **Monitor 用两个条件变量**（design D4 的细化）：「等进入」与「等信号」共用一个条件变量时，两个同时 `wait` 的线程会互相唤醒空转。
  `two_waiters_do_not_spin_against_each_other` 做过退回对照：单条件变量写法下安静期空转 **42149 次**。
- **删除 `Mutex/RwLock/Channel.SlotId()`**：槽位不复存在，全仓无调用方（`Thread.SlotId()` 不受影响）。
- **性能**：锁类原语变快（Mutex 0.21×、RwLock.Read 0.46×），跨线程高频争用的有界 Channel 变慢 1.60×（`enter` 自旋前 2.15×）。
- **本地工具链**：`#635` 把依赖升到需要 rustc 1.95+，本机默认 1.88 ⇒ 构建一律 `RUSTUP_TOOLCHAIN=1.98.1`。
- 调查记录与复现源：`.repro/`（未跟踪）、记忆 `concurrency-null-thread-flake.md`。
