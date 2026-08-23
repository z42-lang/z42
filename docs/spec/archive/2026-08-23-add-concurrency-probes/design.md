# Design: 并发探针（park 直方图 + 用户锁争用）

> 脚本性能分析程序 P1b。统一 DRAFT（P1b/P1c/P2）已 User 批准；本文件是 P1b 分支。
> **A/B 隔离**：本 change 独立 worktree/PR，因 contention 探针有 perf 回归风险，须单独 bench 对比。

## Architecture

```
mutator @ safepoint (GC 暂停):
  park_until_idle(ctx)              [safepoint.rs] —— 常开
    ├─ start = Instant::now()
    ├─ wait on gc_phase_cv until Idle
    └─ core.park_histogram.record((now-start).us)   → PauseHistogram（复用）

user lock acquire (Std.Threading.Mutex.Lock):
  builtin_mutex_lock_acquire(ctx)   [sync.rs] —— feature-gated
    #[cfg(feature="profile-contention")]
    ├─ if arc.try_lock() 成功 → 用它（无争用，不计）
    └─ else 争用: contentions+=1; t=Instant::now(); guard=arc.lock(); wait_us+=elapsed
    #[cfg(not(feature))]
    └─ guard = arc.lock()            // 原始直路，零成本

output:
  app.rs ProfileSnapshot { …, park_count, park_us_total, park_max_us,
                           lock_contentions, lock_wait_us }  → JSON / Display
  xtask profile --threads → 展示
```

## Decisions

### Decision D1: park 直方图复用 `PauseHistogram`（gc/types.rs）
`PauseHistogram`（8 log 桶 + min/max/total/count + rolling window，`record(us)`）已是通用暂停时长直方图。
park 时长语义同构 → 直接复用，`VmCore` 加 `park_histogram: Mutex<PauseHistogram>`。不新造直方图类型。

### Decision D2: park 计时位置 = `park_until_idle`（slow path，零热路径成本）
`park_until_idle`（safepoint.rs:170）只在 mutator 观察到 `Requested|Marking` 相位时调用（真实 GC 暂停）。
`check_safepoint` 快路径（每 backedge，throttle 后偶尔进 slow）**不 park**、不计时。故 park 计时对热路径零成本
→ **常开无需 feature**。计时用 `std::time::Instant`（Rust 侧无 sandbox Date 限制）。

### Decision D3: 争用探针编译期 feature `profile-contention`（默认关）—— perf 风险隔离
争用探针必须包裹**每次** `Std.Threading.Mutex.Lock` acquire（try_lock 分支 + 计时），即使无争用也付 try_lock
一次。多线程锁密集程序上这是真实开销 → **编译期 feature 门控**：
- 默认构建：`#[cfg(not(feature="profile-contention"))]` = 原始 `arc.lock()`，**零分支零成本**（探针代码不编译）。
- profile 构建：`--features profile-contention`，探针生效。
**选编译期 feature 而非 env 运行时开关**：env 开关每次 acquire 都要读 flag + 分支（即便关着也付分支成本）；
编译期 feature 关时代码物理不存在 → 真零成本。代价：profile 争用要单独构建一个 VM（下条）。

### Decision D4: profile 争用走 throwaway feature VM（复用 --heap 手法）
`xtask profile` 的 `--heap` 已有「用 `--features dhat-heap` 隔离 target-dir 现建一个 throwaway VM」的成例。
争用 profile 同法：`--threads` 若请求 contention（或 `--contention` 子标志），现建 `--features
profile-contention` 的 throwaway VM 跑一次取 `lock_contentions`/`lock_wait_us`。park 直方图常开、走默认 VM 即可。

### Decision D5: A/B 验证 = 默认构建 perf 不回归
本 change 的 perf 风险全在 contention 探针，而它 feature-off 时物理不编译 → 默认路径按构造零成本。
**A/B**：默认 `cargo build` 的 VM 跑 `xtask bench`（尤 06_thread_scaling 线程负载），对比 origin/main baseline，
证 park 计时（常开）+ feature 关的 contention 代码**不引入回归**（park 只在 GC 暂停跑，bench 热循环基本不 park）。

### Decision D6: 不动格式 / 不动 z42 API 面
纯运行时计数 + 直方图，无 zbc/zpkg 格式变更；park/contention 只进 profile JSON（scraper 后向兼容超集）+
xtask 展示，**不新增 z42 stdlib API**（暴露到脚本延后，见 Out of Scope）→ 无 bootstrap 两-nightly 约束。

## Implementation Notes

- **VmCore 字段**（vm_context.rs）：
  ```rust
  pub(crate) park_histogram: Mutex<crate::gc::types::PauseHistogram>,
  // feature 下才真正被写；字段常在（避免 cfg 污染构造），feature 关时恒 0
  pub(crate) lock_contentions: std::sync::atomic::AtomicU64,
  pub(crate) lock_wait_us:     std::sync::atomic::AtomicU64,
  ```
  构造在 `new_internal` 补默认值。
- **park 计时**（safepoint.rs `park_until_idle`）：入口 `let __t = Instant::now();`，`drop(phase)` 前
  `ctx.core.park_histogram.lock().record(__t.elapsed().as_micros() as u64);`。注意 park 时不能触发 GC / 分配。
- **争用探针**（sync.rs `builtin_mutex_lock_acquire` + RwLock read/write）：
  ```rust
  #[cfg(feature = "profile-contention")]
  let guard = match arc.try_lock() {
      Some(g) => g,
      None => {
          ctx.core.lock_contentions.fetch_add(1, Relaxed);
          let t = std::time::Instant::now();
          let g = arc.lock();
          ctx.core.lock_wait_us.fetch_add(t.elapsed().as_micros() as u64, Relaxed);
          g
      }
  };
  #[cfg(not(feature = "profile-contention"))]
  let guard = arc.lock();
  ```
  （RwLock 同理 try_read/try_write。保留现有 `mem::forget` + force_unlock 语义不变。）
- **ProfileSnapshot**（counters.rs）：加 `park_count`/`park_us_total`/`park_max_us`/`lock_contentions`/
  `lock_wait_us`，`to_json` 追加键（超集，scraper 兼容），`Display` 补行。app.rs 从 `park_histogram.lock()`
  （count/total_us/max_us）+ atomics 组装。
- **Cargo.toml**：`[features]` 加 `profile-contention = []`（默认 features 不含它）。

## Testing Strategy

- **单元测试**（safepoint_tests.rs）：构造多 VmContext + 触发 STW park，断言 `park_histogram.count > 0`。
- **单元测试**（sync feature 测试，`#[cfg(feature="profile-contention")]`）：两线程争抢同 Mutex，
  断言 `lock_contentions > 0`。默认构建下该测试 cfg-out。
- **A/B bench**：默认 VM `xtask bench --mode both`（含 06_thread_scaling）对比 origin/main baseline，证无回归。
- **VM 验证**：`xtask test`（完整 GREEN）；`cargo test --lib`；`cargo build --features profile-contention` 编译通过。
- **端到端**：`xtask profile --threads` 输出含 park 摘要。

## Deferred

- **park/contention 暴露到 z42 脚本**（`Std.Diagnostics` API）：本 change 只 profile JSON + xtask。
- **native park / channel recv 阻塞计时**：延后。
- **per-thread park 归属**（哪个线程 park 最久）：本 change 聚合到 VmCore 级；per-thread 归属延后。
