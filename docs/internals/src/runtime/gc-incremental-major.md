# 增量 major：SATB 屏障与有界停顿

> **相关**: [GC 调参与 safepoint](gc-tuning.md) · [GC TLAB](gc-tlab.md) ｜ **对齐**: 2026-09-17
> change `add-incremental-major-gc`（M2a：SATB 屏障；M2b：切片调度；M2c 验收完成后补全验收数据）

## 为什么

分代 GC 的**最大停顿全部来自 major**，而 major 的每个阶段都随活堆线性增长：`13_gc_large_heap` 上一次 major
约 60~70 ms（full mark 40 + sweep 20），活堆 ×3 时最大停顿 392 ms。目标是**最大停顿 ≤ 10 ms 且与堆大小无关**，
路线是把 major 拆成有界的 STW 切片，切片之间 mutator 照常运行。这就要求标记在 mutator 改图的同时仍然正确 ——
本页先讲保证正确性的屏障（M2a），再讲切片调度（M2b）。

## 标记位：minor 位 + major epoch

见 [gc-tuning.md「minor 位与 major epoch 同字节分治」](gc-tuning.md)。要点：major 标记 =
「槽里的 epoch 等于本周期 epoch」，开周期即全堆变白；minor 只动 bit 0，切片之间跑 minor 不会擦掉 major 的标记。

## SATB 删除屏障

### 要防的那个洞

```
快照时：root → H（灰，未扫描）→ X（白）
mutator：r = H.f        // X 进了寄存器 —— 寄存器已在快照时扫过
         H.f = null     // 唯一的堆边被剪
marker ：扫描 H，字段已是 null，永远见不到 X
sweep  ：X 被回收，而 r 还攥着它
```

插入式屏障（Dijkstra，染被写入的新值）堵不住它，除非收尾时重扫全部根 —— 现有 `Z42_GC_MODE=concurrent`
正是这样漏的。SATB 走另一头：**覆盖一个堆引用槽之前，把旧值记下来**，标记收尾前把记下的值全部染灰。
配合 allocate-black（周期内出生的对象直接是黑的），就是 Yuasa 论证：快照时可达的每个对象，要么沿原图被走到，
要么它路径上第一条被剪的边的旧值被记录。**根不需要屏障** —— 快照时已整体染灰。

### 屏障放在哪

放在**写原语里**，不在 ~40 个调用点：

| 原语 | 覆盖的写入 |
|---|---|
| `ScriptObject::set_field_value` | 侧表引用叶子；**byte 内联的对象/数组字段**（先 `read_inline_ref` 取旧值） |
| `ScriptObject::set_ref_slot` | 直接写侧表引用叶子（`StructFieldSetPrim`、反射 `SetValue`） |
| `ArrayObj::set_boxed` | 引用数组元素；struct[] 元素的引用叶子 |
| `ArrayObj::write_struct_elem` / `set_struct_ref` | struct[] 元素整体 / 单个引用叶子 |
| `ArrayObj::copy_elems_from` | `Array.Copy` 的批量快路径（Boxed→Boxed 一次 `clone_from_slice`，先整段 `record_overwrite_all`）—— 第一轮审计漏掉，增量 major 的 `Z42_GC_SLICE_MS=0.05` 压测以编译器 SIGSEGV 抓到 |

`refs_mut_raw()` 不带屏障，只给**刚分配的对象**（旧值全是 `Null`）和 **GC 自己断边**（记录死对象会把悬空句柄
塞进标记队列）。规则写在 `../../../agent/rules/runtime-rust.md`。

### 原语手里没有堆 —— 线程本地记录

一个进程里可以有多个 VM 堆。进程级队列会让 A 堆的标记器用自己的 epoch 去染 B 堆的对象，所以：

