# Tasks: chunk 回收改成 O(chunk 数)

> 状态：🟢 已完成（2026-09-10）

| 阶段 | 状态 |
|---|---|
| 1. 分停顿定位 | ✅ |
| 2. `chunk_idx` 进块头（免费）+ 两个 region 的普查 | ✅ |
| 3. 测试 | ✅ |
| 4. GREEN + miri + 实测 + 文档 + 归档 | ✅ |

## 阶段 1: 分停顿定位

- [x] 给 minor 的各段套 `Instant`（照 [[z42-gc-disarmed-quadratic-sweep]] 的「分停顿定位」配方）
- [x] 结果：`reclaim_dead_var_chunks` **45–59 ms**，占 sweep 的 85%、整个停顿约 60%；
      定长区两个合计 ~6.5 ms；`mark_phase_minor` ~23 ms
- [x] 认定根因：`survey_chunks` 为 270 万块**每块一次二分查找**回答「你属于哪个 chunk」

## 阶段 2: 实现

- [x] `GcBlockHeader.chunk_idx: u32` —— 落在 `#[repr(C, align(8))]` 既有的 4 字节 padding 里，
      **头仍是 16 字节**
- [x] `bump` / `alloc_dedicated` 返回 `(ptr, chunk_idx)`；`fill` 盖 claim 的 chunk 下标
- [x] `VarRegion`：`blocks_per_chunk` / `live_per_chunk` / `max_gen_per_chunk`，
      在 alloc / reinit / retire / tombstone / push_chunk / 回收 六处维护
- [x] 删掉 `survey_chunks` + `ChunkIndex`；`purge_blocks` 改读块头的 `chunk_idx`
- [x] `Region<T>`：`init_per_chunk` / `live_per_chunk`；`reclaim_dead_chunks` 改 O(chunk)
- [x] 两处 `HashSet` 换成按 chunk 下标的标志表（free_list 几十万条，每条一次哈希查找
      是普查修完之后剩下的大头：**5–9 ms → 0.44 ms**）

## 阶段 3: 测试

- [x] `chunk_idx_fits_in_the_headers_existing_padding`（16 字节守卫）
- [x] `every_block_carries_its_owning_chunk`
- [x] `the_per_chunk_census_matches_a_full_scan`（**普查算错 = 错回收还有活块的 chunk**，
      比慢严重得多，所以要跟全量扫描对账）
- [x] 既有的 6 个 chunk 回收测试原样通过 —— 判定没变，只是变快了

## 阶段 4: GREEN / miri / 实测 / 文档

- [x] `./xtask test` 全绿
- [x] miri（`gc::var_region`）—— 本 change 动了块头布局与每一处 raw 写入
- [x] 实测见 design.md
- [x] `docs/book/src/runtime/gc-tlab-chunk-exclusive.md`
- [x] 归档

## 交给后续 change 的发现（按大小排）

1. 🔴 **`mark_phase_minor` ~25 ms，现在是 minor 里最大的一段**。它是
   O(young + 脏卡条目)，而**卡是 chunk 粒度的** —— 一个脏 chunk 里 256 条活条目
   **全部**当根；#539 之后晋升也会置脏卡，脏集更大。**细化卡粒度是下一个杠杆。**
2. **`purge_blocks` 的 retain 7–10 ms**：每元素已是 O(1) 判定，但要摸 270 万个块头
   （随机读）。要再往下得把 `all_blocks` 改成按 chunk 分桶（`Vec<Vec<ptr>>`），
   回收一个 chunk 就是 `clear()`。
3. **翻 `Z42_GC_MODE` 默认现在是真实的权衡了**：分代中位停顿 32.5 ms vs STW 58.8 ms，
   代价 RSS +2.9% / CPU +2%。在这之前分代两头都输。要翻仍需先补 CI 的分代覆盖。

## 验收标准

- 块头仍是 16 字节
- per-chunk 普查与全量扫描对账一致
- 被回收的 chunk 集合与修改前相同（RSS 不变）
- minor 的 chunk 回收从 O(堆) 变成 O(chunk 数)
