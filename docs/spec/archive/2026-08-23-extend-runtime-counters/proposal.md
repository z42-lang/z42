# Proposal: 扩展运行时计数 —— 补零 + 分配/堆分代进 profile 快照（P1a）

## Why

脚本性能分析程序 P0（`xtask profile` + `--stats-format json`，已合 #246）落地了 CLI 与 JSON
快照通道，但快照内容仍薄：**GC 维度基本缺席**。

- **分配次数** 已在 `HeapStats.allocations`（`arc_heap.rs:723` 每次分配 bump），但从未进入 profile
  JSON 快照 —— 用户跑 `xtask profile --heap` 看不到脚本到底分配了多少次对象。
- **GC 分代**：generational 收集器内部区分 minor / major 收集，但 `HeapStats` 只 surface 合计
  `gc_cycles`，没有 minor/major 拆分，也没有 `reclaimed_bytes`（每轮回收字节）——无法判断脚本
  的 GC 压力是"频繁小收集"还是"偶发大收集"。

> **事实校正（2026-08-23）**：本 change 初稿曾把 `native_calls`/`exceptions_thrown`/`exceptions_caught`
> 当作"恒 0 待埋点"（承自 P0 memory）。核查发现三者早在 **2026-05-26（"Phase 2 D3+D6" wiring）就已
> 埋点生效**（`exec_native.rs:40`、`interp/mod.rs:1007/1024`）并进入 P0 快照/JSON 输出 —— 只有
> `counters.rs` 的**文档注释**陈旧未更新。因此本 change 的"补零"工作收窄为**修正陈旧注释**；真正的
> 计数增补集中在 GC 维度（allocations + 分代）。

这是 `docs/design/runtime/diagnostics.md` §8 **阶段 3「计数扩」**（L1 计数层）的第一刀：把已有但
未 surface 的 GC 数字接上 profile 快照。不触发 zbc/zpkg 格式 bump（纯计数，避开自举两代墙）。

**不做会怎样**：profile 报告长期缺分配与 GC 分代维度，P0 建的 `--heap` 通道形同虚设，用户无法用它
定位分配热点或判断 GC 行为。

## What Changes

- **堆分代拆分**（HeapStats 新增三字段，不动 `gc_cycles` 合计）：
  - `minor_collections` / `major_collections` ← `run_cycle_collection_minor/major` 各自 bump
  - `reclaimed_bytes` ← 收集周期已算出的 `freed_bytes` 累加
- **合并 profile 快照**：在 `--print-stats-on-exit` 输出点（`app.rs`）除了 `RuntimeCounters` 快照，
  再拉 `ctx.heap().stats()`，把 `allocations` + 三个堆分代字段并入**同一行 JSON**（沿用 P0 的
  `z42vm_counters` sentinel，超集扩展，对 scraper 后向兼容）。text 形式同步显示。
- **profile 报告显示**：`scripts/xtask_profile.z42` 解析并展示新字段（分配次数、native 调用、异常、
  GC minor/major/reclaimed）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/counters.rs` | MODIFY | 新增 `ProfileSnapshot`（counter Snapshot + 堆派生 u64 字段的合并视图）+ `to_json()`；**修正陈旧注释**（native/exceptions 三字段实际已埋点 2026-05-26，非"待埋点"） |
| `src/runtime/src/gc/types.rs` | MODIFY | `HeapStats` 加 `minor_collections`/`major_collections`/`reclaimed_bytes` 三字段 + 注释 |
| `src/runtime/src/gc/arc_heap.rs` | MODIFY | 收集周期 minor/major/reclaimed 各自 bump（gc_cycles bump 点就近） |
| `src/runtime/src/app.rs` | MODIFY | 快照输出点合并 `ctx.heap().stats()` → `ProfileSnapshot`，text/json 两路均含新字段 |
| `scripts/xtask_profile.z42` | MODIFY | 解析 + 展示新字段 |
| `src/runtime/src/counters_tests.rs` | NEW | `ProfileSnapshot::to_json` 单测（含新字段 + sentinel） |
| `src/runtime/src/gc/heap_tests.rs` | MODIFY | HeapStats 新字段默认 0 + 收集后 minor/major/reclaimed 单调 |
| `src/tests/profile/counters_surface/` | NEW | e2e golden：跑一个抛异常 + 分配的脚本，`--print-stats-on-exit` 输出含非零 native/exceptions/allocations |
| `docs/design/runtime/diagnostics.md` | MODIFY | §1 现状表 + §5 覆盖清单：allocations/minor/major/reclaimed 标"已 surface 到 profile" |
| `src/runtime/src/gc/README.md` | MODIFY | 功能索引：HeapStats 分代字段 |
| `src/toolchain/README.md` 或 `scripts/README.md` | MODIFY | profile 报告新字段（如涉及命令面文档） |

**只读引用**（理解上下文，不改）：

- `src/runtime/src/interp/exec_native.rs`（:40）/ `src/runtime/src/interp/mod.rs`（:1007/:1024）— native/exceptions 计数**既有埋点**，本 change 不动，仅核对/引证
- `src/runtime/src/interp/exec_instr.rs` — `CallNative` 派发点，确认 call_native 是唯一 native 入口
- `src/runtime/src/gc/heap.rs` — `stats()` trait 方法签名
- `src/runtime/src/corelib/gc.rs` — `Std.HeapStats` 现有 7 字段投影（本 change **不动** z42 侧 API 面）
- `docs/design/runtime/diagnostics.md` §8 — 阶段蓝图

## Out of Scope

- **`Std.Diagnostics.counters()` / `Std.GC` z42 侧 API 扩展**：把新数字暴露给 z42 脚本自省是 **P1c**
  （新 stdlib API + bootstrap 两-nightly 纪律）。本 change 只让数字进 **profile JSON 快照**，不碰
  `z42.core` stdlib 与 `corelib/gc.rs` 的 TypeDesc 投影。
- **并发探针**（safepoint park 时长、Mutex 争用）：**P1b**（有 perf 回归风险，单独 A/B）。
- **确定性 allocations CI gate**：**P1c**（先 informational 观察几轮）。
- **histogram / gauge kind、span、事件层**：diagnostics.md §5 后续 + §8 阶段 4/5。
- 不触发 zbc/zpkg 格式 bump。

## Open Questions

- 无（拆分粒度与 gate 形式已在会话中裁决：三拆 + gate 先 informational）。