```
VmContext::new*  ── satb::bind_thread(heap_id)        // 本线程的记录属于这个堆（栈式，嵌套 context LIFO 还原）
写原语           ── record_overwrite(old)
                     if MARKING_HEAPS == 0 { return }  // 标记期外：一次 relaxed load
                     slow: 本线程缓存「我的堆在标记吗、epoch 是几」（MARKING_GEN 变了才刷新）
                           旧值已被本周期标记 → 不记；否则 push 进线程本地 buf
retire_thread_tlab ── buf 交给本堆的 satb_queue          // 每条 park 路径都先 retire，collector 等到全员 park 才继续
close_major_marking ── loop { retire 自己；取 satb_queue；标记入 mark_queue；drain } 直到一轮取空
                       satb::end_marking(heap_id)
```

`open_major_cycle`（开 epoch + `begin_marking`）与 `close_major_marking` 接在 STW 周期和并发周期（Phase 1 / Phase 5）上。
在一次性 STW 周期里屏障实际不起作用（标记期没有 mutator 在写），但并发模式的 Phase 3 有 —— **M2a 起并发模式的
上述漏洞已被堵上**。

### 弱 / 软引用读取

一个快照时只剩弱引用的对象，可以经 `upgrade_weak` / 弱 `GcHandle` / `soft_ref_get` 被读回寄存器。SATB 的前提
「快照时不可达的对象不会再变可达」被它打破，所以这三条读路径在标记期把结果交给 `shade_if_marking`（进 `satb_queue`）。

### 标记期中的 minor

`satb_queue` 与 `mark_queue` 里的**年轻**条目是 minor 的**额外根**。否则：屏障记录了一个年轻对象，
随后的 minor 判它死、回收掉，major 再从队列里拿到一个悬空句柄。

真正被这条规则保住的是灰条目**还没被追到的子节点**——条目本身早有 `keep_major` 兜底（见「切片调度」），
而「灰」的定义就是*已标记、尚未追踪*，所以只经由它可达的子节点仍是白的。

**老条目不入根**（`trim-minor-cycle-roots`, 2026-09-18）。minor 根本不回收老年代条目，
把它们当根唯一的效果是多追一层子节点，而那一层里唯一有意义的（年轻子节点）卡表已经覆盖。
对任意一个老灰条目 X，二者必居其一：

| X 的卡 | 结论 |
|---|---|
| **脏** | `seed_from_dirty_cards` 已经在追 X 并把它的年轻子节点入队 ⇒ 灰根这一 push 是**逐字重复** |
| **干净** | 卡只有在「其中每个条目都不再指向任何年轻对象」时才被洗掉，而老→年轻边的两条产生路径（写屏障 `maybe_mark_cross_gen_card`、晋升 `dirty_cards_for_newly_old_*`）都会重新染脏 ⇒ X 没有年轻子节点，追它产出为空 |

这正是 minor BFS 一直依赖的那条不变量——它本来就**不把老年子节点入队**，理由一字不差。

两条支撑它的前提，改动前必须先确认它们还成立：

1. **minor 永不回收「按年龄算已老」的条目**。`Region::sweep_young_in_one_pass` 只走 `young_list`，
   而自适应晋升只在「mark 已用旧线跑完、紧接着的升龄趟会把新线判老的全部排空」那个窗口里降线
   （`Region::set_promotion_age`）。`VarRegion::sweep_young` 另有**显式**跳过
   （`fix-old-block-left-in-young-list`——`age_backing_with_owner` 会在不摘表的情况下抬高 backing 年龄）。
2. **`region_var` 没有卡表**（`maybe_mark_cross_gen_card` 的 `_ => {}` 臂），所以上面那条论证对 var 条目
   要逐个变体核查：`Str` / `FuncRef` 是叶子；闭包的 `env` 在构造时固定、此后与闭包同步升龄
   （每次够得着闭包的 minor 都会标记 env 表头）⇒ 老闭包的 env 必老，而 env 是**数组**、有卡；
   数组的元素 backing 由 `age_backing_with_owner` 保证不比属主年轻，且 minor 追老数组时传 `None`、
   本来就不 `mark_backing`。

队列本身原封不动——标记器仍然拿到它记录的每一条，SATB 的快照承诺不受影响；变的只是**minor 播种什么**。

