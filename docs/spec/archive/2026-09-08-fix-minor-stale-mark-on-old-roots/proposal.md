# Proposal: minor 给老对象留下的 mark 位，让第二次 minor 起就开始丢对象

## Why

`Z42_GC_MODE=generational` 在 main 上是坏的：编 `z42c.semantics` 挂在

```
Std.Exception: __str_hash_code: arg 0 expected string, got Null
  at Z42.IR.StrMap.ContainsKey  ← Z42.Semantics.PureTable.IsPure  ← CSE
```

—— 一个仍被引用的 `Str` 被提前回收了。复现条件是**minor 次数**，不是预算大小：

| 预算 | minor 次数 | 结果 |
|---|---|---|
| 未武装 | 0 | ✓ |
| 256M | 2 | ✓（跑不到第二次 minor 之后） |
| 128M | 4+ | ✗ 必炸 |

**根因**：`mark_phase_minor` 对**每一个**出队的值调 `mark_if_unmarked`，包括老对象。
而 `sweep_phase_young_only` 只给**年轻**幸存者清 mark 位，老对象的位要等到下一次 major
（`reset_all_marks_in_regions`）才清。于是：

```
minor N    : 老 owner 作为脏卡根出队 → 置位 → 追踪孩子 ✓
minor N+1  : 同一个脏卡再入队 → mark_if_unmarked 撞见旧位 → false → `continue`
             → 它的孩子一个都没被追踪 → 只经由它可达的年轻对象全部被扫掉
```

脏卡的根**天生就是老的**（写屏障只在 owner 老、值年轻时置脏），固定根里也满是老对象，
所以第二次 minor 起，「老对象 → 年轻对象」这条边就整体失效了。

这是这套 GC 里同一族缺陷的**第三次**：#533 修的是闭包因为陈旧 mark 位导致 `env` 被提前释放
（那一次是 `gen_age_of` 谎报年龄导致老闭包被反复入队）。**「mark 位活过了它那一轮回收」
是这套代码的惯犯。**

不修的话，三堆路线的 change 3（`add-bounded-nursery`）和 change 4（`arm-gc-by-default`）
都建立在一个会丢对象的模式上 —— 而 bounded nursery 恰恰会让 minor 更频繁，也就更容易炸。

**为什么没被测到**：`cross_gen_write_target_survives_minor_via_dirty_card` 两处漏网 ——
它只跑**一次** minor，且它的 child 落在 owner 自己的 chunk 里（卡是 chunk 粒度的，
child 因此自己就是脏卡根，根本不需要「被 owner 追踪到」）。CI 也从不跑这个模式
（`grep Z42_GC_MODE` 只有 concurrent 的 smoke）。

## What Changes

- `mark_phase_minor`：**只标记年轻的**。老对象出队时不置 mark，直接 `trace_children`
  穿透过去。终止性靠「老的**孩子**从来不入队」这条既有规则（`gen_age < threshold` 过滤）
- `reset_all_marks_in_regions`：补上漏掉的 `region_var`。它和另外两个 region 一样带 mark 位
  （`mark_backing` / `shade_var_newborn` 置位），少这一行意味着一次中途放弃 / 换模式的回收
  留下的位会让下一次 `mark_phase` 跳过某个闭包的 `env`
- 三个回归测试，都验证过「不打补丁就红」

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/gc/arc_heap/generational.rs` | MODIFY | `mark_phase_minor` 只标记年轻的，老对象穿透追踪 |
| `src/runtime/src/gc/arc_heap/collect.rs` | MODIFY | `reset_all_marks_in_regions` 补上 `region_var` |
| `src/runtime/src/gc/arc_heap_tests/generational.rs` | MODIFY | 三个回归测试（老根跨多次 minor 追踪 / minor 不留 mark / 老→年轻图的压力） |
| `docs/book/src/runtime/gc-tuning-and-safepoint.md` | MODIFY | 「minor 不给老对象留标记」不变量 + 违反后果 |
| `docs/spec/changes/fix-minor-stale-mark-on-old-roots/` | NEW | 本变更容器 |

**只读引用**：

- `src/runtime/src/gc/region/generation.rs` — `iterate_dirty_cards` 是 **chunk 粒度**的（一个脏 chunk 里所有活条目都是根），这正是旧测试没抓到 bug 的原因
- `src/runtime/src/gc/arc_heap/generational.rs::maybe_mark_cross_gen_card` — 写屏障只在 owner 老且值年轻时置脏
- `src/runtime/src/gc/arc_heap/control.rs` — `force_collect` 在分代模式下走的是 minor

## Out of Scope

- **分代模式回收得比 STW 还少**（实测 128M：分代 3 个周期 / RSS 905 MB，STW 10 个周期 / RSS 596 MB）
  —— 这是本 change 之后暴露出来的**下一个**问题，归 `add-bounded-nursery` / `arm-gc-by-default`
- **有界 nursery / `Z42_GC_NURSERY_BYTES` / `Z42_GC_PROMOTION_AGE` / `Z42_GC_LOH_BYTES`** → change 3
- **给 CI 加分代模式的常态覆盖** → 归 change 4（真要默认打开时才必须有）

## Open Questions

- [ ] 无。根因、修法、反证测试都已闭环。
