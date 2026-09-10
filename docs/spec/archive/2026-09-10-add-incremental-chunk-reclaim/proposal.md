# Proposal: chunk 回收从 O(堆) 改成 O(chunk 数)

## Why

`add-bounded-nursery`（#541）给 minor 加了 chunk 级回收 —— RSS 从 986 MB 压到 608 MB，
但同时把 minor 的中位停顿从约 30 ms 抬到约 75 ms，并留下一条明确的欠债：
**这个 pass 是 O(堆) 而不是 O(young)**。它一个人压着三件事：

1. **minor 停顿的下界** —— nursery 从 8M 拉到 64M，中位停顿只从 67.6 走到 90.3 ms，
   完全不成比例；
2. **nursery 默认值只能停在 32M**（Mono 是 4M）；
3. **分代模式赢不了 STW** —— 分代该赢在「minor 便宜所以能勤跑」。

按「分停顿定位」配方给 minor 的各段套 `Instant` 实测（`z42c.semantics`，分代模式，
后几次大堆 minor）：

| 段 | 耗时 |
|---|---|
| `mark_phase_minor` | ~23 ms |
| `reclaim_dead_chunks` ×2（对象 + 数组） | ~6.5 ms |
| **`reclaim_dead_var_chunks`** | **45–59 ms** |
| sweep 合计 | 53–123 ms |

**变长区那一个函数就占了 sweep 的 85%、整个停顿的约 60%。** 它的成本结构是
「每个块一次二分查找」×270 万块 —— `survey_chunks` 要为每个块回答「你属于哪个 chunk」，
而 `VarRegion` 的块变长、没有「地址 → 槽下标」的算术。

## What Changes

- **`GcBlockHeader` 加 `chunk_idx: u32`，零成本**。头是 `#[repr(C, align(8))]`，
  原有六个字段共 12 字节被 padding 到 16 —— 这个字段正好落进那 4 字节 padding 里，
  **头仍然是 16 字节**（那条 `assert!(size_of == 16)` 是三堆设计的不可动摇约束之一）
- **两个 region 各加一份 per-chunk 普查**（`init/blocks_per_chunk` + `live_per_chunk`，
  变长区另加 `max_gen_per_chunk`），由 alloc / retire / tombstone 增量维护 ——
  「这个 chunk 全死了吗」变成一次比较
- `VarRegion::survey_chunks` + `ChunkIndex` 整个删掉；`purge_blocks` 的归属判定
  从二分查找换成读块头的 `chunk_idx`
- `Region::reclaim_dead_chunks` 不再逐槽扫描；两处 `HashSet` 换成按 chunk 下标的
  标志表（free_list 有几十万条，每条一次哈希查找是普查修完之后剩下的大头）

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/gc/var_region/block.rs` | MODIFY | `chunk_idx` 字段（落在既有 padding 里） |
| `src/runtime/src/gc/var_region/chunk.rs` | MODIFY | `bump`/`alloc_dedicated` 返回 chunk 下标；`retire_chunk`/`push_chunk` 维护普查；`reclaim_dead_var_chunks` 重写；删 `ChunkIndex`/`ChunkSurvey` |
| `src/runtime/src/gc/var_region.rs` | MODIFY | 三个普查向量；`alloc`/`reinit_slot`/`tombstone` 维护；`write_fresh_header` 带 chunk 下标 |
| `src/runtime/src/gc/var_region/var_ref.rs` | MODIFY | leak/test 块用 `u32::MAX` 哨兵 |
| `src/runtime/src/gc/region.rs` | MODIFY | `init_per_chunk`/`live_per_chunk`；`reclaim_dead_chunks` 改 O(chunk)；两处 HashSet → 标志表 |
| `src/runtime/src/gc/var_region_tests.rs` | MODIFY | 头仍 16 字节 + 普查不变量的测试 |
| `docs/book/src/runtime/gc-tlab-chunk-exclusive.md` | MODIFY | per-chunk 普查的机制说明 |
| `docs/spec/changes/add-incremental-chunk-reclaim/` | NEW | 本变更容器 |

## Out of Scope

- **把 `all_blocks` 改成按 chunk 分桶**（`Vec<Vec<ptr>>`）→ 见 design.md「还剩什么」。
  修完之后 `purge_blocks` 的 retain 是仅剩的 O(堆) 项（7–10 ms），但它已经是
  「每元素一次 O(1) 判定」，再往下要动数据结构
- **卡表粒度**（现在是整个 chunk = 256 条都当根）→ `mark_phase_minor` 现在是最大的一段
  （~25 ms），下一个杠杆
- **翻 `Z42_GC_MODE` 默认** → 本 change 之后它第一次成为真实的权衡，见 design.md

## Open Questions

- [ ] 无。