**这笔账有多大 —— 比想象的小得多，别再来挖第二遍**（2026-09-18 实测）。
账面上是唬人的：`13_gc_large_heap --large` 每次 minor 拷 **118 690** 个老条目，
78 次 minor 累计 683 092 条灰根里 **68.3%** 是老的。但**去掉它买不到停顿**：
A/B（同机交错 ×3）总停顿 −1.4%，最大停顿在逐轮离散度之内，墙钟 / RSS 持平；
固定 nursery 的对照组同样无收益。直接量更干脆——从一次 **100 ms** 的 minor 里拿掉
**45 000** 个老灰根，`minor mark` 纹丝不动（102/99/92 → 88/98/95 ms）。

原因就写在上面的论证里：这批老条目**本来就要被卡表扫一遍**，省掉的只是「第二遍」。
两笔账的比例是：

```
同一次 minor：灰队列跳过老条目    87 855
              卡表播种老条目  2 088 956     ⇒ 4.2%
整个 run：    606 506 / 8 859 716          ⇒ 6.8%
```

⇒ **一次 minor 的固定成本只有一个大头：卡表扫描的 O(老年代)**（`--large` 上单次峰值
**2 088 956** 个老条目）。灰队列是它的零头。要继续压 minor 停顿，砍卡表或做增量 minor，
别再回来动根集。

## 切片调度（M2b）

### 周期的形状

```text
 Idle ─trip→ [开周期：epoch++、SATB 开、allocate-black 开、根染灰] ─→ Marking
 Marking ： 预算内追灰队列；队列空 → 取 SATB 记录染灰 → 再追；两者都空 → 软引用复活一次 → SATB 关 ─→ Sweeping
 Sweeping： 对象区 → 数组区 → var 区，一次一个 chunk，预算内推进游标
 收尾    ： chunk 回收、allocate-black 关、context / 软引用表、晋升年龄切换 ─→ Idle
```

每个切片都是一次普通的 STW 暂停（`request_gc_pause`），预算 `Z42_GC_SLICE_MS`（默认 2 ms；native 看时钟，
wasm32 没有时钟改数工作量，单测也用工作量以保证交错确定）。**标记从不与 mutator 并发执行** ——
切片之间世界照常前进，靠三条不变量跨过去：SATB（被剪的边交出旧值）、allocate-black（周期内出生即黑）、
minor 把灰队列与 SATB 记录当根。状态机在 `gc/arc_heap/incremental.rs`（`IncrementalState` 是 `ArcMagrGC` 上的一个字段）。

预算按「追踪一个值 = 1 单位、清扫一个定长 chunk = 256 单位」计；**一个超大数组算 1 单位**（`trace_children`
一次压进全部元素），所以有一次切片会超出预算 —— 实测 `13_gc_large_heap`（5 万元素的根数组）的最长标记切片 2.2 ms。

### Sweeping 期新出现的一类对象：待清扫的死对象（doomed）

标记一结束，每个 alive 条目要么带本周期 epoch，要么是清扫游标还没走到的**垃圾** —— 它仍是 `alive`，
而它的子对象**可能已经被前面的切片回收、槽位被新对象复用**（游标按 chunk 顺序走，不按图的顺序）。所以它绝不能再被追踪、
也绝不能交给 mutator：

| 路径 | 不处理会怎样 | 处理 |
|---|---|---|
| 弱 / 软引用读、`iterate_live_objects` | 把马上要被回收的句柄放进寄存器 | `admit_resurrected`：Sweeping 期未标记 ⇒ 拒绝（返回 null / 跳过）；Marking 期照 M2a 染色 |
| minor 的脏卡播种 | 老的死对象 D 在脏卡里，D.f 指向已回收并被复用的槽 ⇒ minor 追进别人的对象（`use-after-finalize`） | `seed_from_dirty_cards`：Sweeping 期跳过未标记条目 |
| debug 校验器 | 周期中灰队列非空、老 epoch 合法存在 | 周期打开时只做 region 结构校验 |

还有一处**定长 region 的 young 表**：一次性 major 清扫时不从 young 表删死条目，因为紧接着的升龄趟会顺手删掉。
增量清扫之间有 minor，而死条目的槽一旦被复用，同一个 `(chunk, entry)` 就在 young 表里出现**两次** ——
minor 第一次访问保留新对象并清掉 minor 位，第二次访问判它死、回收一个活对象。所以增量清扫调
`Region::sweep_chunks(delist_young = true)`，边 tombstone 边摘表。

