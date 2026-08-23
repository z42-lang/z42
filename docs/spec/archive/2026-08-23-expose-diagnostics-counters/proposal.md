# Proposal: 把运行时诊断计数暴露给 z42 脚本（Std.Diagnostics.RuntimeStats）

## Why

脚本性能分析程序 P0（`xtask profile` + counter JSON sentinel）/ P1a（堆分代快照并入 ProfileSnapshot）
已把运行时可观测面做进 **profile JSON + 外部 `xtask profile`**，但这些计数**进不了 z42 脚本自身**——
脚本想在运行中读自己的 alloc / GC / 异常计数，只能靠外部进程 scrape stderr JSON。缺一个
第一方 z42 API。同时，`allocations`（P1a 已 surface 到 JSON、确定性、跨 runner 无噪声）已具备做
**CI 分配回归 gate** 的条件，但还没接入 CI（wall-time 跨 runner ±60% 噪声不能做硬 gate，见
[[bench-regression-04-cross-runner-noise]]，`allocations` 才是确定性信号）。

不做的后果：脚本无法自省运行时开销（写 benchmark / 自适应逻辑都得靠外部工具），且分配回归只能靠
人肉看 `xtask profile` 输出、无 CI 兜底。

## What Changes

- **新 builtin `__diag_counters`**（append 到 `BUILTINS` 末尾，BuiltinId 不移位）：把
  `ctx.counters().snapshot()` + `ctx.heap().stats()` 投影成一个 z42 对象。
- **新 z42 stdlib 面 `Std.Diagnostics.RuntimeStats`**（`z42.diagnostics` 库）：`static class Runtime` +
  `Counters()` 返回一个 `RuntimeCounters` 值对象（**全景**：counter 7 字段 + `allocations` +
  分代 `minor/major/reclaimed`）。z42.core 的 `Std.HeapStats`（7 字段）**保持不动**——全景归 Diagnostics。
- **CI `allocations` 回归 gate（informational）**：一个固定脚本跑一遍取 `allocations`，CI 打印并与
  baseline 比对，**只打印不 fail**（观察 3–5 轮跨 OS / GC-mode 稳定性后，再由后续 change 选绝对
  golden 或相对% 转硬 gate）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/corelib/diagnostics.rs` | MODIFY | 加 `builtin_diag_counters`：投影 counter+heap 快照成 `Std.Diagnostics.RuntimeCounters` 对象（按名投影，复用 `alloc_named`）|
| `src/runtime/src/corelib/mod.rs` | MODIFY | `BUILTINS` 末尾 append `("__diag_counters", diagnostics::builtin_diag_counters)`（带日期注释，不插中间）|
| `src/libraries/z42.diagnostics/src/RuntimeStats.z42` | NEW | `namespace Std.Diagnostics; public static class Runtime` + `[Native("__diag_counters")] public static extern RuntimeCounters Counters();` |
| `src/libraries/z42.diagnostics/src/RuntimeCounters.z42` | NEW | `namespace Std.Diagnostics; public class RuntimeCounters` —— 11 只读 auto-property（VM-written）|
| `src/libraries/z42.diagnostics/src/README.md` | MODIFY | 功能索引 + 核心文件表加 Runtime/RuntimeCounters |
| `src/runtime/src/corelib/diagnostics_tests.rs` | NEW/MODIFY | `builtin_diag_counters` 投影单测（字段数、值来源）|
| `src/libraries/z42.diagnostics/tests/runtime-counters/source.z42` | NEW | z42 端到端：调 `RuntimeStats.Counters()`，断言字段可读 |
| `src/libraries/z42.diagnostics/tests/runtime-counters/expected_output.txt` | NEW | golden 期望输出（用数值断言规避非确定字段）|
| `scripts/xtask_profile.z42` | MODIFY | （可选）threads/e2e 摘要注明计数现可经 `Std.Diagnostics.RuntimeStats.Counters()` 脚本内自取 |
| `.github/workflows/bench-pr.yml` | MODIFY | 加 informational `allocations` 步（跑固定脚本取 alloc，打印 + 与 baseline 对比，不 fail）|
| `scripts/xtask_bench.z42` 或新 `bench/scenarios/` gate 脚本 | MODIFY/NEW | CI gate 用的固定 allocations 采集脚本（若现有 bench 场景不合用）|
| `docs/book/src/runtime/diagnostics.md`（若已存在）或对应机制页 | MODIFY | 计数暴露面知识上浮（`Std.Diagnostics.RuntimeStats.Counters()` + informational gate）|
| `docs/design/runtime/diagnostics.md` | MODIFY | §5「暴露到 z42」标注 P1c 已落地（该文件迁移期，仅刷对齐注记）|

**只读引用**：

- `src/runtime/src/counters.rs` — `Snapshot` / `ProfileSnapshot` 字段（投影来源）
- `src/runtime/src/gc/types.rs` — `HeapStats` 字段（分代/allocations 来源）
- `src/runtime/src/corelib/gc.rs` `builtin_gc_stats`（gc.rs:418）— 位置投影范式参考
- `src/libraries/z42.diagnostics/src/Heap.z42` / `Retainer.z42` — 现有 `Std.Diagnostics` 声明范式
- `src/runtime/src/app.rs`（ProfileSnapshot 组装点）— 值来源一致性对账

## Out of Scope

- **确定性硬 gate 阈值**：本 change 只做 informational；转硬 gate 是后续 change（先观察几轮）。
- **P1b 并发探针 / P2 采样 profiler**：各自独立 change。
- **`Std.HeapStats` 补分代字段**：不动 z42.core 的 HeapStats（全景归 Diagnostics.RuntimeStats）。
- **事件订阅 / span（diagnostics.md §7）**：不在本 change。

## Open Questions

- [x] z42 端全景字段落点 → 决策 D3：新 `Std.Diagnostics.RuntimeCounters`（全 11 字段），HeapStats 不动。
- [x] CI gate 形式 → 决策 D4：先 informational。
- [ ] `Counters()` 方法名大小写：现有 stdlib 惯例 PascalCase（`GetStats`）→ 用 `Counters()`（PascalCase 名词复数，作快照访问器语义符合惯例）。实施时与 naming 规范复核。
