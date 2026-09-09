# Proposal: 有界新生代 —— 让分代模式真正压得住内存

## Why

三堆路线的 change 3。前两个计划外的前置（#537 / #539）把分代模式修得**跑得通**了，
于是第一次能诚实地量它 —— 结果是**它比完全不回收还差**：

| 配置 | 周期 | minor/major | 峰值 RSS |
|---|---|---|---|
| 未武装 | 0 | — | 1013.8 MB |
| stw 128M | 14 | 0/14 | 606.8 MB |
| **gen 128M（修前）** | 15 | 15/1 | **986.3 MB** |

两个原因，都不是「major 跑得不够多」：

1. 🔴 **minor 从不做 chunk 级回收**。`reclaim_dead_chunks` / `reclaim_dead_var_chunks`
   只在 `sweep_phase`（major）里调。minor 把条目 tombstone 掉，但**一块 chunk 都不还** ——
   而 TLAB 是**整块**发给 mutator 的，一阵短命对象通常整块死，正是 minor 该收的形状。
   加上这三行调用：RSS 986.3 → **607.9 MB**（同样的 15 minor / 1 major）。
2. 🔴 **闸门只有一个，而且读错了量**。`used_bytes` 是**活字节**，minor 靠回收年轻垃圾
   把它压得很低 —— 于是「用量接近预算」这个条件永远满足不了两次，而**死掉的老对象
   （只有 major 会扫）在旁边不断堆高**。分代需要的是**两个**闸门：
   新生代按分配量触发 minor，老年代按**晋升字节数**触发 major。

顺带还有两个直接的计算错误：

3. **升级启发式把「晋升」当成了「死亡」**。存活率算的是 `young_after / young_before`，
   而熬过阈值的幸存者会**离开 young 表** —— 阈值是 2，于是任何一次 minor 的幸存者
   大多被算成死了，存活率永远低于 `Z42_GC_MINOR_THRESHOLD`，**升级从没触发过**。
4. **徒劳退避的判据被新闸门带偏**：产出率原本对着 `throttle_ratio × limit` 比，
   一旦增长闸门换成 nursery（预算的四分之一），一次收掉大半个 32M nursery 的健康 minor
   也会被判成「不足 32M → 徒劳」，退避每次翻倍。实测 minor 从 18 次掉到 8 次、
   堆干脆不收了。

## What Changes

- **minor 尾部做 chunk 级回收**（三个 region 各一次）—— RSS 的主修
- **新生代闸门**：`Z42_GC_NURSERY_BYTES`，不设时 = `MAX_BYTES / 4`。
  分代模式下增长闸门换成它，不再要求 `used` 先爬到 `near_limit × 预算`
- **老年代闸门**：新增 `promoted_bytes_since_major` 计数器（在 minor sweep 里累加 ——
  **分配路径零成本**），超过预算就要一次 major；major 结束清零
- `pending_major` 标志把「要哪种回收」从策略层传到延迟到 safepoint 的执行层
- 修存活率口径（改成 `1 - reclaimed / young_before`）与徒劳判据（对着 throttle 比，不是 nursery）
- 四个新测试 + 两个 config 测试

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/gc/arc_heap/generational.rs` | MODIFY | minor 尾部 chunk 回收；晋升字节累加 + major 清零；`MinorSweepResult` 带回 `reclaimed_entries` / `promoted_bytes` |
| `src/runtime/src/gc/arc_heap/auto_collect.rs` | MODIFY | 分代双闸门（nursery / 晋升字节）+ `nursery_bytes()` + 徒劳判据修正 |
| `src/runtime/src/gc/arc_heap/control.rs` | MODIFY | 消费 `pending_major`；存活率口径修正 |
| `src/runtime/src/gc/arc_heap.rs` | MODIFY | `promoted_bytes_since_major` / `pending_major` 两个原子 + 测试访问器 |
| `src/runtime/src/gc/arc_heap/construct.rs` | MODIFY | 初始化两个新字段 |
| `src/runtime/src/gc/region.rs` | MODIFY | `free_chunk_pool_len_for_test` |
| `src/runtime/src/config.rs` / `config/parse.rs` / `config/knob_table.rs` | MODIFY | `Z42_GC_NURSERY_BYTES`（字节大小解析器提取成共用的 `parse_byte_size`） |
| `src/runtime/src/gc/arc_heap/auto_collect_tests.rs` | MODIFY | nursery 闸门会先于预算闸门触发；且只在分代模式下生效 |
| `src/runtime/src/gc/arc_heap_tests/generational.rs` | MODIFY | minor 归还整块 chunk；晋升喂老年代预算、major 清零 |
| `src/runtime/src/config_tests.rs` | MODIFY | 新旋钮的解析与默认值 |
| `docs/book/src/runtime/gc-tuning-and-safepoint.md` | MODIFY | 双闸门机制 + 「minor 也要还 chunk」+ 实测表 |
| `docs/spec/changes/add-bounded-nursery/` | NEW | 本变更容器 |

## Out of Scope

- **`Z42_GC_PROMOTION_AGE`**（`PROMOTION_THRESHOLD` 开成构造期旋钮）→ 单独一个 PR，
  它要动 `Region` / `VarRegion` 的构造与约 20 处测试的默认值来源。**User 已裁决走「构造期读取」这条路**
- **`Z42_GC_LOH_BYTES`**（change 2 推过来的）→ 与上面同一个 PR
- **把 chunk 回收做成增量的** → 见 design.md「留给下一步」：它现在是 minor 停顿的大头
  （O(堆) 而不是 O(young)），给 nursery 能买到的停顿下界压了一个地板
- **GC 默认武装 / 默认值怎么定** → change 4 `arm-gc-by-default`

## Open Questions

- [x] major 用什么信号触发 → **User 裁决：晋升字节数**（分配路径零成本）
- [x] `Z42_GC_NURSERY_BYTES` 默认值 → **User 裁决：`MAX_BYTES / 4`**
