# Proposal: 按停顿预算自适应 nursery —— 让 minor 也有停顿上界

## Why

`add-incremental-major-gc` 之后，**major 的停顿已经与堆大小脱钩**（切片 ≤ 2.2 ms）。剩下的最大停顿**全部来自 minor**：

| 负载 | 最大停顿 | 构成 |
|---|---|---|
| `z42c.semantics`（16M nursery） | 17~19 ms | minor mark ∝ 年轻代 |
| `13_gc_large_heap` | 82~89 ms | 同上，且被徒劳退避放大过的 gate 喂了更大的年轻代 |
| `09_alloc_ctorless` | 104~118 ms | 100% 存活，minor 扫的就是整个活堆 |

实测一次 26.2 ms 的 minor：**mark 21.2 ms（334 136 条）+ sweep 5.0 ms + chunk reclaim 0.14 ms** ——
即 **~63 ns/条**，而条数 ∝ nursery 字节数。nursery 是**唯一**能直接买到 minor 停顿上界的旋钮，
但它今天是一个**固定常量**（`Z42_GC_NURSERY_BYTES`，默认 16M）：调小它伤吞吐、调大它伤停顿，
而「多大合适」取决于负载的存活率与对象大小，静态值必然在某些负载上错得离谱。

同一根线的历史证据：`retune-gc-nursery-and-promotion-age` 把 32M 调到 16M 时，
`09_alloc_ctorless` 墙钟回归 18%，`z42c.semantics` 的停顿却还差得远 —— 一个常量同时满足两者是做不到的。

## What Changes

- **按实测代价定 nursery**：每次 minor 记录「停顿微秒 / 本次消耗的 nursery 字节」，用 EWMA 平滑，
  下一次的 nursery = `目标停顿 / 每字节代价`，夹在 `[最小, 最大]` 之间并限制单次变化幅度。
- **新旋钮 `Z42_GC_PAUSE_TARGET_MS`**（默认 10，设 `0` 关闭自适应回到固定 nursery）。
  `Z42_GC_NURSERY_BYTES` 仍然有效：显式设了它就是关掉自适应（它变成「我知道我在干什么」的手动挡）。
- **徒劳退避不再放大 minor 的年轻代**：退避的乘数只作用于「何时再考虑回收」，
  不得把一次 minor 要扫的年轻代放大到停顿预算之外（`13_gc_large_heap` 的 85 ms minor 就是这么来的）。
- 诊断：`Z42_GC_PHASES` 打一行「本次 minor 的每字节代价 / 下一次 nursery」。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/gc/arc_heap/pause_budget.rs` | NEW | 每字节代价的 EWMA + 下一次 nursery 的计算（纯函数 + 单测） |
| `src/runtime/src/gc/arc_heap/pause_budget_tests.rs` | NEW | 上述纯函数的单测（收敛、夹紧、抖动、关闭开关） |
| `src/runtime/src/gc/arc_heap.rs` | MODIFY | 持有 `PauseBudget`；`mod pause_budget;` |
| `src/runtime/src/gc/arc_heap/construct.rs` | MODIFY | 初值：读 `Z42_GC_PAUSE_TARGET_MS` / 显式 nursery 则关闭 |
| `src/runtime/src/gc/arc_heap/control.rs` | MODIFY | minor 结束时把（停顿 µs, 消耗字节）喂给 `PauseBudget` |
| `src/runtime/src/gc/arc_heap/auto_collect.rs` | MODIFY | `nursery_bytes()` 改读自适应值；退避乘数不再放大 minor 年轻代 |
| `src/runtime/src/gc/arc_heap/generational.rs` | MODIFY | minor 记录本次扫过的年轻条目数（喂代价模型） |
| `src/runtime/src/config.rs` / `config/parse.rs` / `config/knob_table.rs` | MODIFY | `Z42_GC_PAUSE_TARGET_MS` |
| `src/runtime/src/gc/arc_heap_tests/auto_collect_tests.rs` | MODIFY | 退避不再放大年轻代的回归测试 |
| `docs/internals/src/runtime/gc-tuning.md` | MODIFY | 旋钮表 + 「nursery 不再是常量」一节 |
| `docs/internals/src/runtime/gc-incremental-major.md` | MODIFY | 「剩余停顿来自 minor」一节指向本 change |
| `src/tests/perf/scenarios/*` | 只读 | 验收用既有场景，不新增 |

**只读引用**：`gc/arc_heap/promotion_policy.rs`（晋升年龄与 nursery 是一对，见 design D3）、
`docs/internals/src/runtime/gc-tuning.md` 的「过早晋升」一节。

## Out of Scope

- **不改晋升年龄的自适应逻辑**（`promotion_policy`）——只在 design 里说明两者如何互不打架。
- **不做 minor 的增量化**（把 minor 也切片）——那是另一条更大的线；本 change 只把 minor 的**输入规模**管住。
- **不碰 chunk reclaim 的 O(chunks)**：实测 0.14 ms，不是当前瓶颈；若 nursery 缩到 1M 级别它才会浮出来，届时单独开。
- **不动 M2b 的 RSS 账**（chunk 碎片 / 空洞复用）——见 `add-incremental-major-gc` tasks 备注。

## Open Questions

- [ ] 默认目标停顿定 **10 ms**（与 User 裁决的「先 ≤10 ms 后 ≤5 ms」一致）还是直接 5 ms？
- [ ] `Z42_GC_NURSERY_BYTES` 显式设置时是否**必须**关掉自适应（本提案主张：是，手动挡优先）？
- [ ] nursery 下界定多少：1M（够小到把 semantics 压到 ~2 ms）还是 4M（避免过早晋升把老年代喂爆）？
