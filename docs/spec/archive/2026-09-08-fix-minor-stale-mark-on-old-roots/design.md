# Design: minor 的标记不变量

## Architecture

`mark_phase_minor` 的 BFS 有三类种子：固定根（`inner.roots`）、external scanner
（静态字段 / 调用栈 / func-ref 槽）、**脏卡里的每一条活条目**。写屏障
（`maybe_mark_cross_gen_card`）只在「owner 是老 **且** 被写的值是年轻」时置脏，
所以**脏卡的根天生就是老对象**；固定根里也满是老对象。

BFS 的另一条既有规则是：**老的孩子从不入队**（`gen_age_of(child) < threshold` 才 push）。
理由是老→年轻的边已由脏卡单独做根覆盖。这条规则是本 change 的终止性依据。

改动只有一处：出队时先看年龄。

```
while let Some(v) = queue.pop() {
    if gen_age_of(v) < threshold {        // ← 新增的条件
        if !mark_if_unmarked(v) { continue }
        marked += 1
    }
    // 老的：不 mark，直接穿透
    v.trace_children(|child| if gen_age_of(child) < threshold { queue.push(child) })
}
```

## Decisions

### Decision 1: 老对象「不标记 + 穿透」，而不是「标记后再清位」

**选项 A**：minor 结束时把这一轮标记过的老对象的位清掉（需要记一份名单）。
**选项 B**：minor 根本不给老对象置位。

**选 B。** mark 位的语义是「本轮回收判定它存活」，而 minor **从不清扫老对象** ——
给它置位从一开始就是没有含义的写。B 顺带省掉每个老根一次 CAS，并把不变量收紧成一句
可断言的话：**一次 minor 结束时堆里没有任何被置位的 mark**（`minor_leaves_no_mark_on_any_entry`
就是在断言这句）。A 要维护一份名单，还得保证异常路径也清 —— 又是一条「谁来清位」的新债。

### Decision 2: 终止性与重复代价

不 mark 老对象就等于放弃了「老根的去重」。可接受，因为：

- 老的**孩子**从不入队，所以老对象只可能来自有限的种子集合（根 + 脏卡）；
- 同一个老对象在种子里出现两次（比如既是固定根又在脏卡里）的代价是
  **O(它的字段数)**，不是 O(它的子图) —— 递归只经由年轻对象展开，而年轻对象仍由
  mark CAS 去重，每个至多追踪一次。

### Decision 3: 顺带补 `reset_all_marks_in_regions` 的变长区

`run_cycle_collection_stw` 开头的防御性清位只覆盖 `region_object` / `region_array`，
漏了 `region_var` —— 而变长块一样带 mark 位（`mark_backing` / `shade_var_newborn` 置位，
`sweep` 在幸存者上清位）。少这一行，一次中途放弃或换模式的回收留下的位就会让下一次
`mark_phase` 在某个闭包上 `mark()` 失败、跳过它的 `env` —— **正是 #533 从另一头修的那个
use-after-free**。major 一次构建里也就个位数次，多一遍 `iterate_alive` 是防御性清位里便宜的那半。

不把它算进 Out of Scope，是因为它和主修是同一条不变量的两面：**mark 位不许活过它那一轮回收**。

## Implementation Notes

- 判据用的是 `Self::gen_age_of(&v)`，和过滤孩子用的是同一个函数 —— 保证「入队时算年轻」
  和「出队时算年轻」不会分叉。
- 非堆值（`Value::I64` / `Value::Ref` 等）`gen_age_of` 返回 0，走年轻臂，
  `mark_if_unmarked` 对它们返回 `false` → `continue`。行为与改前一致。

## Testing Strategy

三个测试，**全部验证过「不打补丁就红」**：

1. `old_root_traces_its_young_children_at_every_minor` —— 老 owner 持有一个年轻 child，
   连跑 3 次 minor。**关键的两个 setup 细节**（旧测试就栽在这两点上）：
   跑**多于一次** minor；用 600 个 pinned filler 把 child 顶出 owner 的 chunk
   —— 卡是 chunk 粒度的（`iterate_dirty_cards` 把脏 chunk 里每条活条目都当根），
   同 chunk 的 child 自己就是根，根本不需要被 owner 追踪到。
2. `minor_leaves_no_mark_on_any_entry` —— 直接断言不变量本身。
3. `generational_minors_keep_old_to_young_graphs_intact` —— 8 个老 owner，每轮换一个
   **新的**年轻 child，跑 6 轮。形状对应真实负载（老的 `StrMap` 桶数组持有年轻 `Str`）。

反证（临时去掉年龄判断后）：测试 1、2 均红。

## ⚠️ 更正（2026-09-08，由 `fix-promotion-creates-uncarded-old-to-young` 补上）

**下面这张实测表和「本 change 之后暴露的下一个问题」那一节都是错的**，两处都源于同一个
zsh 坑：那些 run 写成

```zsh
for cfg in "generational 128M"; do set -- $cfg; env Z42_GC_MODE=$1 Z42_GC_MAX_BYTES=$2 … ; done
```

而 **zsh 对未加引号的参数展开不做分词**（和 bash 相反）——`$1` 是整串
`"generational 128M"`、`$2` 是空。于是 `Z42_GC_MODE` 拿到非法值被忽略、
`Z42_GC_MAX_BYTES` 空 = **未武装**。所谓「三档 RSS 几乎一样」正是因为它们本来就是
同一次未武装的 STW 跑。

因此：

- ❌ **「三档预算下 generational 都能编完」不成立** —— 本 change 只修掉了一层缺陷
  （陈旧 mark 位，真实且已由单测反证）；分代模式当时**仍然 6/6 必崩**，
  第二层缺陷（晋升造出没有卡的 old→young 边）由
  `fix-promotion-creates-uncarded-old-to-young` 修掉。
- ❌ **「分代收得比 STW 还少：905 MB vs 596 MB」这个对比无效**（跨配置且跨 seed）。
  同 seed、逐条显式 `env` 的诚实数据见那个 change 的 design.md。

**教训**：量 GC 前先确认那一跑真的按你以为的配置在跑 —— `Z42_GC_TRACE=1` 数周期数，
0 周期就是没武装。

## （已作废）原实测

`z42c.semantics --release --no-incremental`：

| 配置 | 修前 | 修后 |
|---|---|---|
| `generational` + 64M | ✗ 崩 | ✓ 6.61 s / 903.3 MB |
| `generational` + 128M | ✗ 崩（`expected string, got Null`） | ✓ 6.31 s / 905.3 MB |
| `generational` + 256M | ✓（只跑到 2 次 minor，侥幸） | ✓ 6.45 s / 905.2 MB |

**非分代模式一行都不受影响** —— 改的两处一处只在 minor 里跑（minor 只有分代模式有），
另一处是 major 开头多清一个 region 的位。

## ⚠️ 本 change 之后暴露的下一个问题（归 change 3/4）

分代模式现在**跑得通，但回收得比 STW 还少**：

| 模式 | 预算 | 回收周期数 | 峰值 RSS |
|---|---|---|---|
| STW | 128M | 10+ | 596 MB |
| generational | 128M | **3** | **905 MB** |

而且 64M / 128M / 256M 三档的 RSS 几乎一样（903–905 MB）—— 预算旋钮基本失效。
minor 只清年轻代，老垃圾要等 major，而升级启发式（存活率 ≥ `MINOR_THRESHOLD`）
在这个负载上几乎不触发。这正是 `add-bounded-nursery` + `arm-gc-by-default` 要解决的事。
