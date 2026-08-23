# Spec: concurrency-probes（并发探针）

## ADDED Requirements

### Requirement: safepoint park 时长直方图（常开）

#### Scenario: 单线程程序 park 计数为 0 或极小
- **WHEN** 一个不触发 GC 暂停的单线程程序运行结束
- **THEN** ProfileSnapshot 的 `park_count` == 0（无 STW park 发生），`park_us_total` == 0

#### Scenario: 多线程 + GC 暂停累积 park 时长
- **WHEN** 多线程程序运行，期间发生 STW GC 暂停（mutator 在 safepoint park）
- **THEN** `park_count` > 0，`park_us_total` > 0，`park_max_us` ≥ 单次最长 park；这些值并入
  `z42vm_counters` JSON（`--print-stats-on-exit --stats-format json`）

#### Scenario: park 计时不影响热路径
- **WHEN** park 计时逻辑加入 `park_until_idle`
- **THEN** 计时只在 mutator 实际 park（GC 暂停 slow path）时执行；`check_safepoint` 快路径
  （每次 backedge/call）**不含**任何 park 计时代码 → 热路径零新增成本

### Requirement: 用户锁争用探针（编译期 feature `profile-contention`，默认关）

#### Scenario: 默认构建零成本（feature off）
- **WHEN** 默认 `cargo build`（无 `--features profile-contention`）
- **THEN** `builtin_mutex_lock_acquire` 走原始 `arc.lock()` 直路，**无 try_lock 分支、无计时**；
  `lock_contentions`/`lock_wait_us` 恒 0；A/B bench 证默认路径 perf 与 baseline 无差异

#### Scenario: feature 开时记录争用
- **WHEN** 用 `--features profile-contention` 构建的 VM 跑一个多线程争抢同一 `Std.Threading.Mutex` 的程序
- **THEN** `lock_contentions` > 0（try_lock 失败次数），`lock_wait_us` > 0（阻塞 acquire 累计等待）；
  无争用（try_lock 成功）时两者不增

#### Scenario: xtask profile 展示并发数据
- **WHEN** `xtask profile --threads <script>` 运行
- **THEN** 输出含 park 直方图摘要（count / total_us / max_us）；争用数据在带 feature 的 profile 路径下展示

## IR Mapping

无新 IR 指令 / 无 zbc·zpkg 格式变更（纯运行时计数 + 直方图，不触及二进制格式）。

## Pipeline Steps

- [x] VM interp / runtime —— safepoint.rs park 计时 + sync.rs 争用探针 + VmCore 字段
- [x] counters —— ProfileSnapshot park/contention 字段
- [x] toolchain —— xtask profile 展示 + feature VM 构建
- [ ] Lexer/Parser/TypeChecker/Codegen —— 无
