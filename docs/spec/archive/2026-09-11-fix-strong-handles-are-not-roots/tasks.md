# Tasks: fix-strong-handles-are-not-roots

> 状态：🟢 已完成 | 创建：2026-09-11 | 完成：2026-09-11

**变更说明：** 把 `handle_slab` 里的 **strong** 槽加进三条标记路径的根集合
（`mark_phase` / `snapshot_roots_into_mark_queue` / `mark_phase_minor`），
并在 retention 图里作为 `Pinned` 根上报。

**原因：** `GCHandle.AllocStrong(target)` **锚不住目标**。`handle_slab` 住在 `RcHeapInner`，
全仓只有 `arc_heap/interface.rs` 的四个 `handle_*` 方法碰它，**没有任何 mark 阶段扫过它**。
于是 Strong 与 Weak 两模的唯一可观察差别只剩「能不能 `downgrade`」——
而 `HandleEntry` 自己的文档注释写的是「strong slots ... **anchor their target across
collection**」。**先前一直存在**（#534 归档时记过，没人修）。

**文档影响：** `docs/book/src/runtime/gc-tuning-and-safepoint.md`（GC 根集合一节）。

- [x] 1.1 `HandleSlab::strong_targets()`（weak 槽刻意不含）
- [x] 1.2 三条标记路径的根集合各加一处
- [x] 1.3 retention 图把 strong 句柄作为 `Pinned` 根上报
- [x] 1.4 三个测试（两个正向 + 一个 weak 反向），全部两头反证过
- [x] 1.5 GREEN

## 三个测试

| 测试 | 钉住的契约 |
|---|---|
| `a_strong_handle_anchors_its_target_across_a_collection` | 完整回收下 strong 槽锚住目标 |
| `a_strong_handle_anchors_its_target_across_a_minor` | **minor 有自己的根集合** —— 只修完整标记，默认收集器下（minor 占绝大多数回收）等于没修 |
| `a_weak_handle_does_not_anchor_its_target` | 反向：weak **不能**锚。没有它，「把每个槽都当根」也能让前两个变绿，却会静默毁掉 `AllocWeak` |

反证：去掉 minor 那一处根，`across_a_minor` 立刻红（`left: 0, right: 1`）。

## 备注

- Strong 槽里的 `StrongAtomic(Value)` 也一并作为根 —— 它可能是 `Value::Str`，那是变长区的一个块。
- **不在本 change 范围**：`for_each_root` / `stats.roots_pinned` 仍只统计 `pin_root`，
  没把句柄算进去。那是 API 语义问题（「pinned」指的是 `pin_root`），不是正确性问题。
