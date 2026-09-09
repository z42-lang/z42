# Tasks: 有界新生代

> 状态：🟢 已完成（2026-09-08）

## 进度概览

| 阶段 | 状态 |
|---|---|
| 1. 量清楚「分代为什么比不回收还差」 | ✅ |
| 2. minor 尾部 chunk 回收 | ✅ |
| 3. 双闸门（nursery / 晋升字节）+ 两处口径修正 | ✅ |
| 4. 测试 + GREEN + 实测 + 文档 + 归档 | ✅ |

## 阶段 1: 量清楚

- [x] 确认 `used_bytes`（活字节）≈ 90 MB 而 RSS ≈ 1 GB —— 计账没漂，是**口径**不对
- [x] 实验：强制每 N 次 minor 补一次 major → RSS 1031 / 710 / 671 / 616 MB（N = ∞/4/2/1）
- [x] **推翻「多跑 major 就行」**：加上 minor 尾部 chunk 回收后，同样的 15 minor / 1 major
      就把 RSS 从 986.3 压到 607.9 MB
- [x] 发现升级启发式把「晋升」当「死亡」→ 升级从没触发过
- [x] 发现徒劳退避的判据被新闸门带偏（minor 18 → 8 次）

## 阶段 2: minor 尾部 chunk 回收

- [x] `sweep_phase_young_only` 末尾调三个 region 的 `reclaim_dead_chunks` / `reclaim_dead_var_chunks`

## 阶段 3: 双闸门

- [x] `promoted_bytes_since_major` 原子（minor sweep 累加、major 清零）
- [x] `pending_major` 标志（策略层 → 延迟执行层）
- [x] `Z42_GC_NURSERY_BYTES` 旋钮（config / parse / knob_table），默认 `MAX_BYTES / 4`
- [x] `maybe_auto_collect` 分代臂：nursery 增长闸门 + 晋升字节闸门
- [x] 存活率改成 `1 - reclaimed / young_before`
- [x] 徒劳判据固定对着 `throttle_ratio × limit`

## 阶段 4: 测试 / GREEN / 文档

- [x] 六个新测试（两个 auto-collect 闸门、两个分代行为、两个 config）
- [x] `./xtask test` 全绿
- [x] 实测表（含 nursery 扫描）见 design.md
- [x] `docs/book/src/runtime/gc-tuning-and-safepoint.md`
- [x] 归档

## 交给后续 change 的发现

1. 🔴 **chunk 回收是 minor 停顿的地板**：它是 O(堆) 而不是 O(young)，中位停顿因此只从
   67.6 走到 90.3 ms（nursery 8M → 64M），远不如最大停顿敏感。设计文档「p99 随 nursery
   容量线性变化」要完全成立，得先把 `reclaim_dead_chunks` 做成增量的
   （只看这一轮 minor 碰过的 chunk）。
2. **`Z42_GC_PROMOTION_AGE` + `Z42_GC_LOH_BYTES` 另开一个 PR** —— User 已裁决
   `PROMOTION_THRESHOLD` 走**构造期读取**（per-heap 字段，不是写屏障热路径的全局原子），
   书里「刻意不做」那一节据此更新。
3. **change 4 `arm-gc-by-default` 现在有数据了**：分代 128M 与 STW 的 RSS 打平、
   墙钟略快、最大停顿 −26%；256M 下 RSS −4.6%。默认值可以按这张表定。

## 验收标准

- 分代模式下 `used` 远低于预算时，nursery 闸门仍会触发 minor
- STW 模式行为不变
- minor 之后整块死亡的 chunk 回到池子
- 晋升字节累加、major 清零
- 分代模式的 RSS 不再高于未武装
