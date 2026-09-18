# Proposal: 周期中的 minor 只把**年轻**的灰条目当根

## Why

`add-incremental-major-gc` M2a 定下一条规则：**一个增量 major 周期打开期间，该周期的灰队列（`mark_queue`）
与 SATB 记录（`satb_queue`）是 minor 的额外根**。理由是对的——标记器手上攥着句柄的对象，
minor 不能把它扫掉再把悬垂句柄交回给标记器。

但这条规则**下手比需要的宽**：它把灰队列里的**每一个**条目都当根，其中绝大多数是**老年代**条目。
而 minor 根本不回收老年代条目——把它们当根唯一的效果是**多追一层它们的子节点**，
而那一层里唯一有意义的（年轻子节点）**卡表已经覆盖了**。

这是 `add-pause-budget-nursery`（M3）挖出、当时明确不在 M3 修的两笔账之一。M3 的实测结论是
**「nursery 买不到 10 ms」**，因为一次 minor 的代价里有一块**与 nursery 无关的固定部分**，
而这条灰队列重复遍历正是其中一个大头：

| 固定成本来源 | 实测量级 |
|---|---|
| 卡表扫描（O(老年代)） | `z42c.semantics` 最坏一次 minor 扫 280 382 个老年代条目 |
| **增量 major 的灰队列作 minor 根** | `13_gc_large_heap --large` 上**每次** minor 重新拷贝并遍历 **108 142** 个老条目 |

灰队列在一个周期里通常有几万到十几万条，而周期打开期间会发生**几十次** minor ——
同一批老条目被反复拷进根集、反复追一层子节点，**每次都白做**。

## What Changes

- **minor 播种灰队列时按年龄分流**：年轻灰条目照旧入根（它们必须被标记，否则 minor 会把标记器
  正拿着的对象扫掉）；**老年灰条目不入根**——它们不会被 minor 回收，其年轻子节点由卡表覆盖，
  这正是 minor BFS 一直以来「不把老年子节点入队」所依赖的同一条不变量。
- **SATB 队列同样处理**——论证与灰队列逐字同构。实测其规模可忽略（0~481 条），
  纳入的理由是**规则的一致性**而非数字；见 design D4，最终由 User 在 gate 上裁决。
- 诊断：`Z42_GC_PHASES` 的 `minor roots` 一行区分「播种的 / 跳过的老条目」，让这笔账此后可见。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/gc/arc_heap/generational.rs` | MODIFY | `mark_phase_minor` 的灰/SATB 播种按年龄分流 + `minor roots` 诊断行 |
| `src/runtime/src/gc/arc_heap_tests/incremental.rs` | MODIFY | 回归：老灰条目的年轻子节点仍活（卡表覆盖）+ 年轻灰条目仍不被回收 + 阴性对照 |
| `docs/internals/src/runtime/gc-incremental-major.md` | MODIFY | 「灰队列/SATB 作 minor 根」一节改写为**年轻条目**，写明卡表论证 |
| `docs/internals/src/runtime/gc.md` | MODIFY | 卡表不变量一节补「周期根集也依赖它」的交叉引用 |

**只读引用**：`gc/arc_heap/barrier.rs`（卡表染脏的唯一入口）、`gc/arc_heap/incremental.rs`（灰队列的生产者）、
`gc/var_region/generation.rs`（var region **无卡表**，见 design D2）。

## Out of Scope

- **不碰 M2b 的第二笔账**（切片把 minor 闸门推远 / `rearm_auto_collect` 的锚点）——独立 change，
  其原型已证明有别的交互（`semantics` 改善但 `--large` 从 22.8 崩到 131.6 ms），风险等级不同，不同锅不同炒。
- **不做增量 minor**（把 minor 本身切片化）——那才是 10 ms 目标的真正解法，是与 M2b 同级的独立线。
- **不改卡表本身**（粒度 / var region 补卡表）——本 change 只是**不再重复**卡表已经做的事。
- **不改 `doomed_unless_marked` 的清扫期语义**——本 change 只动 Marking 期的根集。

## Open Questions

- [x] SATB 队列一并纳入（User 裁决 2026-09-18，理由是规则一致性）
- [x] 验收门槛 = 大堆两档显著下降 + 其余不回归（User 裁决 2026-09-18）
- [ ] 🔴 **门槛没达成**：实测两档都没有可测收益（D5b）。立项时引用的「原型 −8% 总停顿」无法复现，
      且与算术对不上。**落不落待 User 裁决**
