# Design: 卡表不变量的三个破口

## Architecture

分代 minor 的正确性完全压在一句话上：

> **一个存活的老条目，只要还指着任何年轻的东西，它的卡就必须是脏的。**

因为 minor 的标记只从三种种子出发（固定根 / external scanner / 脏卡里的活条目），
而**老的孩子从不入队**（`gen_age < threshold` 才 push）。老对象若既不是根、卡又是干净的，
它的年轻孩子就没有任何人去标记。

这句不变量有**三个**可能被破坏的时刻，本 change 之前只堵了第一个：

| # | 时刻 | 谁负责 | 状态 |
|---|---|---|---|
| 1 | 往老对象里**写**一个年轻引用 | 写屏障 `maybe_mark_cross_gen_card` | ✅ 一直都有 |
| 2 | 一个持有年轻引用的对象**被晋升成老的** | —— | ❌ 无人负责（本 change） |
| 3 | major **清空**卡表，而 old→young 边还在 | —— | ❌ 无人负责（本 change） |

### 破口 2：晋升

父对象比子对象先分配 → 先老。它跨过 `PROMOTION_THRESHOLD` 的那一瞬间就成了
「老对象持有年轻对象」，而当初那次写是 young→young，屏障正确地什么都没做。

实测形状：老的 `Z42.IR.StrMap`（age 2）持有它后来扩容出来的桶数组（age 1）。
map 是图中间的节点、不是固定根，于是下一次 minor 谁也 root 不到它，数组当场被扫，
`ContainsKey` 读到 `Null`。

> 注意：**固定根上的老对象不会踩这个坑** —— #537 之后 minor 会「穿透」老根去追踪它的孩子。
> 只有图中间的老对象（既不是根，又只靠卡）才会。这也是为什么回归测试必须把 owner
> 藏在一个 root 后面，直接 pin owner 的测试是空转的。

### 破口 3：major 清卡

`run_cycle_collection_major` 扫完全堆后把卡全清，注释说「cross-gen references are now fully
traced; cards can reset」。但 **major 不做晋升**（`sweep_phase` 只 mark + sweep，不碰 `gen_age`），
所以扫完之后年轻对象仍然年轻、老对象仍然指着它们 —— 卡却没了。

这条在本仓的 z42c 负载上暂时打不着（0 次 major，见下），但一旦 change 3/4 让 major 真的跑起来，
它就是同一个崩溃的另一条路径。单测 `major_rebuilds_cards_for_surviving_cross_gen_edges`
直接调 `run_cycle_collection_major` 复现。

## Decisions

### Decision 1: 晋升处「检查后置脏」，不无条件置脏

**选项 A**：任何条目一晋升就把它的卡置脏。
**选项 B**：只有确实还指着年轻对象的才置脏（一次 `trace_children`）。

**选 B。** 卡是 **chunk 粒度**的（256 条一格，`iterate_dirty_cards` 把脏 chunk 里每条活条目
都当根）。一次 minor 里晋升的条目成千上万、分散在几乎所有 chunk 上，A 等于把下一次 minor
变成全堆扫描 —— 那就没有分代了。B 的代价是每个**真正跨阈值**的条目一次 `trace_children`
（O(字段数)，且遇到第一个年轻孩子就短路），只在晋升的那一轮付一次。

### Decision 2: major 处「重建」而不是「保留」

**选项 A**：major 干脆不清卡。
**选项 B**：清完之后按存活的图重建。

**选 B。** A 是正确的，但脏集只增不减，minor 会一路退化成全堆扫描 —— 清卡这件事本身
是有意义的，错的是「清完就不管了」。B 的代价是一次 major 一遍 `iterate_alive` +
每个老条目一次 `trace_children`；major 在一次编译里是个位数次，换来的是之后 10–20 次 minor
拿到最小脏集。

### Decision 3: 变长区（Closure）不需要同样的处理 —— 论证而非遗漏

变长块里唯一有出边、且**自身的 `gen_age` 参与追踪判定**的是 `Closure`
（`gen_age_of(Value::Closure)` 读的是闭包块自己的年龄）。它需要同样的保护吗？**不需要**：

