# Proposal: minor GC 漏扫变长区 —— 分代模式回收不到 45% 的堆

## Why

`run_cycle_collection_minor` 只有三步：retire TLAB、`mark_phase_minor`、`sweep_phase_young_only`
（`gc/arc_heap/generational.rs:193`），后两者**只扫 `region_object` / `region_array`**，
`region_var` 完全不在其中。而变长区（字符串 / 闭包 / 数组元素存储）占 RSS 约 45%
（PR #526 后仍有约 397 MB / 884.8 MB），只在升级到 major 时才被扫
——**分代模式的 minor 回收不到堆的大头**。

更糟的是 `generational.rs:172`：minor tombstone 数组头时把 `array_size_estimate` 计进
`freed_bytes`，而它含 `elem_storage_bytes()`（`arc_heap/alloc.rs:277`）——那些字节住在
`region_var` 里、这一轮根本没被回收。**账上退了、内存没退**，自动回收的预算闸门因此
读到虚高的回收量。这是同一族的第四个计账缺陷（前三个是 #519 / #521 / #522）。

不做的话，`Z42_GC_MODE=generational` 就只是个跑得快但几乎不回收的空转模式，
「分代 GC 默认也关着且在 main 上就是坏的」这条结论会一直成立；后续
`add-bounded-nursery` / `arm-gc-by-default` 都建立在本 change 的正确性之上。

## What Changes

- 给 `GcBlockHeader` 加 `gen_age`，**打包进 `type_tag` 的空闲位**（`BlockType` 只有 5 个变体
  占 3 位，`PROMOTION_THRESHOLD = 2` 只需 2 位）——头必须保持 16 字节，否则 180 万个贴着
  32 字节下限的块会集体跳档，把 #526 刚省的 142 MB 吐回去十几兆
- `VarRegion` 维护 young 块集合 + `iterate_young` / `promote`，与 `Region<T>` 的形状对齐
- `mark_phase_minor` 把变长块纳入追踪（`ArrayValue` / `Closure` 有出边；`Str` / `ArrayPrim` 是叶子）
- `sweep_phase_young_only` 增加变长区的 young 扫描
- `gen_age_of` 改读变长块的真实年龄；修掉由此暴露的**陈旧 mark 位 use-after-free**
  （闭包的 `env` 在第 2 次 minor 被提前释放 —— 见 design.md 决策 3b）
- 修 `freed_bytes` 口径：minor 不再把未回收的 `elem_storage_bytes()` 计入

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/gc/var_region/block.rs` | MODIFY | `gen_age` + `IN_YOUNG_BIT` 打包进 `type_tag`（换 `AtomicU8`）+ 访问器；`BlockType` 编解码收窄到 3 位 |
| `src/runtime/src/gc/var_region.rs` | MODIFY | `young_list` 字段、`iterate_young` / `young_count` / `sweep_young`；`alloc` 两条路径登记 young |
| `src/runtime/src/gc/var_region/chunk.rs` | MODIFY | `fill()` 写 header 时带 `gen_age = 0`；`retire_chunk` 登记 young；`reclaim_dead_var_chunks` purge `young_list` |
| `src/runtime/src/gc/var_region/var_ref.rs` | MODIFY | `alloc_leaked` / `leak_block_for_test` 两处 header 写入点同步带 `gen_age` |
| `src/runtime/src/gc/arc_heap/generational.rs` | MODIFY | `gen_age_of` 读变长块真实年龄（修 UAF）；`sweep_phase_young_only` 调 `region_var.sweep_young()` |
| `src/runtime/src/gc/arc_heap/control.rs` | MODIFY | `young_count` 统计含变长区（升级启发式的分母） |
| `src/runtime/src/gc/arc_heap/interface.rs` | MODIFY | `set_mode` 把 `set_generational` 转发给 `region_var`（**实施期追加**：young 表非分代模式下多吃 20 MB） |
| `src/runtime/src/gc/arc_heap/construct.rs` | MODIFY | 按模式构造 `VarRegion`（**实施期追加**，同上） |
| `src/runtime/src/metadata/vstr.rs` | MODIFY | `Str::gen_age()` 转发到块头（**实施期追加进 Scope**：`Value::Str` 持的是 `vstr::Str` 而非 `VarGcRef`，`gen_age_of` 需要这个转发） |
| `src/runtime/src/gc/mode.rs` | MODIFY | `GenerationalMarkSweep` 的契约注释更新（三个 region 都参与、变长块不需要卡表） |
| `src/runtime/src/gc/var_region_tests.rs` | MODIFY | 变长块年龄 / 晋升 / young 扫描 / 卡表的单测 |
| `src/runtime/src/gc/arc_heap_tests/generational.rs` | MODIFY | minor 回收变长区的端到端断言 + `freed_bytes` 口径断言 |
| `docs/book/src/runtime/gc-tlab-chunk-exclusive.md` | MODIFY | 变长区分代的机制说明（位布局 + young 表的重建式维护 + 陈旧 mark 的坑） |
| `docs/spec/changes/fix-minor-gc-skips-var-region/` | NEW | 本变更容器（proposal / design / specs / tasks） |

**只读引用**（理解上下文必须读，但不修改）：

- `src/runtime/src/gc/region.rs` — 对齐 `Region<T>` 的 young / promote / 卡表形状
- `src/runtime/src/gc/arc_heap/alloc.rs` — `array_size_estimate` / `alloc_charge_bytes` 语义
- `src/runtime/src/gc/arc_heap/auto_collect.rs` — 预算闸门如何消费 `freed_bytes`
- `src/runtime/src/gc/mode.rs` — `GenerationalMarkSweep` 的既有契约描述

## Out of Scope

- **有界 nursery 容量闸门**（`Z42_GC_NURSERY_BYTES` + 按容量触发）→ 另开 `add-bounded-nursery`
- **大对象堆死后不归还**（D3）→ 另开 `fix-loh-never-freed`
- **GC 默认武装 / 默认值怎么定** → 另开 `arm-gc-by-default`
- **chunk 级分代 / 删 `young_list`** → 前置实验证明拿不到 RSS（object 区存活率 ≈ 0.2，
  `0.8^256 ≈ 0`），不做
- 任何需要移动对象的方案（压缩 / 疏散 / Immix）——被 `GcRef` 地址即身份 + native pin 挡着

## Open Questions

- [x] 变长块的卡表放哪 → **实施期推翻：不需要卡表**。变长块不产生跨代写（`Closure` 创建后不可变、
      数组元素只经 `Value::Array` owner 写、其余是叶子），且 `mark_backing()` 早已覆盖标记。
      详见 design.md 决策 3；原阶段 3 取消
- [x] `PROMOTION_THRESHOLD` 是否给变长块独立阈值 → **决策 D-4：先统一**，无数据支持拆分；
      注意 2 位 `gen_age` 最大只能表达到 3
