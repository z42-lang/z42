# Proposal: 并发探针 —— safepoint park 时长直方图 + 用户锁争用探针

## Why

脚本性能分析程序的并发维度目前只有「z42 无后台 GC/JIT 线程、协作式 safepoint STW」的**静态说明**
（P0 `xtask profile --threads` 只打印 counter 摘要 + 文字 hint），没有任何**量化**：线程在 GC safepoint
被 STW 停了多久、用户 `Std.Threading.Mutex`/`RwLock` 争用了多少次·等了多久，全无数据。写多线程 z42
程序的人无法定位「卡在 GC 暂停」还是「卡在锁争用」。

不做的后果：并发性能问题只能靠猜；`xtask profile --threads` 名不副实（threads 维度无实质并发数据）。

## What Changes

两个探针，**perf 风险分层**：

1. **park 时长直方图（常开，热路径零成本）**：mutator 在 safepoint park（`park_until_idle`，仅 GC 暂停
   时才走的 slow path）时计时，累积进一个复用 `PauseHistogram` 的 per-VmCore 直方图。因只在真实 park 时
   跑，**不在每次 safepoint 检查的热路径上** → 零热路径成本、常开。并入 ProfileSnapshot JSON + xtask profile。

2. **用户锁争用探针（编译期 feature `profile-contention`，默认关）**：给 `Std.Threading.Mutex.Lock` /
   `RwLock` acquire 的阻塞点（`corelib/sync.rs`）加「try_lock 失败才计时」的 contended-path 探针——记录
   争用次数 + 累计等待 µs。**给每次 acquire 加一个 try_lock 分支 = perf 风险** → 编译期 feature 门控，
   默认构建**完全编译掉**（零分支零成本）；profile 时用带 feature 的 throwaway VM 测（复用 P0 `--heap`
   的隔离 target-dir 现建 VM 手法）。

**注**（对记忆表述的精化）：探针落点是**用户级 `Std.Threading` 锁**（`corelib/sync.rs` 的
`builtin_mutex_lock_acquire` 等），**不是** VmCore 的 `mutexes`/`channels`/`vm_contexts` slot-table 锁
——后者只是短暂 HashMap 访问、非争用热点；用户锁 acquire 才是真正的并发瓶颈信号。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/gc/safepoint.rs` | MODIFY | `park_until_idle` 计时 → `park_histogram.record(us)`（常开）|
| `src/runtime/src/vm_context.rs` | MODIFY | `VmCore` 加 `park_histogram: Mutex<PauseHistogram>` + `lock_contentions`/`lock_wait_us`(feature-gated AtomicU64) 字段 + 构造初始化 |
| `src/runtime/src/corelib/sync.rs` | MODIFY | `builtin_mutex_lock_acquire` + RwLock read/write acquire：`#[cfg(feature="profile-contention")]` try_lock→计时 contended path |
| `src/runtime/src/counters.rs` | MODIFY | `ProfileSnapshot` 加 park 派生字段（`park_count`/`park_us_total`/`park_max_us`）+ contention 字段（feature 下非 0）；`to_json`/`Display` 同步 |
| `src/runtime/src/app.rs` | MODIFY | ProfileSnapshot 组装点补 park/contention 来源 |
| `src/runtime/Cargo.toml` | MODIFY | 新 feature `profile-contention`（默认不启用）|
| `scripts/xtask_profile.z42` | MODIFY | `--threads` 展示 park 直方图摘要；`--contention` 子路径现建 feature VM 测争用（或并入 threads）|
| `src/runtime/src/gc/safepoint_tests.rs` | MODIFY | park 计时单测（park 后直方图 count>0）|
| `src/runtime/src/corelib/sync_tests.rs`（如存在）或新 `*_tests.rs` | NEW/MODIFY | feature 下争用计数单测 |
| `docs/design/runtime/diagnostics.md` | MODIFY | §5/§6 记 park 直方图 + contention feature-gate 机制 |
| `src/runtime/README.md` / `src/runtime/src/corelib/README.md`（若有） | MODIFY | 功能索引同步（如涉及）|

**只读引用**：`src/runtime/src/gc/types.rs`（`PauseHistogram` 复用）、`counters.rs`（现 ProfileSnapshot）、
`src/libraries/z42.threading/`（Std.Threading 锁语义）。

## Out of Scope

- **park 直方图暴露到 z42 脚本**（`Std.Diagnostics` API）：本 change 只进 profile JSON + xtask；z42 API 面
  延后（可并入 P1c 的 RuntimeCounters 或独立）。
- **native park**（`NativeParkGuard`，REPL readline 阻塞）计时：只测 safepoint park，不测 native 阻塞。
- **channel recv 阻塞计时**：本 change 只测 Mutex/RwLock；channel 延后。
- **P1c / P2**：各自独立 change。

## Open Questions

- [x] 探针 gate 形式 → 决策 D2（DRAFT 已定）：编译期 feature `profile-contention`（默认零成本），非 env。
- [x] 争用探针落点 → 用户级 Std.Threading 锁（sync.rs），非 VmCore slot-table 锁。
- [ ] park 直方图是否也暴露 8 桶分布到 JSON，还是只 count/total/max？倾向只 count/total/max（JSON 紧凑），
      完整桶留 xtask profile 文字展示（实施时定）。