- `ClosureData` **创建后不可变**（`value_aux.rs` 明写，全仓没有任何 `env` 的写点）；
- `env` 数组必须在闭包块写入之前就存在 —— 所以**闭包块永远不会比它的 `env` 老**。
  「老父亲 → 年轻儿子」这个方向在这里构造不出来。

其余变长块：`Str` / `ArrayPrim` 是叶子；`ArrayValue` / `ArrayStruct` 是数组的元素存储，
追踪它们走的是 `Value::Array`（数组**头**在 `region_array`），判年龄用的是头的年龄，
块自身的年龄根本不参与 —— 头由本 change 的数组区 helper 覆盖。

既有的 `closure_env_survives_repeated_minors`（#533 留下的）继续盯着这条论证。

## Implementation Notes

- `refers_to_young` 复用 `gen_age_of`，和 minor 标记里过滤孩子用的是同一个判据 ——
  保证「谁算年轻」不会在两处分叉。
- 两个 helper 都是**先收集、再置脏**：`region.resolve(h)` 借的是 `&`，
  `mark_card_dirty` 要 `&mut`，不能在同一个借用里做完。

## Testing Strategy

三个新测试，**全部验证过「不打补丁就红」**：

1. `promoted_owner_keeps_the_young_child_it_was_holding` —— 破口 2，对象区。
   三个 setup 要点缺一不可：owner **藏在 root 后面**（直接 pin 就变空转，见上）；
   写入发生在 owner **还年轻**的时候（否则走的是屏障那条正常路）；
   child 用 600 个 pinned filler 顶出 owner 的 chunk（卡是 chunk 粒度的）。
2. `promoted_array_keeps_the_young_element_it_was_holding` —— 破口 2，数组区。
3. `major_rebuilds_cards_for_surviving_cross_gen_edges` —— 破口 3，直接调
   `run_cycle_collection_major`。

## 实测（同一次会话、同一个 seed、逐条显式 `env`）

`z42c.semantics --release --no-incremental`：

| 配置 | 周期 | minor / major | 墙钟 | 峰值 RSS |
|---|---|---|---|---|
| 未武装（默认） | 0 | — | 6.70 s | 902.9 MB |
| stw 128M | 14 | 0 / 14 | 7.73 s | **606.6 MB** |
| stw 256M | 3 | 0 / 3 | 6.97 s | 761.3 MB |
| **gen 64M** | 35 | 35 / **0** | 7.88 s | 1058.4 MB |
| **gen 128M** | 18 | 18 / **0** | 7.41 s | 1034.6 MB |
| **gen 256M** | 3 | 3 / **0** | 6.82 s | 1003.4 MB |

修前：`gen` 三档全部崩（6/6 必现）。修后全部编完。

## ⚠️ 交给 change 3/4 的实测结论

**分代模式的 RSS 比完全不回收还高**（1034.6 vs 902.9 MB），因为
**18 次 minor、0 次 major** —— 老垃圾一次都没被收，而分代模式额外维护三个 region 的
young 表和卡表。升级启发式（`minor 存活率 ≥ Z42_GC_MINOR_THRESHOLD` 就当场补一次 major）
在这个负载上**从来没触发过**。

这不是本 change 引入的，是本 change 第一次把它**量准**了。`add-bounded-nursery` 的
容量闸门和 `arm-gc-by-default` 的默认值必须解决「什么时候该 major」这个问题，
否则打开分代等于把 RSS 抬高 15%。

## ⚠️ 前一条记录的更正（zsh 分词坑）

#537 design.md 里「三档预算下 generational 都能编完」和「分代 905 MB vs STW 596 MB」
**都是错的**。那些 run 写成：

```zsh
for cfg in "generational 128M"; do set -- $cfg; env Z42_GC_MODE=$1 Z42_GC_MAX_BYTES=$2 … ; done
```

而 **zsh 对未加引号的参数展开不做分词**（这点和 bash 相反）——`$1` 是整串
`"generational 128M"`、`$2` 是空。于是 `Z42_GC_MODE` 拿到非法值被忽略、
`Z42_GC_MAX_BYTES` 空 = **未武装**。三档「RSS 一模一样」正是因为它们本来就是同一次
未武装的 STW 跑。教训：**量 GC 前先确认那一跑真的按你以为的配置在跑**
（`Z42_GC_TRACE=1` 数一下周期数，0 周期就是没武装）。
