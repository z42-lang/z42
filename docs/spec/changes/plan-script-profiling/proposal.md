# Proposal: 脚本性能分析方案（三阶段程序）

> 状态：DRAFT，待 User 确认。
> 类型：**program-plan**（非单一 change）。P0 属 toolchain，P1/P2 属 vm——各阶段实施时
> 各开自己的 change 容器；P1/P2 为 vm 类，届时走完整 spec-first 流程。

## Why

z42 运行一门脚本（`z42vm <compiled.zbc/.zpkg> [entry] [--mode …]`）时，当前的性能可观测能力
分散且有明显空白：

- **e2e 只有 wall-clock**：`xtask bench` 用 hyperfine 测墙钟时间，**不测内存 / RSS**
  （[`.github/workflows/bench-pr.yml`](../../../../.github/workflows/bench-pr.yml) 明确 harness 不追踪 RSS）。
- **计数器框架齐、埋点稀疏**：[`src/runtime/src/counters.rs`](../../../../src/runtime/src/counters.rs)
  的 `RuntimeCounters` 只有 `builtin_calls` / `jit_*` 已埋点，`native_calls` / `exceptions_*` **恒 0**；
  **没有分配计数、没有指令计数、没有 per-函数计时**。
- **GC 统计丰富但代际盲**：[`gc/types.rs:288`](../../../../src/runtime/src/gc/types.rs) `HeapStats`
  + `PauseHistogram` 已暴露到 z42（`GC.GetStats()`），但只有合计 `gc_cycles`，**没有 minor/major
  拆分、没有 reclaimed_bytes**。
- **零外部 profiler 集成**：全仓无 perf / valgrind / flamegraph / heaptrack 任何脚本化集成。
- **无 script 级归因**：无法回答"**哪个 .z42 函数 / 行**最热、分配最多"——只能靠 native profiler
  看到 Rust 侧栈帧。
- **已有设计未落地**：[`docs/design/runtime/diagnostics.md`](../../../../docs/design/runtime/diagnostics.md)
  §5/§7 已设计好 counter 扩展 + `--trace-out` perfetto 导出，但**均未实现**。

不做的后果：性能回归只能靠噪声大的 wall-time gate（跨 runner ±60% 抖动）挡，分配 / 内存回归静默
进 main；优化 VM 或脚本都缺乏归因数据，只能盲猜。

## What Changes

分三阶段，按 ROI 递减、工程量递增：

### P0 — 外部工具 + 统一入口（toolchain，零 VM 改动）

- 新增 `xtask profile <script.z42>` 子命令，四维开关 `--cpu` / `--heap` / `--threads` / `--e2e` / `--all`。
- 集成 **samply**（跨平台 CPU 火焰图，mac+linux 同一命令）与 **dhat**（Cargo `dhat-heap` feature，
  跨平台堆分配归因）。
- 封装现成 knob：`--print-stats-on-exit` / `GC.GetStats()` / `Z42_STACKALLOC=stats` /
  `Z42_LOG=z42::gc=trace` / hyperfine + `/usr/bin/time` 抓 peak-RSS。

### P1 — RuntimeCounters 扩展 + 确定性 CI gate（vm）

- `RuntimeCounters` 补 `allocations`（在 [`gc/arc_heap.rs`](../../../../src/runtime/src/gc/arc_heap.rs)
  分配路径埋点）+ 补齐 `native_calls` / `exceptions_*`。
- `HeapStats` 拆 `minor_collections` / `major_collections` / `reclaimed_bytes`。
- 并发计数：safepoint 每线程 park 时长、三把共享 `Mutex`（`vm_contexts` / `mutexes` / `channels`，
  [`vm_context.rs`](../../../../src/runtime/src/vm_context.rs)）的 acquire 等待时长 + 争用次数。
- 经 `Std.Diagnostics.counters()` 暴露到 z42 脚本。
- peak-RSS 入 `bench/results/e2e.json`（schema v2 已预留 memory metric 位）。
- **确定性 CI gate**：`allocations` 计数可复现 → 设严格阈值门禁（不受 runner 时间噪声影响）。

### P2 — script 级归因 + trace 导出（vm，实现 diagnostics.md §7）

- **safepoint 驱动的采样 profiler**：复用已有 safepoint 轮询（[`gc/safepoint.rs`](../../../../src/runtime/src/gc/safepoint.rs)），
  命中时记录当前 `(function, ip)` 到采样缓冲 → 聚合成 **z42 源码级火焰图**，用 `.zsym` 离线符号化
  映回函数 / 行。
- `--trace` / `--trace-out=trace.json`（**perfetto / chrome trace 格式**）：全程时间线 + 每线程
  run/park/GC/lock-wait 分段。
- 分配点采样归因（"哪个 z42 函数在狂分配"）。

## Scope（分阶段，各阶段实施时在各自 change 容器细化）

本 plan 只登记文件半径，**精确 Scope 表由各阶段 change 的 proposal 给出**。

**P0（toolchain）**
| 文件 | 类型 | 说明 |
|------|------|------|
| `scripts/xtask_profile.z42` | NEW | `xtask profile` 实现 |
| `scripts/xtask.z42` | MODIFY | 注册 `profile` 子命令 |
| `src/runtime/Cargo.toml` | MODIFY | 加 `dhat-heap` feature + dev-dep |
| `src/runtime/src/main.rs` | MODIFY | dhat 全局 allocator 门控（feature-gated）+ `--print-stats-on-exit` 结构化 JSON（`--stats-format=json`，供 `xtask profile` 汇总；见 OQ3 裁决） |
| `scripts/README.md` | MODIFY | 功能索引 + 基础用法 |

**P1（vm）** — `src/runtime/src/counters.rs` / `gc/arc_heap.rs` / `gc/types.rs` / `vm_context.rs` /
`corelib/gc.rs` / `src/libraries/z42.core/src/Diagnostics/*` / `scripts/xtask_bench.z42` /
`.github/workflows/bench-pr.yml` / `docs/book/`（知识上浮）。

**P2（vm）** — `src/runtime/src/gc/safepoint.rs` / `interp/` 采样钩子 / 新 trace 导出模块 /
`main.rs`（`--trace` flag）/ `.zsym` 符号化接线 / `docs/book/`。

## Out of Scope

- 不改 zbc / zpkg 二进制格式（加计数器 / trace 不触发格式 bump，避免自举两代墙）。
- 不做 AOT profiling（AOT 尚未落地，M9）。
- samply / dhat **不进 CI**（火焰图是交互产物、太慢；CI 只吃确定性 counter）。
- 不引入 async profiling（runtime 未实现 async）。

## Open Questions

- [ ] `docs/design/runtime/diagnostics.md` 是 P1/P2 的设计蓝图，但按 doc-system D2 该目录不再更新。
      P2 实现时应将其**知识上浮到 `docs/book/`**（新机制页），diagnostics.md 随迁移清理——避免同一
      设计两处各写（规范冲突）。需 User 确认迁移落点。
- [ ] `allocations` CI gate 的阈值策略：绝对次数还是相对基线百分比？先 informational 观察几轮再定 gate 阈值？
- [x] **（已裁决 2026-08-22）** P0 顺带把 `--print-stats-on-exit` 输出结构化为 JSON（`--stats-format=json`），
      便于 `xtask profile` 汇总。这轻微触碰 `main.rs`（严格说 P0 不再 100% 零 VM 改动），User 已确认接受
      ——JSON 只加一条输出分支，不改 VM 执行语义。

## 续推

程序级续推口令见 memory：**「推进脚本性能分析」**。
