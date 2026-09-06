# Tasks: 修 GC 预算不生效 —— 增长闸门基线 + chunk 归属查找

> 状态：🟢 已完成 | 创建：2026-09-07 | 完成：2026-09-07

**变更说明：** `Z42_GC_MAX_BYTES` 设了也约束不住堆——自动回收的增长闸门从**回收前的高水位**
起算，导致触发点每回收一次就自涨一格；同时 sweep 尾的 chunk 回收按块线性查所属 chunk，
`O(块数 × chunk 数)`，整个 GC 停顿都花在这里。

**原因：** 两处独立缺陷，合起来让「武装 GC」既不省内存也不划算，于是一直默认关着。
- **闸门基线**：`last_auto_collect_used` 记的是 trip 时（回收**前**）的 `used`，而规则 1 恰好把
  它钉在 `near_limit_ratio × budget`。再次触发要求 `used ≥ last + throttle_ratio × budget`
  = **100% 预算**起步，回收一次涨一格。能干活的回收器把堆压在预算以下 → 第二次永不到来。
- **首次 trip 误判**：产出率读的是「上一次 trip 所要求的那次回收」的战果；第一次 trip 没有
  上一次，读到 0 被当作徒劳 → 回收还没发生，退避倍数已经是 2。
- **chunk 查找**：`reclaim_dead_var_chunks` 对 `all_blocks` 每个块调一次线性扫的 `chunk_of`，
  且 `in_reclaimed` 又对每块线性扫一遍回收区间——两个 `O(块 × chunk)` 项。与 #519 修掉的
  `young_list` 是同一形状，换了个 region。

**文档影响：** `docs/book/src/runtime/gc-tuning-and-safepoint.md`（闸门语义 + 退避策略，
原文「距上次自动回收」描述的正是修复后的语义）、`docs/book/src/runtime/gc-tlab-chunk-exclusive.md`
（D7 chunk 级回收的代价）。

- [x] 1.1 `gc/arc_heap/auto_collect.rs`：增长闸门基线改为 `last - reclaimed_since`（上一次回收
      结束时的水位）；首次 trip（`gc_cycles == 0`）不计徒劳；回收仍挂起时不重复 trip
- [x] 1.2 `gc/arc_heap/auto_collect_tests.rs`：回归测试——可回收的堆必须被压在预算内
      （修前 2.5× 超预算，是真判别器）
- [x] 1.3 `gc/var_region/chunk.rs`：chunk 归属查找改二分（按 base 排序 + `partition_point`），
      `in_reclaimed` 改按 chunk 下标查表；`chunk_of` 随之删除（无其它调用点）
- [x] 1.4 `gc/var_region_tests.rs`：回归测试——dedicated（超大块专属）chunk 与 64K bump chunk
      混排时，回收仍只池化全死的 bump chunk，存活块不受清理影响
- [x] 1.5 文档同步（book 两页）
- [x] 1.6 GREEN

## 实测（z42c.semantics --release --no-incremental，同一次会话）

| 配置 | 周期数 | 总停顿 | 峰值 RSS | 墙钟 | 指令数 |
|---|---|---|---|---|---|
| 未武装（默认） | 0 | — | 1031.7 MB | 6.31 s | 74.75 G |
| 256M · 修前 | 1 | 2.26 s | 892.0 MB | 8.95 s | — |
| 256M · 只修闸门 | 2 | 9.20 s | 927.3 MB | 15.26 s | 247.2 G |
| 256M · 两处都修 | 2 | **0.31 s** | 924.5 MB | 6.94 s | 78.0 G |
| 128M · 两处都修 | 4 | **0.38 s** | **734.0 MB** | 6.40 s | 78.3 G |
| 64M · 两处都修 | 20 | 1.31 s | 634.9 MB | 7.35 s | 87.0 G |

停顿逐周期翻倍的来源已定量：128M 下 `reclaim_dead_var_chunks` 单点占每次停顿的
92–98%（494ms → 975ms → 1490ms → 3072ms，而同期 mark + 两个定长 region 的 sweep 合计
只有 15–45ms）。

**结论**：武装 128M 预算现在近乎白送——墙钟与未武装同在噪声内，RSS −28.9%，指令 +4.8%。

## 备注（后续，不在本次 Scope）

- `used_bytes` 会被 sweep 减成 0：128M 下 cycle 4 报 `used 115.2M -> 0B  freed 137.0M`，
  freed > used_before。`sub_used_bytes` 用 `saturating_sub` 兜住了，但说明 sweep 的
  `freed_bytes` 估算与 `record_alloc` 的计账口径对不上，预算读的数因此偏松。
- RSS 里 GC 堆只占约四成：未武装时退出前 `used_bytes` = 450.0 MB，而 RSS 1031.7 MB。
  另外那 580 MB 不归 GC 管——继续压 RSS 要往非 GC 侧看。
- chunk 内存只回池、不还给 OS（`VarRegion::drop` 才 `dealloc`）。所以 RSS 降的是**高水位**
  （靠复用），不是「回收后还回去」。
