# Tasks: 默认武装 GC

> 状态：🟢 已完成（2026-09-09）

| 阶段 | 状态 |
|---|---|
| 1. 核实 Mono SGen 的真实常量与公式 | ✅ |
| 2. 触发条件全部改成相对量 | ✅ |
| 3. `next_collect_at`：让默认武装可负担 | ✅ |
| 4. 测试重写 + 实测选默认 + GREEN + 文档 + 归档 | ✅ |

## 阶段 1: 核实 Mono

- [x] `sgen-conf.h`：nursery 4M / allowance 比例 0.33 / 下界 4 个 nursery
- [x] `sgen-memory-governor.c`：`soft_heap_limit` 默认**无上限**；
      `major_collection_trigger_size = new_heap_size + allowance`
- [x] 结论：**「给 MAX_BYTES 挑一个默认值」这个框架是错的**，相对阈值不需要它

## 阶段 2: 相对策略

- [x] `collection_allowance(live, nursery, soft_limit)` —— Mono 的公式
- [x] `decide_trip`：分代（promoted vs allowance → major；grown vs nursery → minor）／
      STW（grown vs allowance → major）
- [x] `MAX_BYTES` 降级成软上限；不设 = 无上限
- [x] nursery 默认改成绝对值，并成为策略的计量单位
- [x] 徒劳退避判据改成「闸门 / 2」

## 阶段 3: `next_collect_at`

- [x] 原子字段 + 分配路径的闸门
- [x] `maybe_auto_collect` 每条出口都重新装填
- [x] `sub_used_bytes` 收尾装填（**必须无锁** —— 三处调用正持着 `inner.lock()`）
- [x] `set_max_heap_bytes` / `set_mode` 后重新装填
- [x] nursery 改成 per-heap 原子（顺带把 `runtime_config()` 查表移出策略路径）

## 阶段 4: 测试 / 实测 / GREEN / 文档

- [x] 五个测试重写（四个的前提被本 change 反转）
- [x] nursery 扫描选默认（32M 是墙钟/RSS 的拐点）
- [x] 同 seed A/B：默认武装 = +2.8% 墙钟 / −26.4% RSS；设了 128M 上限时新策略墙钟还 −8.1%
- [x] `./xtask test` 全绿
- [x] `docs/book/src/runtime/gc-tuning-and-safepoint.md`
- [x] 归档

## 交给后续 change 的发现

1. 🔴 **头号杠杆：把 chunk 回收做成增量的**（只看这一轮 minor 碰过的 chunk）。
   它是 O(堆) 而不是 O(young)，同时压着两件事：minor 的停顿下界，以及 nursery 默认值
   （只能停在 32M，而 Mono 是 4M）。
2. ⚠️ **`Z42_GC_MODE` 默认翻到 generational 要再等一步**。User 已裁决「切，先补 CI」，
   但实测显示今天翻过去墙钟和内存**两头都不占优**（7.34 s / 758.5 MB vs STW 武装的
   7.07 s / 756.2 MB）—— 分代该赢在「minor 便宜所以能勤跑」，而 z42 的 minor 还背着
   O(堆) 的 chunk 回收。正确顺序：**增量 chunk 回收 → 补 CI → 翻默认**。
3. **CI 仍然零分代覆盖** —— #537 / #539 三个丢对象的缺陷能活几个月的直接原因。

## 验收标准

- 不设任何 `Z42_GC_*` 时 GC 会自动回收，且不再增长的堆不回收
- 分配路径上不新增互斥锁
- 设了软上限时行为不差于旧策略
- 三堆路线的四个 change 全部落地
