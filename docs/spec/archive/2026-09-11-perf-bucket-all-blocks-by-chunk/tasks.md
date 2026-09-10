# Tasks: perf-bucket-all-blocks-by-chunk

> 状态：🟢 已完成 | 创建：2026-09-11 | 完成：2026-09-11

**变更说明：** `VarRegion::all_blocks` 由一个扁平 `Vec` 改成**按 chunk 分桶**
（`Vec<Vec<NonNull<GcBlockHeader>>>`，下标即 chunk 索引）。

**原因：** `reclaim_dead_var_chunks` → `purge_blocks` 要把被回收 chunk 的块从三张表里摘掉，
对 `all_blocks` 做 `retain`，**每个元素解引用一次 header 读 `chunk_idx`**（一次随机访存）。
实测：为摘掉 ~600 个 chunk，扫了 **1 875 000** 个块，**8.1 ms / 9.5 ms 的 minor 停顿**。
分桶后这一步是 `all_blocks[ci] = Vec::new()`，O(被回收的 chunk 数)。

**文档影响：** `docs/book/src/runtime/gc-tuning-and-safepoint.md`。

- [x] 1.1 `all_blocks` 分桶 + `all_blocks_iter()` 扁平视图（4 处遍历点改用它）
- [x] 1.2 `push_chunk` / `retire_chunk` / `purge_blocks` 的桶维护
- [x] 1.3 三处测试跟着容器形状改（断言的**意图**不变）
- [x] 1.4 GREEN（`./xtask test` 全绿含自举不动点；`cargo test --lib` debug 1218 passed）

## 实测（base=ae950016，各 3 跑）

| | 墙钟 | 峰值 RSS | 中位停顿 | 最大停顿 |
|---|---|---|---|---|
| base | 7.20 s | 791.2 MB | 22.1 ms | 97.8 ms |
| **本 change** | 7.33 s | **790.2 MB** | **18.3 ms（−17%）** | **89.1 ms（−9%）** |

## ⚠️ 两个把 RSS 吃回去的坑（都踩过、都量过）

1. **`Vec` 的翻倍空闲容量 ×每个 chunk 一份** → +20 MB。
   修法：chunk 一旦「填满不再增长」就 `shrink_to_fit()` ——
   TLAB 路径在 `retire_chunk`，ambient 路径在 `bump()` 换 chunk 时。
2. **`clear()` 保留容量** → 每次 minor 回收 ~600 个 chunk，它们的桶清空后仍各握 ~2 KB 不放，
   在 region 里永久累积 → +18 MB。**修法：`= Vec::new()`，真正把内存还回去。**
   坑 1 修完 RSS 只从 811.8 降到 809.8 —— 说明主因是坑 2，两个都得修。

## 备注

- **下一步（同一条线）**：`purge_blocks` 现在只剩 `free_lists` 要扫（~122 万条目，约 3.2 ms）。
  它按 size class 组织、不按 chunk 分区，所以要么给每条目内联 chunk 索引（+4 B/条 ≈ 5 MB），
  要么改成 pop 时惰性校验。**需要单独设计，别顺手做。**
