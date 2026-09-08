# Tasks: 卡表不变量的两个破口

> 状态：🟢 已完成（2026-09-08）

## 进度概览

| 阶段 | 状态 |
|---|---|
| 1. 定位（探针按 trace_children 走全堆） | ✅ |
| 2. 修破口 2（晋升）+ 破口 3（major 清卡） | ✅ |
| 3. 回归测试（含反证） | ✅ |
| 4. GREEN + 实测 + 文档 + 归档 | ✅ |

## 阶段 1: 定位

- [x] 确认 #537 之后 `generational` + 128M 在 main 上仍然 **6/6 必崩**
- [x] 探针：minor 结束后按 GC 自己的 `trace_children` 走一遍所有活对象/数组，
      检查每个孩子是否已死 → `owner=Z42.IR.StrMap owner_age=2 -> dead Array(age=1)`
- [x] 推导出破口 2（晋升造边）与破口 3（major 清卡）

### 探针的两个坑（下次照抄可省半小时）

- **别在 `trace_children` 的回调里再锁 owner 的 `value`**（为了打印类型名）——
  `trace_children` 自己就持着那把锁，`parking_lot::Mutex` 不可重入 → 死锁，
  表现为「跑了十分钟一行输出都没有」。类型名要在进回调前先取好。
- **别在回调里 `region_var.lock()` 判块是否活着** —— 每个孩子一次加锁，几百万次，
  慢到跑不完。用 `VarGcRef::is_live()`（无锁读块头）。

## 阶段 2: 修

- [x] `sweep_phase_young_only`：`promote` 返回 true 的条目收集起来
- [x] `dirty_cards_for_newly_old_objects` / `..._arrays`：`refers_to_young` 为真才置脏
- [x] `refers_to_young`：复用 `gen_age_of`，遇到第一个年轻孩子短路
- [x] `run_cycle_collection_major`：`clear_card_dirty` → `rebuild_card_table`

## 阶段 3: 测试

- [x] `promoted_owner_keeps_the_young_child_it_was_holding`
- [x] `promoted_array_keeps_the_young_element_it_was_holding`
- [x] `major_rebuilds_cards_for_surviving_cross_gen_edges`
- [x] **反证**：注释掉两个 helper 的调用 → 前两个测试红；未修 major → 第三个测试红

## 阶段 4: GREEN + 实测 + 文档 + 归档

- [x] `./xtask test` 全绿
- [x] 三档预算下 `generational` 编完 `z42c.semantics`（修前 6/6 必崩）
- [x] `docs/book/src/runtime/gc-tuning-and-safepoint.md` 写下三个破口
- [x] **更正 #537 archive 里被 zsh 分词坑污染的实测表**
- [x] 归档

## 交给后续 change 的发现

1. 🔴 **分代模式的 RSS 比完全不回收还高**（1034.6 vs 902.9 MB）：128M 下
   **18 次 minor、0 次 major**，老垃圾一次都没被收，而分代还额外维护三个 region 的
   young 表和卡表。升级启发式（存活率 ≥ `Z42_GC_MINOR_THRESHOLD`）**从没触发过**。
   `add-bounded-nursery` / `arm-gc-by-default` 必须解决「什么时候该 major」。
2. ⚠️ **量 GC 前先确认那一跑真的按你以为的配置在跑**：`Z42_GC_TRACE=1` 数周期数，
   0 周期 = 没武装。**zsh 对未加引号的参数展开不做分词**（和 bash 相反），
   `set -- $cfg` 只会得到一个参数 —— 这个坑污染了 #537 的整张实测表。
3. ⚠️ 规范冲突仍待裁决：`PROMOTION_THRESHOLD` 不入 config（book）vs
   三堆设计要求的 `Z42_GC_PROMOTION_AGE`。

## 验收标准

- 被晋升的条目若仍持有年轻引用，其卡为脏
- major 之后仍存在的 old→young 边重新被卡记录
- 不引用年轻条目的晋升不置脏卡
- 三档预算下 `Z42_GC_MODE=generational` 能编完 `z42c.semantics`
