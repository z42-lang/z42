# Tasks: 大对象的 chunk 在死后归还分配器

> 状态：🟢 已完成（2026-09-08）

## 进度概览

| 阶段 | 状态 |
|---|---|
| 1. `Chunk` 原地释放 + 墓碑槽位 | ✅ |
| 2. `reclaim_dead_var_chunks` 分两路 + 拆函数 | ✅ |
| 3. 测试 | ✅ |
| 4. GREEN + 实测 + 文档 + 归档 | ✅ |

## 阶段 1: `Chunk` 原地释放 + 墓碑槽位

- [x] `Chunk::free_in_place`：`dealloc` + `base` 置 dangling + `cap = 0`；幂等
- [x] `Chunk::is_freed`（`cap == 0`）
- [x] `VarRegion::free_chunk_slots` 字段（两个构造点：`Default` / `with_drop_glue`）
- [x] `push_chunk` 优先复用墓碑槽位，并重置 `borrowed` / `reuse_gen`
- [x] `VarRegion::drop` 跳过墓碑槽位（否则 double free）

## 阶段 2: `reclaim_dead_var_chunks` 分两路 + 拆函数

- [x] 提取 `ChunkIndex`（地址 → chunk 下标的二分；建表时跳过墓碑槽位）
- [x] 提取 `survey_chunks`（`has_live` / `max_gen` / `any_block` 普查）
- [x] 提取 `partition_dead_chunks`：`cap == CHUNK_BYTES` → 入池，否则 → 释放
- [x] 提取 `purge_blocks`：`all_blocks` / `free_lists` / `young_list` 一次 retain 清扫两组
- [x] 返回值换成 `VarChunkReclaim { pooled, freed_chunks, freed_bytes }`
- [x] `chunk_count()` 改成「仍持有内存的 chunk 数」；新增 `chunk_slot_count()`

## 阶段 3: 测试

- [x] `dead_oversized_chunk_is_freed_and_its_slot_reused`
- [x] `live_oversized_chunk_is_never_freed`
- [x] `oversized_churn_does_not_grow_the_per_chunk_tables`
- [x] `freeing_a_dedicated_chunk_leaves_bump_chunk_indices_valid`
- [x] `oversized_var_blocks_are_freed_not_leaked`（端到端）
- [x] 既有 `reclaim_dead_var_chunks_*` / `reclaimed_chunk_purges_young_list` 跟随返回值变化
- [x] **反证**：临时去掉释放那一路 → 端到端测试红，chunk 数 `[4, 8, 12, 16, 20, 24]`

## 阶段 4: GREEN + 实测 + 文档 + 归档

- [x] `./xtask test` 全绿
- [x] miri（`cargo +nightly miri test --manifest-path src/runtime/Cargo.toml --lib gc::var_region`）
      —— 本 change 直接动 raw 内存的生命周期，是 miri 最该跑的一类；第一轮就抓到一处
      **测试自己写的** UAF，见「实施期发现」
- [x] 实测 RSS：128M −8.0 MB / 256M −16.0 MB，墙钟持平（见 design.md）
- [x] `docs/book/src/runtime/gc-tlab-chunk-exclusive.md` 补 chunk 生命周期三种归宿
- [x] 归档到 `docs/spec/archive/`

## 实施期发现

- **miri 抓到的是测试自己写的 UAF**：`live_oversized_chunk_is_never_freed` 里原本写了
  `assert!(r.resolve(dead).is_none())` —— 那个块的 chunk 刚被释放，`resolve` 会去解引用
  已还给分配器的内存。生产代码没问题，测试才是越界方。已改成显式注释「这里不能 resolve」，
  正好把决策 2 的代价钉在测试里。**结论：miri 对本 change 是必跑项，不是走过场。**
- 🔎 **交给后续 change 的发现（不在本 change 修）**：`ArcMagrGC` 的 **GC handle table
  （`handle_slab`）根本不是 mark 的根** —— `mark_phase` 的根只有 `inner.roots` +
  external scanner，`handle_slab` 在全仓的引用只有 `arc_heap.rs:175` 的字段声明和
  `interface.rs` 的四个方法。也就是说 `GCHandle.AllocStrong(target)`（`z42.core`
  的 `__gc_handle_alloc`）**根本锚不住目标**，Strong / Weak 两模的差别只体现在能不能
  `downgrade`。这是先于本 change 存在的缺陷，本 change 只是让它在 > 64 KB 的字符串上
  从「resolve 返回 None」恶化成 UAF。没有任何测试断言过「强句柄能扛住一次回收」。

## User 裁决（2026-09-08，均已落进 proposal / design）

1. **`Z42_GC_LOH_BYTES` → 推到 `add-bounded-nursery`**（design.md 决策 4）：门槛做成运行时值
   要在最热的分配路径上加一次全局原子读，而今天没有消费者；和 change 3 的 profile 实验
   一起进、一起 A/B。
2. **失效模式变差 → 接受，按 D-3 原案**（design.md 决策 2）：影响面只有 > 64 KB 的块，
   且前提是标记本身有 bug；代价已写进 design.md 与 book。

## 验收标准

- 死掉的 oversized 块的内存在同一次 major sweep 结束时已归还分配器
- 大对象反复生灭的负载下 chunk 内存与槽位表都达到稳态（不单调增）
- 其余 chunk 的下标身份不受释放影响
- `used_bytes` 计账一个字节不变
