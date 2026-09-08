# Proposal: 晋升与 major 清卡都会造出「没有卡的 old→young 边」

## Why

`Z42_GC_MODE=generational` 编 `z42c.semantics` **仍然**挂在
`__str_hash_code: arg 0 expected string, got Null` —— #537 修掉的陈旧 mark 位只是其中一层。
在 main（6e671bfd，#537 已合）上 **6/6 必现**。

探针（minor 结束后按 GC 自己的 `trace_children` 走一遍全部活对象，检查每个孩子是否已死）
第一次就抓到形状：

```
DANGLING after minor: from_objects=51 from_arrays=33
first= owner=Z42.IR.StrMap owner_age=2 -> dead Array(age=1)
```

一个**老**的 `StrMap`（age 2）持有它后来才扩容出来的**年轻**桶数组（age 1），
数组被 minor 扫掉了，而 map 还活着。

**根因：卡表的不变量有两个缺口。**

写屏障记录的是**写的那一刻**的 old→young。但有两条路径能在**没有任何写**的情况下
造出 old→young 边：

1. **晋升**（本次抓到的那条）：父对象比子对象先分配，于是先老。它跨过
   `PROMOTION_THRESHOLD` 的那一瞬间，就成了「老对象持有年轻对象」——
   而当初那次写是 young→young，屏障正确地什么都没做。
2. **major 清卡**：`run_cycle_collection_major` 扫完全堆后把所有卡**清空**。
   可 major **不做晋升**（只 mark + sweep），所以之后年轻对象仍然年轻、老对象仍然指着它们
   —— 卡却没了。下一次 minor 再也 re-root 不到这些 owner，孩子当场被扫。

两条都是同一句不变量的破口：**「一个老条目只要还指着任何年轻的东西，它的卡就必须是脏的」**。

不修的话三堆路线的 change 3 / 4 全部建立在一个会丢对象的模式上。

## What Changes

- **晋升处补屏障**：`sweep_phase_young_only` 里，凡是本轮真正跨过阈值变老的条目，
  检查它是否还指着年轻的东西；是则把它的卡置脏 —— 正是那次写如果发生在晋升之后
  屏障会做的事。只检查真正跨阈值的条目、每个一次；只有确实指着年轻对象的才置脏
  （无条件置脏会把下一次 minor 变成全堆扫描 —— 卡是 chunk 粒度、256 条一格）
- **major 处改「清空」为「重建」**：`rebuild_card_table()` 清完卡后，扫一遍存活的老条目，
  把仍指着年轻对象的重新置脏。一次 major 一遍，换 10–20 次 minor 拿到最小脏集
- 四个回归测试（两个晋升 + 一个 major 重建 + 既有的 minor 不留 mark），均验证过「不打补丁就红」

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/gc/arc_heap/generational.rs` | MODIFY | 晋升后置脏卡（对象区 / 数组区各一个 helper）+ `refers_to_young` + `rebuild_card_table` 换掉 major 的 `clear_card_dirty` |
| `src/runtime/src/gc/arc_heap_tests/generational.rs` | MODIFY | 三个回归测试 + `pin_filler` 辅助 |
| `docs/book/src/runtime/gc-tuning-and-safepoint.md` | MODIFY | 卡表不变量的三个破口（写 / 晋升 / 清卡）与各自的补法 |
| `docs/spec/archive/2026-09-08-fix-minor-stale-mark-on-old-roots/` | MODIFY | **更正**其中被 zsh 分词坑污染的实测表（见下） |
| `docs/spec/changes/fix-promotion-creates-uncarded-old-to-young/` | NEW | 本变更容器 |

## ⚠️ 必须更正的前一条记录

#537 的 design.md 里那张「三档预算下 generational 都能编完」的表，以及「分代收得比 STW 还少
（905 MB vs 596 MB）」的对比，**都是错的**：那些 run 是用
`for cfg in "generational 128M"; do set -- $cfg; env Z42_GC_MODE=$1 Z42_GC_MAX_BYTES=$2 …` 跑的，
而 **zsh 对未加引号的参数展开不做分词**，于是 `$1` 是整串 `"generational 128M"`、`$2` 是空 ——
`Z42_GC_MODE` 拿到非法值被忽略、`Z42_GC_MAX_BYTES` 空 = 未武装。三档「一模一样」是因为
它们本来就是同一次未武装的 STW 跑。本 change 用逐条显式 `env` 重测。

## Out of Scope

- **分代模式的 RSS 比不回收还高** —— 现在有了同 seed 的诚实数据（见 design.md）：
  128M 下 18 次 minor、**0 次 major**，老垃圾一次都没被收。归 `add-bounded-nursery` /
  `arm-gc-by-default`
- **有界 nursery / `Z42_GC_NURSERY_BYTES` / `PROMOTION_AGE` / `LOH_BYTES`** → change 3
- **变长区（Closure 块）的同类缺口** → 见 design.md 决策 3，当前论证为不可达，留了测试盯着

## Open Questions

- [ ] 无。两个缺口都有反证测试。
