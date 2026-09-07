# Proposal: 大对象死后永不归还 —— dedicated chunk 只在 VM 退出时才释放

## Why

超过 `CHUNK_BYTES`（64 KB）的变长块走 **dedicated chunk**：一块按 payload 精确定尺、
只装它一个的独立 malloc。而这类块死后没有任何一条路径把内存还回去：

- `VarRegion::tombstone`（`gc/var_region.rs`）显式跳过 `OVERSIZED_CLASS`，不入 `free_lists`
  —— 变长块不复位对齐，尺寸各异的 dedicated 块没有可复用的 size class；
- `reclaim_dead_var_chunks`（`gc/var_region/chunk.rs`）用 `cap != CHUNK_BYTES` 明确把它们
  排除在 `var_free_chunk_pool` 之外；
- **全仓唯一的 `dealloc` 在 `VarRegion::drop` 里**。

于是**一个死掉的大对象把它的内存一直攥到 VM 退出**。实测 `z42c.semantics --release
--no-incremental`：退出时仍活着的有 173 块 / 上界约 27 MB；死掉的那些不在这个统计里，
而它们的 chunk 一样还占着 —— 加探针实测，一次 256M 预算的构建里有 **12.0 MB**、
128M 预算里有 **19.3 MB** 的 dedicated chunk 在这一路上死掉却从未归还。

这是三堆 GC 设计里的缺陷 D3（设计文档 `04 关键决策 · D-3`），也是四个 change 里的第 2 个。
它必须排在 `add-bounded-nursery` / `arm-gc-by-default` 之前：**大对象堆是三堆里唯一
「不参与分代、只在 major 回收」的一堆**，如果它连 major 都不归还内存，后面把 GC 默认
武装起来就是在一个漏底的桶上调水位。

## What Changes

- `reclaim_dead_var_chunks` 在 sweep 尾把整块死掉的 chunk 分两路处理：`cap == CHUNK_BYTES`
  的 bump chunk 照旧入池复用，**dedicated chunk 直接 `dealloc` 还给全局分配器**（决策 D-3
  选项 B —— 按 size 分桶入池已被否决：173 块跨 80 KB–1.3 MB，池子必退化成不还的内存）
- `Chunk` 支持**原地释放**：留下 `cap == 0` 的墓碑槽位。槽位不能删 —— `bump_chunk` /
  `borrowed` / `reuse_gen` / `var_free_chunk_pool` 全按**下标**寻址，`Vec::remove` 会把
  后面每个 chunk 悄悄改号
- 墓碑槽位进 `free_chunk_slots`，被 `push_chunk` 复用 —— 否则 chunk churn 会让三张
  per-chunk 表以每个死大块 ~29 B 的速度只增不减，RSS 仍然单调增，只是慢 3000 倍
- `reclaim_dead_var_chunks` 的返回值从 `usize`（入池数）换成 `VarChunkReclaim`
  （`pooled` / `freed_chunks` / `freed_bytes`），让「真还回去了多少」可观测
- 顺手把 90 行的 `reclaim_dead_var_chunks` 拆成 `ChunkIndex` / `survey_chunks` /
  `partition_dead_chunks` / `purge_blocks`（原函数越过 60 行函数硬限）

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/gc/var_region/chunk.rs` | MODIFY | `Chunk::free_in_place` / `is_freed`；`push_chunk` 复用墓碑槽；`reclaim_dead_var_chunks` 分池/释放两路 + 拆函数；`VarChunkReclaim` |
| `src/runtime/src/gc/var_region.rs` | MODIFY | `free_chunk_slots` 字段（两个构造点）；`Drop` 跳过墓碑槽；`chunk_count` 改为「仍持有内存的 chunk 数」+ 新增 `chunk_slot_count` |
| `src/runtime/src/gc/var_region_tests.rs` | MODIFY | 释放 / 存活不释放 / churn 稳态 / 下标不错位四组单测；既有 reclaim 测试跟随返回值变化 |
| `src/runtime/src/gc/arc_heap_tests/tlab.rs` | MODIFY | 端到端：大字符串反复生灭后 chunk 数达到稳态 |
| `docs/book/src/runtime/gc-tlab-chunk-exclusive.md` | MODIFY | chunk 生命周期的三种归宿（存活 / 入池 / 释放）+ 墓碑槽位的机制说明 |
| `docs/spec/changes/fix-loh-never-freed/` | NEW | 本变更容器（proposal / design / specs / tasks） |

**只读引用**（理解上下文必须读，但不修改）：

- `src/runtime/src/gc/arc_heap/collect.rs` — sweep 尾的调用顺序（对象 → 数组 → 变长 → 三个 reclaim）
- `src/runtime/src/gc/arc_heap/control.rs` — STW / 并发两条路径都在 mutator 停住时才 sweep
- `src/runtime/src/gc/var_region/var_ref.rs` — `VarGcRef` 的「地址 + 16 位 generation」身份契约

## Out of Scope

- **`Z42_GC_LOH_BYTES` 旋钮**（把硬编码的 64 KB 门槛开出来）→ **User 裁决：推到
  `add-bounded-nursery`**，和它的第一个真实消费者（profile 实验）一起进、一起 A/B。
  理由见 design.md 决策 4
- **有界 nursery / 按容量触发 minor** → `add-bounded-nursery`
- **GC 默认武装 + 默认值怎么定** → `arm-gc-by-default`
- **大对象按 size 分桶入池** → 决策 D-3 已否决，别再推一遍
- **消灭变长区碎片**（回收后 70.5 MB 卡在「≥75% 是垃圾但不全空」的 chunk 上）→ 不移动就吃不掉，
  本路线明确不追求

## Open Questions

- [x] **失效模式变差要不要接受** → **User 裁决：接受，按 D-3 原案**。入池的 chunk 内存还在，陈旧 `VarGcRef` 解引用读到的是
      活着的块头、`reuse_gen` 对不上 → 优雅地退化成 `None`（这正是 #533 那个
      `expected string, got Null` 的形状）。释放掉的 chunk 没有这层网：陈旧句柄就是
      use-after-free。**只影响 oversized 块**，且前提是标记有 bug（标记正确时死块不可达，
      不存在活结构还持有它的句柄）。详见 design.md 决策 2
- [x] **`Z42_GC_LOH_BYTES` 并入本 change 还是推到 `add-bounded-nursery`** → **User 裁决：
      推到 change 3**。门槛变成运行时值意味着 `class_for`（最热的分配路径）要读一个全局原子；
      今天没有任何消费者，第一个真正要用它的是 change 3 的 profile 实验。详见 design.md 决策 4