### 调度：一次停顿只做一件事

自动回收策略（`auto_collect.rs`）把请求的种类放进标志位，safepoint 上的回收再按标志位决定这一次停顿做什么
（`choose_generational_work`）：

| 周期状态 | 请求 | 这次停顿 |
|---|---|---|
| 未开 | major（晋升字节闸门 / 升级 / 自适应晋升） | 开周期（第一个切片） |
| 未开 | major **且** minor | **minor**；周期在下一个 safepoint 开 —— 增量周期没有升龄趟，升龄是 minor 的活，开周期不能顶掉它 |
| 进行中 | minor | minor |
| 进行中 | 切片 / major | 下一个切片（周期进行中晋升字节闸门不再开新周期） |
| 进行中 | 触及软上限 | **同步做完整个周期** |
| 进行中 | 什么都没请求 | 显式 `GC.Collect()` ⇒ **同步做完** |

`force_collect`（`GC.ForceCollect()`、保留图诊断）也先同步做完打开的周期。

### 周期期间的 minor：不得回收标记器已经标记的东西

标记器对「快照时可达的一切」做出承诺，而 minor 的可达性视角更窄（根 + 脏卡 + 灰队列 / SATB 队列）。
周期打开时，`Region::sweep_young_in_one_pass` / `VarRegion::sweep_young` 因此多收一个 `keep_major`：
**带本周期 epoch 的条目与变长块，在 minor 里一律算活**。这与「灰队列和 SATB 记录是 minor 的额外根」是同一条原则的延伸
——它们正是周期已经承诺保住的那批对象。

不这么做会怎样：`Z42_GC_SLICE_MS=0.05` 跑 `z42.net` 全套 stdlib 测试，**关掉这条规则两轮都在 ~6 个测试后挂死**
（编译器读到已被回收的字符串 / 数组 backing：SIGSEGV，或 `str_meta` 拿到垃圾长度转圈），**打开后连续 5 轮 50/50 全过**。
代价是周期期间的浮动垃圾。

### 代价落在 chunk 碎片上，不是浮动垃圾

`13_gc_large_heap` 上 M2b 的峰值 RSS 比一次性 major 高 30~43%，但**GC 计账的峰值 `used` 一样**（339M vs 355M）。
差在 **chunk 数**：切片与周期期间的保留让幸存者散布在更多 chunk 里，而 chunk 只有全死才回池、TLAB 又只整块取、
不复用块内空洞。⇒ 这笔账属于「空洞复用」那条线，不是切片本身的结构代价；两个便宜旋钮（收紧 pacer 目标、
把保留缩到只覆盖标记期）各能拿回 8~10%，但都不足以到 ≤5%，且后者会削弱刚立起来的正确性保证。

### pacer：按剩余工作量排切片

周期开的时候定一个目标：**在堆比开周期时再长半个 allowance 之前做完**（一次性 major 让堆峰值落在 `live + allowance` 附近，
周期期间死掉的对象要等下个周期才能回收，所以只给一半）。每个切片结束时：

```text
剩余切片 = (预计工作量 − 已完成) / 平均每切片工作量
下一次切片前允许的堆增长 = (目标 − 当前 used) / 剩余切片      clamp 到 [64 K, nursery/4]
```

预计工作量取上一个周期的实际值，首个周期用 region 的 O(1) 上界估计。落后（已过目标或估计偏低）时间隔触底，
切片几乎首尾相接 —— **让出去的是吞吐，不是停顿上界**。

固定间隔做不到：最初「每 1/4 nursery 一个切片」在 `13_gc_large_heap` 上一个周期要 30~37 片、跨 ~8 个 nursery 的分配，
期间 churn 死掉的全部浮到下个周期，峰值 RSS 从 878 MB 涨到 1.42 GB；换成 pacer 后 ~970 MB。

