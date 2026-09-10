# Proposal: 卡从「一个 chunk 一张」改成「一个 chunk 32 张」+ 扫过即清

## Why

`add-incremental-chunk-reclaim`（#552）把 chunk 回收从 O(堆) 改成 O(chunk 数)，
minor 中位停顿 −57%。之后 **`mark_phase_minor` 成了 minor 里最大的一段（~25 ms）**，
并留下一条明确的线索：**卡是 chunk 粒度的**。

给 mark 的种子与 BFS 分别计时（`z42c.semantics`，分代，后几次大堆 minor）：

| minor | 固定根 | **脏卡根** | BFS | **最终标记数** |
|---|---|---|---|---|
| 早期 | 2 040 | 27 608 | 9.3 ms | 292 062 |
| 中期 | 3 015 | 379 821 | 21.1 ms | 180 199 |
| **后期** | 2 955 | **518 530** | 20.1 ms | **76** |
| **后期** | 3 061 | **518 531** | 21.3 ms | **95** |

**后期的 minor 播了 51.8 万个脏卡根，只为了找到 76 个年轻对象。** 两个原因叠加：

1. **一张卡盖住整个 chunk 的 256 条**：一次跨代写就把 256 条全变成根；
2. **卡从来不清**：major 之前没有任何东西清它，而写屏障**和晋升**（#539）都在往里加 ——
   脏集因此滚到活堆的约 65%。

## What Changes

- **`card_dirty` 的 32 个位全用起来**。它一直是 `Vec<u32>` 却**只用了 bit 0**，
  所以 32 张卡 × 每张 8 条（`CHUNK_SIZE / 32`）**不花一分钱内存** ——
  和 #552 那次「块头对齐 padding 里塞 `chunk_idx`」是同一种免费空间。
  `mark_card_dirty(chunk_idx)` → `mark_card_dirty(chunk_idx, entry_idx)`
- **扫过即清**：minor 扫一张卡时顺便判断它是否还指向年轻对象；不指向就清掉它。
  卡是「这里**可能**有跨代边」的提示，不是事实 —— 扫过一次发现没有，就该停止当根。
  写屏障与 `dirty_cards_for_newly_old_*`（#539）会在需要时重新置脏
- 播种时**老条目直接穿透**（追踪它、把年轻孩子入队），不再推进 BFS 再让它出队重来一遍
  —— #537 之后 BFS 对老条目本来就只做这件事

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/gc/region/generation.rs` | MODIFY | `CARDS_PER_CHUNK` / `ENTRIES_PER_CARD` / `card_of`；`mark_card_dirty` 带 entry；新增 `clean_card`；`iterate_dirty_cards` 按位遍历并回传卡号 |
| `src/runtime/src/gc/region.rs` | MODIFY | `generation` 模块提升为 `pub(crate)`（测试要用常量） |
| `src/runtime/src/gc/arc_heap/generational.rs` | MODIFY | `seed_from_dirty_cards` + `seed_card_entry` + 卡的清扫记账；四处 `mark_card_dirty` 传 entry |
| `src/runtime/src/gc/region_tests.rs` | MODIFY | 「只点亮自己那张卡」「清一张不动其它」两个新测试 + 既有卡测试跟随 |
| `docs/book/src/runtime/gc-tuning-and-safepoint.md` | MODIFY | 卡粒度与「扫过即清」的机制说明 |
| `docs/spec/changes/add-finer-card-granularity/` | NEW | 本变更容器 |

## Out of Scope

- **把年轻的脏卡条目从根里去掉** —— 试过，**多 −24% 停顿但 +6.3% RSS**，见 design.md
  决策 3。它是一次**语义**改动（改的是「收什么」），该单独立项、单独拿证据
- **`purge_blocks` 的 retain（7–10 ms）** → 要把 `all_blocks` 改成按 chunk 分桶
- **翻 `Z42_GC_MODE` 默认** → 前置仍是补 CI 的分代覆盖

## Open Questions

- [ ] 无。
