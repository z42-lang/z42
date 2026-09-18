# Design: 周期中的 minor 只把**年轻**的灰条目当根

## 现状（M2a 定下的规则）

`mark_phase_minor`（`gc/arc_heap/generational.rs`）在拼根集时：

```rust
queue.extend(self.satb_queue.lock().iter().cloned());   // SATB 记录
queue.extend(self.mark_queue.lock().iter().cloned());   // 灰队列
```

两条都是**无差别全量拷贝**。灰队列是增量 major 标记器的工作集（M2b 的切片超预算时把本地栈放回这里），
在一个周期里长期维持在几万~十几万条的规模，而**周期打开期间会发生几十次 minor**。

## D1 —— 核心论证：老条目入根**不可能**改变结果

先确立两条前提，都已在代码中坐实：

**前提 A：minor 永不回收「按年龄算已老」的条目。**
- 对象/数组区：`Region::sweep_young_in_one_pass` 只走 `young_list`。降线（自适应晋升）只在
  `set_promotion_age` 的窗口里发生——「mark 已用旧线跑完、紧接着的晋升趟会把新线判老的全部排空」
  （`region/generation.rs:205` 的注释 + `age_survivors_after_major`）⇒ 下一次 mark 时 `young_list` 里不含已过线的条目。
- var 区：`VarRegion::sweep_young` 里有**显式**的 `gen_age() >= threshold ⇒ 跳过并摘表、不回收`
  （`fix-old-block-left-in-young-list`，因为 `age_backing_with_owner` 会在不摘表的情况下抬高 backing 的年龄）。

**前提 B：老条目 X 的年轻子节点，卡表一定覆盖得到。**
X 成为「老 → 年轻」边的持有者只有两条路，两条都染脏：
① 写屏障 `maybe_mark_cross_gen_card`（X 当时已老、写入年轻值）；
② 晋升 `dirty_cards_for_newly_old_{objects,arrays}`（X 刚过线，`refers_to_young` 为真）。
（`rebuild_card_table` 是第三条，走同一判据。）
而卡只有在**扫过且其中每个条目都不再指向任何年轻对象**时才被 `clean_card` 洗掉。

于是对任意一个**老**灰条目 X，二者必居其一：

| X 的卡 | 结论 |
|---|---|
| **脏** | `seed_from_dirty_cards` 已经在追 X 并把它的年轻子节点入队（`seed_card_entry` 的 old 分支）⇒ 灰根这一push 是**逐字重复** |
| **干净** | 按前提 B，X 没有任何年轻子节点 ⇒ 追它只会遍历一圈老指针，**产出为空** |

两种情况都不丢东西。**这正是 minor BFS 一直依赖的那条不变量**——它本来就「不把老年子节点入队」，
理由一字不差。老灰条目今天唯一的效果是被 `gen_age_of` 判老 → 不打标记 → 追一层子节点 → 年轻的那些
（卡表已经给过）入队、老的那些被过滤掉。

**⇒ 改动：灰队列与 SATB 队列播种时按 `gen_age_of(v) < threshold` 过滤，只播种年轻条目。**

年轻条目**必须**留：minor 会回收年轻条目，而标记器正攥着它们的句柄——这是 M2a 那条规则真正在保护的东西。

## D2 —— 例外核查：var region 没有卡表

`maybe_mark_cross_gen_card` 的 `_ => {}` 说明 **var 区（字符串 / 闭包 / funcref）没有卡**。
所以前提 B 对 var 区条目不成立，必须逐个变体核查灰队列里可能出现的 var 值：

- `Value::Str` / `Value::FuncRef`：`visit_gc_children` 里是**叶子**，没有子节点可丢。✅
- `Value::Closure`：子节点是 env 数组的**表头**（`Value::Array`）+ `fn_name` 串。
  「老闭包 + 年轻 env」是唯一的危险形状，而它不可能出现：`data.env` 在构造时固定、此后不再改写；
  env 数组的分配不晚于闭包；此后每一次能看见闭包的 minor 都会追它 → 标记 env 表头 → 两者**同步升龄**
  （闭包某次没被标记就直接死了）。⇒ 闭包老 ⇒ env 老，而 env 是**数组区**条目、有卡，回到 D1。✅
- `Value::Array` 的元素 backing 是 var 块：`age_backing_with_owner` 保证 backing 年龄 ≥ 属主年龄
  ⇒ 老数组 ⇒ 老 backing。且今天 minor 追老数组时传的是 `None`（不打标记、**不** `mark_backing`），
  与去掉这个根之后完全一致。✅

## D3 —— 与清扫期 / doomed 的关系：无交互

`doomed_unless_marked()` = `major_cycle_sweeping().then(...)`，**只在 Sweeping 阶段**返回 `Some`。
而灰队列在进入 Sweeping 之前已经被 `end_marking` 排空、SATB 屏障也随 `MARKING_ACTIVE` 关闭。
⇒ 本 change 只影响 Marking 阶段的根集，与 M2b 修的「清扫期不得回收 major 已标记条目」（`keep_major`）正交。

## D4 —— SATB 队列是否一并处理

论证与 D1 **逐字同构**：SATB 记录的是被删除的旧值 V。V 年轻 ⇒ 留作根（minor 会回收它）；
V 老 ⇒ minor 不回收它，其年轻子节点由 V 自己的卡覆盖。本 change 只改**minor 播种什么**，
`satb_queue` 本身原封不动交给标记器 ⇒ SATB 的快照承诺不受影响。

**实测规模（见 D5）：SATB 在四个负载上分别是 0 / 0 / 0 / 481 条，收益可忽略。**
建议仍然一并改，理由不是数字而是**规则的一致性**——两条队列是因同一条理由成为 minor 根的，
只给其中一条加年龄限定，会留下一个下一个读者必须重新推导的不对称。
（若 User 倾向最小风险面，砍掉 SATB 这半边不影响 D1 的收益。）