切片不参与徒劳退避（标记切片按设计不回收任何东西），也不移动 minor 闸门的水位 —— 否则每 1/4 nursery 一个切片会
把 minor 的「已增长」不停清零、把 minor 饿死。

## 切片化之后：剩下的长停顿全是 minor

major 的停顿与堆大小脱钩之后，实测剩余的最大停顿**全部来自 minor**，于是
[`add-pause-budget-nursery`](gc-tuning.md#按停顿预算自适应-nursery) 接着把 nursery 从常量改成
按实测代价反推的量。那一轮实测也反过来暴露了本机制留下的两个账，都记在这里：

- **开着的周期会把 grey 队列变成每次 minor 的根**（M2a 的 `queue.extend(mark_queue)`）。
  实测 `13_gc_large_heap --large` 上是 **108 142** 个老条目，*每次* minor 重新遍历一遍
  （`Z42_GC_PHASES` 的 `minor roots` 行可见）。这些老条目的年轻子节点**卡表已经覆盖** ——
  minor 的 BFS 本身就靠这条不入队老 children —— 所以这份工作很可能是白做的。
- **清扫切片会把下一次 minor 推远**。`rearm_auto_collect` 以*当前* `used` 为锚，而切片释放的字节
  也走 `sub_used_bytes` 落到那里，于是每个切片都把 minor 闸门往后推至多一个闸门。实测
  `z42c.semantics` 打出 `trip minor gate 18.1M grown 30.2M` —— **超调 67%**。

两条都需要各自的 change 与设计评审（改锚点的原型把 semantics 打到 11.5 ms，但让
`13_gc_large_heap --large` 从 22.8 ms 崩到 131.6 ms）。

## 代价

标记期外每次堆引用写多一次 relaxed load + 不跳转的分支。实测（与只有 epoch 改动的二进制交错）：
`09_alloc_ctorless` 指令 +0.26%，`z42c.semantics` −0.03%（噪声内），编译产物逐字节一致。

## 测试

`src/runtime/src/gc/arc_heap_tests/incremental.rs`，手工驱动周期（开周期 → 快照根 → mutator 动作 → drain → 收尾 → sweep），
存活性一律经弱引用观察，不解引用可能已被回收的句柄：

- 字段读进寄存器再清字段：开屏障存活 / **关屏障被回收**（阴性对照，证明测试能判别）
- `set_field_value`（byte 内联引用）/ 数组元素覆盖同样被记录
- 标记期外不记录；另一个堆在标记时本线程的记录不会进它的队列
- 标记期弱读：读了存活 / 不读被回收
- minor 把 SATB 记录当根：有记录存活 / 无记录被回收

M2b（同一文件，切片用工作量预算手工驱动，交错完全确定）：

- 周期跨多个切片、回收垃圾、保留根链
- 真实切片之间的「字段读进寄存器再清字段」：开屏障存活 / 关屏障丢失
- 标记期、清扫期出生的对象都活过本周期，下个周期被回收
- 清扫到一半：弱读拒绝待清扫的死对象、堆遍历不列出它
- 切片之间跑 minor：屏障记录的年轻对象活过 minor / 关屏障被回收
- 增量清扫释放的槽在下一次 minor 前被复用（young 表摘除的回归测试；关掉摘除即红）
- 死的老对象在脏卡里、子对象先被清扫并复用、再跑 minor（关掉 doomed 跳过即 `use-after-finalize`）
- 调度表逐行；随机写 / 寄存器移动 / minor / 微小切片交错 6000 步后全图可解引用 + 不变量

**模型 D**（`src/runtime/tests/gc_incremental_model.rs`）：6 个对象的抽象堆，collector / minor / 两个 mutator 四条脚本，
**穷举全部交错**（状态记忆化）。每个切片和每个 mutator 步都是整体原子的（切片停世界，mutator 只在 safepoint 之间），
所以「有没有一种顺序能打破不变量」就是枚举顺序 —— 不需要 loom，而且跑在默认测试集里。完整策略全绿；
逐个关掉 SATB / allocate-black / 队列作 minor 根 / 弱读拒绝 doomed / minor 跳过 doomed 卡，**每一个都给出反例**。
