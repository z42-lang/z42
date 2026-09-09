# Design: 相对量的回收策略

## Architecture

```
   分配 ──► used ≥ next_collect_at ？ ──否──► 什么都不做（一次 relaxed load）
                    │是
                    ▼
              decide_trip()
                    │
    ┌───────────────┴────────────────┐
    │ 分代                            │ STW（只有一代）
    │ promoted ≥ allowance → major    │ used - live ≥ allowance → major
    │ 否则 grown ≥ nursery → minor    │
    └────────────────────────────────┘

   allowance(live) = MAX(live × 0.33, nursery × 4)，再被软上限压一道
```

**每一个阈值都是相对的**，所以没有任何一处需要「一个字节预算先存在」——
这就是默认武装的前提。`Z42_GC_MAX_BYTES` 从「武装开关」降级成「软上限」，
不设 = 无上限（和 Mono 的 `soft_heap_limit` 一样）。

## Decisions

### Decision 1: 照抄 Mono SGen 的 allowance 规则

`allowance = MAX(live × ALLOWANCE_HEAP_RATIO, nursery × ALLOWANCE_NURSERY_RATIO)`
（0.33 / 4.0，两个都是 Mono 的原值）。含义：**让老年代吃进上次全量回收后活集的三分之一，
再扫一次**；下界是几个 nursery，否则一个活集极小的程序会不停回收
（「几乎没有」的三分之一还是几乎没有）。

软上限从另一侧压：一旦 `live + allowance` 会越过上限，allowance 收缩成剩下的余量
（仍以下界兜底 —— 活集已经超过上限的堆退化成「每个下界收一次」，而不是「每次分配收一次」）。

**为什么不是「按机器内存比例定一个默认预算」**：那是本 change 原本的方案，被 Mono 的形状
证明是多余的。相对阈值自适应 —— 10 MB 的脚本和 4 GB 的服务用同一套参数，
而按机器内存的启发式在容器里还会读错。

### Decision 2: `next_collect_at` —— 默认武装的可负担性全在这里

`maybe_auto_collect` 要读 `inner` 里的水位线，也就是**要拿堆的互斥锁**。默认武装
= 每次分配都拿一次锁，那是 VM 里最热的路径。在这之前，唯一挡住它的就是
「没设预算 ⇒ 永不回收」那条早退。

改法是 Mono 的 `major_collection_trigger_size`：把**下一次该被询问的 `used` 读数**
缓存进一个原子。分配路径变成**一次 relaxed load + 一次比较**，慢路径每个闸门至多进一次。

维护点两处，缺一不可：
- `maybe_auto_collect` 的**每一条出口**（包括「这次不收」和「已经有一次待决」）——
  否则下一次分配又走进来拿锁；
- `sub_used_bytes` —— 四条回收路径都在自己的 stats 块里调它来记回收量，
  是「一个周期刚结束」唯一的公共汇合点。
  ⚠️ **它必须无锁**：其中三处调用时**正持着 `inner.lock()`**，而 `parking_lot::Mutex`
  不可重入。所以 `rearm_auto_collect` 只读原子，退避倍数（住在 `inner` 里）不参与 ——
  慢路径下次运行时会带上它重算，最多晚一个闸门。

触发那一刻把 `next_collect_at` 置成 `u64::MAX`：回收落地前不该再被问，而回收之后的
`rearm` 会从**新的活集**算出正确的下一个闸门（用触发前的读数算会偏高一整个闸门）。

### Decision 3: nursery 默认 32M（Mono 是 4M）

nursery 是整套策略的计量单位：minor 的闸门，以及 major allowance 的下界（×4）。
Mono 取 4 MB，因为**它的 minor 是 O(young) 的**；z42 的 minor 还要做一遍
**O(堆)** 的 chunk 回收（`add-bounded-nursery` 留下的头号杠杆），所以频繁 minor 在这里贵得多。

实测（`z42c.semantics --release --no-incremental`，**不设任何预算**）：

| nursery | 分代 minors/majors | 墙钟 | RSS | STW majors | 墙钟 | RSS |
|---|---|---|---|---|---|---|
| 8M | — | — | — | 10 | 6.87 s | 753 MB |
| 16M | 25/2 | 7.65 s | 583 MB | 6 | 6.73 s | 785 MB |
| 24M | 15/1 | 7.19 s | 653 MB | — | — | — |
| **32M** | **10/1** | **6.94 s** | **775 MB** | **4** | **6.67 s** | **743 MB** |
| 48M | 5/1 | 6.67 s | 858 MB | — | — | — |