## D5 —— 实测：这笔账有多大（main `a76d03f76` + 纯诊断探针）

**账面规模**（一次跑，纯诊断探针，不改行为）：

| 负载 | minor 次数 | 灰条目累计 | 其中**老** | 单次 minor 峰值 | SATB 累计 |
|---|---|---|---|---|---|
| `13_gc_large_heap --large` | 78 | 683 092 | **466 629（68.3%）** | **118 690** | 0 |
| `13_gc_large_heap` | 52 | 145 857 | **120 975（82.9%）** | 42 207 | 0 |
| `09_alloc_ctorless` | 4 | 741 | 0（0%） | 741 | 0 |
| `12_gc_churn` | 10 | 0 | 0 | 0 | 0 |
| `z42c.semantics`（编译器自举） | 24 | 1 070 | 924（86.4%） | 711 | 481 |

## D5b —— 🔴 **实测推翻了立项时的收益预期**（2026-09-18）

M3 的笔记把这笔账记成「一次 minor 的两个固定成本大头之一」，并记了一个原型数字 **总停顿 −8%**。
**两条都不成立。**

**A/B（同机交错 ×3，两个二进制，`base` = main `a76d03f76`）**：

| 负载 | 最大停顿 | 总停顿 | 墙钟 | RSS |
|---|---|---|---|---|
| `13_gc_large_heap` | −10.4% | −0.2% | −2.3% | −0.5% |
| `13_gc_large_heap --large` | −0.6% | **−1.4%** | −0.1% | −0.1% |
| `09_alloc_ctorless` | +6.1% | +5.2% | +4.9% | +1.9% |
| `12_gc_churn` | +1.7% | −4.4% | −2.2% | −1.3% |
| `z42c.semantics` | −1.9% | +1.3% | −0.3% | −1.0% |

产物逐字节一致（6 份 `z42c.semantics.zpkg` 同一个 md5）。`09`/`12` 的 ±5% 是噪声——
这两个负载上灰队列里**一个老条目都没有**，改动对它们是彻底的 no-op。
`lh` 的「最大停顿 −10.4%」同样是噪声：逐轮 base 是 55.7 / 76.7 / 64.4，trim 是 55.0 / 72.8 / 57.7，重叠得很厉害。

**排除了「收益被自适应 nursery 吃掉」这个假设**：M3 的控制器按实测代价反推 nursery，理论上会把省下的
固定成本换成更大的 nursery 而不是更短的停顿。固定 `Z42_GC_NURSERY_BYTES=16M` 重测 ×3：
`--large` 总停顿 −0.4% / 最大 −3.5%，`lh` 总停顿 +8.1% —— **同样没有收益**。

**为什么**（直接量，不是推断）。拿 `--large` 最大的那几次 minor 比 `minor mark` 耗时：

| | 播种的灰根 | 跳过的老灰条目 | `minor mark` |
|---|---|---|---|
| base | 117 810 / 117 370 / 116 726 | — | 102.2 / 98.9 / 92.5 ms |
| trim | 72 884 / 71 906 / 71 776 | ~45 000 | 87.8 / 97.5 / 94.7 ms |

**从一次 100 ms 的 minor 里拿掉 45 000 个老灰根，耗时纹丝不动。** 因为同一批老条目**本来就要被卡表扫一遍**
（这正是 D1 的论证），省掉的只是「第二遍」的 clone + pop + 一次 `visit_gc_children`。
而这笔账相对于它旁边那笔的比例是：

```
同一次 minor：灰队列跳过老条目    87 855
              卡表播种老条目  2 088 956     ⇒ 灰队列 = 卡表的 4.2%
整个 run：    灰队列 606 506 / 卡表 8 859 716 ⇒ 6.8%
```

⇒ **「两个固定成本大头」的说法要改**：真正的大头只有**一个**，是卡表扫描的 O(老年代)；
灰队列是它的一个零头。M3 笔记里的 −8% 无法复现，也和这里的算术对不上
（466 629 个条目就算按 150 ns/条算也只有 70 ms，占 3 798 ms 总停顿的 1.8%——**天花板就在 2% 上下**）。

**这条留给下一个人**：`13_gc_large_heap --large` 的 `card seed` 单次峰值 **2 088 956 个老条目**
（一次 minor！），才是 10 ms 目标的真正障碍。下一刀应该砍那里，或者做增量 minor。

## D6 —— 被否决的替代方案

1. **给 var region 补卡表**：能让论证少一个分支，但 `fix-primitives-count-as-young` 的历史证据表明
   「给每个条目付一张卡」会把卡表点着（`z42.text.levenshtein` 回归 28%）。本 change 不需要它——D2 已逐个变体核查过。
2. **只播种灰条目的年轻*子节点*而不是条目本身**：更精确，但与 `seed_card_entry` 的 young 分支同款
   ——那条路实测**省 24% 中位停顿却多付 6.3% 峰值 RSS**，是一笔独立的精度/RSS 交易，不该混进来。
3. **让标记器维护两条队列（年轻/老分开）**：省掉 minor 侧的年龄判断，但把复杂度推给热路径的标记器，
   且年龄会在周期中途变化（晋升）⇒ 队列会失准。年龄判断放在 minor 播种这一侧是唯一不会腐坏的位置。

## 诊断

`Z42_GC_PHASES` 的 `minor roots` 一行改为区分播种与跳过：

```
minor roots  pinned+handles 12, satb 0 (skipped old 0), grey 3 (skipped old 118690)
```

这笔账此后长期可见——M3 的教训是「固定成本看不见就会被当成 0」。
