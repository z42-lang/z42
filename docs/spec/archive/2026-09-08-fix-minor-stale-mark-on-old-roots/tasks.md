# Tasks: minor 的标记不变量

> 状态：🟢 已完成（2026-09-08）

## 进度概览

| 阶段 | 状态 |
|---|---|
| 1. 定位根因 | ✅ |
| 2. 修 `mark_phase_minor` + `reset_all_marks_in_regions` | ✅ |
| 3. 回归测试（含反证） | ✅ |
| 4. GREEN + 实测 + 文档 + 归档 | ✅ |

## 阶段 1: 定位根因

- [x] 在 main（4e0f22f9）上复现 `generational` + 128M 的 `expected string, got Null`
- [x] 读 `mark_phase_minor` / `sweep_phase_young_only`：老条目被置位，只有年轻幸存者被清位
- [x] 用一个探针测试证实：老 owner 在 minor #1 后 `marked=true`，minor #2 后仍 `true`，
      其 child 在 minor #2 被 tombstone
- [x] 弄清旧测试为什么没抓到：只跑一次 minor + child 落在 owner 自己的 chunk 里
      （卡是 chunk 粒度的，child 自己就是根）

## 阶段 2: 修

- [x] `mark_phase_minor`：出队时先判年龄，只给年轻的 `mark_if_unmarked`，老的直接穿透
- [x] `reset_all_marks_in_regions` 补 `region_var`

## 阶段 3: 测试

- [x] `old_root_traces_its_young_children_at_every_minor`
- [x] `minor_leaves_no_mark_on_any_entry`
- [x] `generational_minors_keep_old_to_young_graphs_intact`
- [x] **反证**：临时去掉年龄判断 → 前两个测试红

## 阶段 4: GREEN + 实测 + 文档 + 归档

- [x] `./xtask test` 全绿
- [x] ~~三档预算下 `generational` 编 `z42c.semantics` 全部通过~~ —— ❌ **该结论作废**，
      见 design.md 顶部的更正：那几跑其实未武装。本 change 修掉的是第一层缺陷
      （陈旧 mark 位，已由单测反证），第二层由 `fix-promotion-creates-uncarded-old-to-young` 修
- [x] `docs/book/src/runtime/gc-tuning-and-safepoint.md` 写下不变量与违反后果
- [x] 归档

## 交给后续 change 的发现

1. ❌ ~~**分代模式回收得比 STW 还少**：128M 下分代只跑 3 个周期 / RSS 905 MB，
   STW 跑 10+ 个 / RSS 596 MB；三档 RSS 几乎一样（903–905 MB）~~ ——
   **已作废**：那些 run 因 zsh 不分词而实际是未武装的 STW，见 design.md 顶部的更正。
   同 seed 的诚实数据在 `fix-promotion-creates-uncarded-old-to-young` 的 design.md 里
   （分代 128M：18 次 minor、**0 次 major**、RSS 1034.6 MB，比未武装的 902.9 MB 还高）。
2. ⚠️ **规范冲突待裁决**：`docs/book/src/runtime/gc-tuning-and-safepoint.md` 有一节
   「刻意不做：`PROMOTION_THRESHOLD` 不入 config」，理由是写屏障热路径成本 + 约 20 处测试
   把它当编译期常量；而三堆设计的旋钮总表要求把它开成 `Z42_GC_PROMOTION_AGE`。
   两者直接冲突，`add-bounded-nursery` 开工前需要 User 裁决（书里已给出折中路径：
   做成**构造期**读取、缓存进 heap 字段，而不是 write-barrier 的运行时读）。
3. **CI 从不跑 `generational`**（`grep Z42_GC_MODE` 只有 concurrent 的 smoke）——
   本缺陷能活这么久的直接原因。真要在 change 4 里默认打开，得先补常态覆盖。

## 验收标准

- 一次 minor 结束时堆里没有被置位的 mark
- 只经由老对象可达的年轻对象在任意多次 minor 后仍存活
- 三档预算下 `Z42_GC_MODE=generational` 能编完 `z42c.semantics`
- 非分代模式行为不变
