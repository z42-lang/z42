# Tasks: fix-primitives-count-as-young

> 状态：🟢 已完成 | 创建：2026-09-11 | 完成：2026-09-11

**变更说明：** 分代 GC 里三处「这个孩子年轻吗」的判据补上 `is_heap_ref()` 守卫；
顺带补上「数组的 backing 块也算这个条目持有的年轻东西」——去掉前者会暴露后者。

**原因：** `gen_age_of` 对**一切非 GC 引用**（`Null` / `I64` / 栈句柄）答 `0`，而 `0 < threshold`，
于是**每个基元字段、每个空槽都被当成「一个年轻对象」**。

**文档影响：** `docs/book/src/runtime/gc-tuning-and-safepoint.md`。

- [x] 1.1 `mark_phase_minor` 的 BFS 孩子过滤 + `seed_card_entry` + `refers_to_young` 加 `is_heap_ref()`
- [x] 1.2 `ArrayObj::min_backing_gen_age` + `owns_young_backing`：backing 块的年龄要算数
- [x] 1.3 两个不变量测试（都两头反证过）
- [x] 1.4 文档同步
- [x] 1.5 GREEN（`./xtask test` 全绿含自举不动点；`cargo test --lib` debug 1220 passed）

## 实测（`z42c.semantics --release --no-incremental`，base=ae950016，各 3 跑）

| | 墙钟 | 峰值 RSS | 中位停顿 | 最大停顿 |
|---|---|---|---|---|
| base | 6.99 s | 811.1 MB | 21.5 ms | 95.7 ms |
| **本 change** | 6.84 s（−2%） | **783.9 MB（−3.4%）** | **14.4 ms（−33%）** | 94.4 ms |

稳态 minor 的内部形状（同一次构建的第 12–16 周期）：

| | 修前 | 修后 |
|---|---|---|
| 脏卡 | **33 001 张**，扫 **203 884** 条目 | **1–3 张**，扫 1–3 条目 |
| 推入标记队列 | **1 254 097** | **20–35** |
| 其中是基元/Null 的 | **1 254 047**（99.996%） | — |
| `seed_from_dirty_cards` | 10.4 ms | **0.0 ms** |
| minor mark（最终只标 59–112 个对象） | 12.0 ms | **0.1 ms** |
| minor 停顿 | 21.5 ms | **9.5 ms** |

`freed` 逐周期逐字节相同 —— **回收的是同一批对象**，只是不再为找到 ~100 个年轻对象而
扫 20 万条目、克隆 125 万个 `Value`。

## 🔑 为什么「卡永远是脏的」

`refers_to_young` 是 `dirty_cards_for_newly_old_*`（晋升时）和 `rebuild_card_table`（major 后）
共同的判据。`Value::Null` 让它恒为真，于是**实际上每个条目的卡都会被置脏、而且再也清不掉**
（#553 的「扫过即清」也救不了：扫的时候它照样报告「有年轻的」）。
这把 #553 换来的 32× 收缩基本抵消掉了。

## ⚠️ 去掉假信号会暴露一个被它掩盖的真缺陷

**数组的元素存储块（`region_var` 里的 backing）只靠 `mark_backing()` 存活，而那是
*trace 这个数组* 的副作用** —— 它不是数组的任何一个 `Value` 孩子，所以任何「遍历孩子」的
检查都看不见它年不年轻；而 minor 只在卡脏时才 trace 一个老数组。

以前「卡永远脏」意外地保证了每个老数组每次 minor 都被 trace，于是 backing 总被标记。
把判据改诚实之后，**young backing 开始在活着的老数组底下被扫掉**。

症状不是崩溃，而是**自举字节不动点断裂**（gen1≠gen2，`z42c.semantics` 差 167 B）——
单测一个都没红。所以补了 `owns_young_backing`：一个数组只要 backing 还年轻，
它自己就「持有年轻的东西」，卡必须保持脏。

🔑 **教训：「几乎总是真」的判据会掩盖依赖它的缺陷。** 把它修准的那一刻，
所有搭它便车的东西同时暴露 —— 这类改动必须连自举不动点一起跑，单测覆盖不到。

## 备注

- **不在本 change 范围**：`chunk reclaim x3`（现在是稳态 minor 里最大的一块，~8.6 ms /
  9.5 ms 停顿）——`purge_blocks` 对 `all_blocks` 的 retain 是 O(堆)，要把 `all_blocks`
  改成按 chunk 分桶，单独立项。
