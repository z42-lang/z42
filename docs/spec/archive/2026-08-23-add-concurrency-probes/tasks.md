# Tasks: 并发探针（park 直方图 + 用户锁争用）

> 状态：🟢 已完成 | 创建：2026-08-23 | 完成：2026-08-23

## 进度概览
- [x] 阶段 1: park 直方图（常开）
- [x] 阶段 2: 争用探针（feature-gated）
- [x] 阶段 3: ProfileSnapshot + xtask 展示
- [x] 阶段 4: 测试 + A/B + 文档 + 验证

## 阶段 1: park 直方图（常开，零热路径成本）
- [x] 1.1 `vm_context.rs` VmCore：加 `park_histogram: Mutex<PauseHistogram>` + 构造初始化
- [x] 1.2 `safepoint.rs` `park_until_idle`：入口 Instant::now → 出口 `park_histogram.record(us)`

## 阶段 2: 争用探针（编译期 feature `profile-contention`，默认关）
- [x] 2.1 `Cargo.toml`：`[features] profile-contention = []`
- [x] 2.2 `vm_context.rs` VmCore：`lock_contentions`/`lock_wait_us: AtomicU64` + 构造初始化
- [x] 2.3 `sync.rs` `builtin_mutex_lock_acquire`：`contended_lock` helper（feature 关=原始 arc.lock）
- [x] 2.4 `sync.rs` RwLock read/write acquire：`contended_read`/`contended_write`
- [x] 2.x helper 抽到 `sync_contention.rs`（keep sync.rs 不加重 500 行限；见备注）

## 阶段 3: ProfileSnapshot + xtask 展示
- [x] 3.1 `counters.rs` ProfileSnapshot：加 park×3 + lock×2 字段；`with_concurrency` 链式 setter；to_json/Display 同步
- [x] 3.2 `app.rs`：组装点从 park_histogram + atomics 取值
- [x] 3.3 `xtask_profile.z42` `--threads`：park 展示（默认 VM）+ contention（现建 feature throwaway VM）

## 阶段 4: 测试 + A/B + 文档 + 验证
- [x] 4.1 `safepoint_tests.rs`：`pause_guard_drop_notifies_waiters` 加 `park_histogram.count>=1` 断言
- [x] 4.2 `sync_tests.rs`：cfg-gated `contended_acquire_bumps_contention_counters` + `uncontended_...does_not_bump`
- [x] 4.3 `cargo build`(默认) + `cargo build --features profile-contention` 均通过；cargo test 默认 73 pass / feature sync 46 pass
- [x] 4.4 **A/B bench**：默认 VM vs depscan #265(≈origin/main) 06_thread_scaling jit（99-110ms vs 95-121ms，噪声内无回归）；contention `#cfg`-out → lock 路径机器码与 origin/main 相同
- [x] 4.5 `xtask test`（完整 GREEN gate）—— 全 stage 绿 + self-host 5/5 gen1==gen2 逐字节
- [x] 4.6 spec scenarios 逐条覆盖确认
- [x] 4.7 文档同步：diagnostics.md §5（park + contention 机制 + feature-gate）
- [x] 4.8 self-host 5/5 gen1==gen2；bootstrap 无越界（z42c 源未变，同 P1c）

## 备注
- park 常开无 feature（只在 GC 暂停跑，零热路径）；contention feature-gated（默认零成本，编译掉）。
- 探针落点=用户级 Std.Threading 锁（sync.rs），非 VmCore slot-table 锁。
- 无格式 bump；不新增 z42 API（暴露延后）。
- **端到端实证**：feature VM 多线程抢 Mutex → `lock_contentions=42071`/`lock_wait_us=360331µs`。
- **code-org**：`sync.rs` pre-existing 561 行（>500，非本 change 引入）；本 change 把新 helper 抽到
  `sync_contention.rs`（88 行）避免加重，sync.rs 净增 ~14 行。拆分现有 Mutex/Channel 内容属独立 refactor。