（这张扫描表是在选默认值那一轮的 seed 上量的，绝对值与下面的总表不可跨 seed 比，
形状才是结论：**32M 是墙钟与 RSS 的拐点**。）等 chunk 回收做成增量的，这个值应该往
Mono 的 4M 靠。

### Decision 4: 徒劳退避的判据改成「不到半个闸门」

原判据是 `throttle_ratio × limit` —— 又一个依赖预算的量。改成 `闸门 / 2`：
一次健康的 minor 回收掉一个 nursery 的**大部分**（不是全部），拿整个闸门当及格线会把
每一次健康回收都判成徒劳（实测：minor 从 18 次掉到 8 次，堆干脆不收了）。

顺带一提：相对闸门本身就大幅削弱了徒劳的病理 —— 闸门随活集一起长，回收次数对堆增长是
**对数**而不是线性的。退避现在是软上限把 allowance 压到下界那种情况的保险。

### Decision 5（⚠️ 与既有裁决的偏差）: 这次**不**翻 `Z42_GC_MODE` 的默认

User 已裁决「切 generational，但先补 CI 覆盖」。实测说明还应再往后排一步：

| 无预算 | 周期 | 墙钟 | RSS |
|---|---|---|---|
| **新默认（STW，武装）** | 4 major | **7.07 s** | **756.2 MB** |
| 新默认 + generational | 10 minor / 1 major | 7.34 s | 758.5 MB |

**今天翻过去，墙钟和内存两头都更差。** 分代该赢的地方是「minor 便宜所以可以勤跑」，
而 z42 的 minor 还背着 **O(堆)** 的 chunk 回收 —— 这正是它赢不了的原因，
也是 nursery 只能停在 32M 的原因。**正确顺序是：先把 chunk 回收做成增量的，再补 CI，
再翻默认。** 裁决没有作废，只是前面多了一个前置。

## Testing Strategy

四个既有测试的**前提被本 change 反转**，全部重写：

- `no_budget_means_no_automatic_collection` → `no_budget_still_collects`
- 新增 `a_heap_that_is_not_growing_does_not_collect` —— 相对闸门的另一半：
  不长的堆不该收，这是「默认武装」不等于「一直在收」的保证
- `the_nursery_gate_is_generational_only` → `generational_collects_more_often_than_stw_on_the_same_workload`
  （两种模式的闸门大小不同：一个 nursery vs 一个 allowance = 4 个 nursery）
- `futile_collections_back_off...` → `an_over_budget_live_set_does_not_re_collect_forever`
  （旧写法靠「反复摘掉预算」来避开内联回收吃掉未 pin 的新对象；预算不再是开关之后
  这招失效，改成反复放大/缩小 nursery）
- `auto_collect_throttled_by_growth_delta` 改用显式 nursery 控制算术

**测试要能控制 nursery**，所以它成了 per-heap 的原子 + `set_nursery_bytes_for_test`
（顺带把 `runtime_config()` 查表从策略路径上拿掉了）。

## 实测总表（同一 seed、同一次会话、各 3 跑取中位）

`z42c.semantics --release --no-incremental`，基线 = 本 change 的父提交（#542 合入 main 之后）：

| 配置 | 周期 | 墙钟 | 峰值 RSS |
|---|---|---|---|
| **旧默认（未武装）** | 0 | 6.88 s | 1027.4 MB |
| **新默认（无任何 env）** | 4 major | **7.07 s (+2.8%)** | **756.2 MB (−26.4%)** |
| 旧策略 + `MAX_BYTES=128M` | 14 major | 7.90 s | 614.7 MB |
| **新策略 + `MAX_BYTES=128M`** | 12 major | **7.26 s (−8.1%)** | **593.7 MB (−3.4%)** |
| 新默认 + generational | 10 minor / 1 major | 7.34 s | 758.5 MB |

两条结论：

1. **默认武装用 +2.8% 的墙钟换 −26.4% 的 RSS** —— 而在这之前，不设 `Z42_GC_MAX_BYTES`
   的程序**一次都不回收**。
2. **设了软上限时相对策略也更好**（墙钟 −8.1%、RSS −3.4%）：allowance 随活集自适应，
   不像固定的 `throttle_ratio × limit` 那样在活集变大后仍按同一格触发。
