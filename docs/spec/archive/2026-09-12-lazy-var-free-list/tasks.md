# Tasks: lazy-var-free-list

> 状态：🟢 已完成 | 创建：2026-09-12 | 完成：2026-09-12

**变更说明：** 变长区的 chunk 回收不再扫 `free_lists`。陈旧条目留在表里，**pop 时**用
per-chunk 的 `pool_epoch` 校验掉；按 size class 精确记账，某类陈旧过半就压缩那一类。

**原因：** `purge_blocks` 对 `free_lists` 做的那次 `retain` 是**每个条目解引用一次块头**，
`O(堆)` 的活干 `O(本次回收的 chunk)` 的事 —— 实测 `z42c.semantics` 上
**142.5 ms / 总停顿 513 ms**（45 次 minor × 最多 1 235 912 条目）。
`retune-gc-nursery-and-promotion-age`（#575）把 nursery 砍到 16M、回收次数从 11 翻到 47 之后，
它成了单项最大开销（33% 的总停顿落在 `minor/chunk reclaim`，其中 27.7% 是这一条）。

**文档影响：** `docs/book/src/runtime/gc-tlab-chunk-exclusive.md` 新增
「free-list 的陈旧条目为什么可以留着」一节，并改掉「和 `all_blocks` / `free_lists` 同一个
`retain`」这句已失效的不变式。

## 任务
- [x] 1.1 `pool_epoch: Vec<u32>`（每 chunk 一个池化计数器）+ `free_epochs`（与 `free_lists`
      并行的 `Vec<u32>`，不用 `(ptr, u32)` 元组——会 padding 到 16 B）
- [x] 1.2 `pop_free_slot`：epoch 对不上就丢弃，循环到拿到可用槽
- [x] 1.3 `purge_blocks`：删掉 `free_lists` 的 retain；顺着被回收 chunk 的块表按 class 记账
- [x] 1.4 `compact_class` / `compact_stale_classes`：某类陈旧过半只压那一类
- [x] 1.5 回归测试四条（见下）
- [x] 1.6 文档同步
- [x] 1.7 GREEN —— `xtask test` 全绿（基于 main `44607c5b`）

## 验证

**两个二进制、各三跑**（base 从 `origin/main` 单独编）：

| | 墙钟 | 峰值 RSS | 停顿合计 | 中位 | p90 | 最大 |
|---|---|---|---|---|---|---|
| main（`44607c5b`） | 7.13 s | 599 MB | 474.7 ms | 9.9 ms | 13.5 ms | 44.5 ms |
| **本 PR** | 7.04 s | **620 MB** | **360.7 ms** | **7.8 ms** | 13.2 ms | 43.0 ms |

`12_gc_churn` 中性（0.57→0.58 s、RSS 142 MB 持平、停顿 187.7→193.1 ms，噪声内）——
它的变长块流失形态不同，这一刀对它没什么可省。

阶段分解上 `minor/chunk reclaim` **188.7 → 35.2 ms（−81%）**。

**代价：峰值 RSS +21 MB（+3.5%）**，来自滞留的陈旧条目 + 并行 epoch 数组：free-list 常驻
内存从约 10 MB 涨到约 34.5 MB（实测 2 336 623 条目、其中 922 433 陈旧）。

⚠️ **p90 基本没动（13.5 → 13.2 ms）。** 这一刀省的是**每次回收一笔近似固定的开销**，
所以整条分布平移约 3 ms —— 对中位（−21%）和总停顿（−24%）很显著，对尾部不显著。
**别把它当尾延迟改进宣传。**

## 测试
- `reclaim_does_not_scan_the_free_lists` —— 直接对 `purge_blocks` 断言表未被动过。
  **有人把 `retain` 加回去，这条会红**（否则那 142.5 ms 会被静默地重新引入）。
- `a_stale_free_entry_is_never_handed_out` —— 正确性主testcase：池化后猛分配，
  断言没有地址被发出两次、且 live 数与发出数一致。
- `pooling_records_the_entries_it_staled` —— 记账精确（总数 + 按类拆分对得上），
  压缩后正好少掉那么多。
- `stale_entries_are_compacted_rather_than_accumulating` —— 浪费有界。

## 备注：量错三次的地方（留给下一个人）
那 +21 MB 我**先后猜错三次**才量对：以为是压缩阈值太松（调紧只换回 6 MB）、
以为是 `Vec` 容量松弛（`shrink_to_fit` 无效，反而因 realloc 尖峰把 RSS 推到 +43 MB）、
以为是 chunk 数变多（实测 chunk 数、live_count 都正常）。
**真相靠的是直接打印 `free_lists` 的 len/capacity/stale**，不是推理。
