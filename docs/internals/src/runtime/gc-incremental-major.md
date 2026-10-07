# 增量 major：SATB 屏障与有界停顿

> **相关**: [GC 调参与 safepoint](gc-tuning.md) · [GC TLAB](gc-tlab.md)

## 为什么

分代 GC 的**最大停顿全部来自 major**，而 major 的每个阶段都随活堆线性增长：`13_gc_large_heap` 上一次 major
约 60~70 ms（full mark 40 + sweep 20），活堆 ×3 时最大停顿 392 ms。目标是**最大停顿 ≤ 10 ms 且与堆大小无关**，
路线是把 major 拆成有界的 STW 切片，切片之间 mutator 照常运行。这就要求标记在 mutator 改图的同时仍然正确 ——
本页先讲保证正确性的屏障，再讲切片调度。

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
| `ArrayObj::copy_elems_from` | `Array.Copy` 的批量快路径（Boxed→Boxed 一次 `clone_from_slice`；struct[]→struct[] 的引用叶子区间一次拷贝；都先整段 `record_overwrite_all`）—— 第一轮审计漏掉，增量 major 的 `Z42_GC_SLICE_MS=0.05` 压测以编译器 SIGSEGV 抓到 |
| `ArrayObj::copy_elems_within` | 同一数组内的 `Array.Copy`（struct[] 的引用叶子区间 `copy_within`，先整段 `record_overwrite_all`；Boxed 走 `set_boxed`） |

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
在一次性 STW 周期里屏障实际不起作用（标记期没有 mutator 在写），但并发模式的 Phase 3 有 —— **并发模式的
上述漏洞由屏障堵上**。

### 弱 / 软引用读取

一个快照时只剩弱引用的对象，可以经 `upgrade_weak` / 弱 `GcHandle` / `soft_ref_get` 被读回寄存器。SATB 的前提
「快照时不可达的对象不会再变可达」被它打破，所以这三条读路径在标记期把结果交给 `shade_if_marking`（进 `satb_queue`）。

### 标记期中的 minor

`satb_queue` 与 `mark_queue` 里的**年轻**条目是 minor 的**额外根**。否则：屏障记录了一个年轻对象，
随后的 minor 判它死、回收掉，major 再从队列里拿到一个悬空句柄。

这条规则保住的不只是灰条目本身，还有它**还没被追到的子节点**——「灰」的定义就是*已标记、尚未追踪*，
所以只经由它可达的子节点仍是白的。周期的标记本身保不住年轻条目（见「周期期间的 minor：年轻代归 minor 管」），
能保住它们的只有 minor 自己的根集。

**老条目不入根**。minor 根本不回收老年代条目，
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
   （`age_backing_with_owner` 会在不摘表的情况下抬高 backing 年龄）。
2. **`region_var` 没有卡表**（`maybe_mark_cross_gen_card` 的 `_ => {}` 臂），所以上面那条论证对 var 条目
   要逐个变体核查：`Str` / `FuncRef` 是叶子；闭包的 `env` 在构造时固定、此后与闭包同步升龄
   （每次够得着闭包的 minor 都会标记 env 表头）⇒ 老闭包的 env 必老，而 env 是**数组**、有卡；
   数组的元素 backing 由 `age_backing_with_owner` 保证不比属主年轻，且 minor 追老数组时传 `None`、
   本来就不 `mark_backing`。

队列本身原封不动——标记器仍然拿到它记录的每一条，SATB 的快照承诺不受影响；变的只是**minor 播种什么**。

**这笔账有多大 —— 比想象的小得多，别再来挖第二遍**（实测）。
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

## 切片调度

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
| 弱 / 软引用读、`iterate_live_objects` | 把马上要被回收的句柄放进寄存器 | `admit_resurrected`：Sweeping 期未标记 ⇒ 拒绝（返回 null / 跳过）；Marking 期照 SATB 染色 |
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
| 进行中 | minor | minor（年轻代策略判 minor 不划算时是 **tenure**，任何周期状态下同理；见下） |
| 进行中 | 切片 / major | 下一个切片（周期进行中晋升字节闸门不再开新周期） |
| 进行中 | 触及软上限 | **同步做完整个周期** |
| 进行中 | 什么都没请求 | 显式 `GC.Collect()` ⇒ **同步做完** |

`force_collect`（`GC.ForceCollect()`、保留图诊断）也先同步做完打开的周期。

### 周期期间的 minor：年轻代归 minor 管

周期打开时，minor 判年轻条目的死活**只看 minor 自己到没到达**——根、寄存器、脏卡、灰队列与 SATB 记录里的年轻条目。
本周期 epoch 在年轻条目上只是「周期内出生」的标签，用在三处：major 清扫保留它、它不算 doomed、SATB 不记录它；
minor 不认它（`Region::sweep_young_in_one_pass` / `VarRegion::sweep_young`）。于是周期内出生即垃圾的对象
被周期内的下一次 minor 收掉，而不是在年轻代里占位到周期结束。

**为什么安全。** 设 minor 回收了一个它没到达的年轻条目 X，要说明此后没有人会再解引用 X：

- **mutator**：寄存器与栈是 minor 的根；老对象指向 X 的边必有脏卡（写屏障 `maybe_mark_cross_gen_card`、
  晋升时的 `dirty_cards_for_newly_old_*`），脏卡里的条目也是根。唯一被跳过的脏卡条目是 doomed 持有者，
  它本身就不会再被追踪或交给 mutator。弱 / 软引用读在 Sweeping 期拒绝未标记的条目，在 Marking 期把结果染灰。
- **标记器**：灰队列与 SATB 记录里的年轻条目都是 minor 的根，所以 X 不在其中；黑条目不会被再次追踪。
  新生对象带 epoch，既不进 SATB 也不进灰队列。
- **槽复用**：`RegionEntry::new` / var 块头重建时把标记字节清零，新住户不继承 X 的 epoch；
  弱引用靠 generation 防 ABA。

**promote-black 自然成立**：周期内出生的对象晋升时本来就带 epoch；周期前出生的年轻对象若在 Marking 期
晋升，Yuasa 论证保证它在标记结束前会被标到。所以 Sweeping 期间 minor 晋升的每个条目都必须已带本周期 epoch——
否则它是一个清扫马上要回收、却仍被引用的老条目。两个 sweep 里各有一条 debug 断言守着这一点（`promote_black`）。

**不能做的**：对周期前出生、还没被追踪的条目「只标黑、不入灰」。反例是快照时 root → Y（年轻）→ O（老）：
Y 被提前涂黑后再也不会被追踪，O 仍可达却被清掉。模型 D 的 `promote_marks_without_grey` 对照给出的就是这条。

压力配方 `Z42_GC_SLICE_MS=0.05` 跑 `z42.net` 全套 stdlib 测试，让 minor 最大程度地插进周期中间，是这条规则的回归检查。

### 周期期间的 tenure

年轻代策略（[gc-tuning.md「年轻代策略」](gc-tuning.md#年轻代策略按收益判定-minor不划算就-tenure)）可以把策略要的
minor 换成 **tenure**：年轻代整体晋升、不标记、不扫卡。它插在周期任意位置都安全，论证复用上面两条：

- **标记期**：被晋升的周期前出生的白条目若快照可达，Yuasa 论证保证标记结束前标到它（同 minor 在标记期晋升）。
  tenure 不给它置标记位 —— 置了就是「只标黑、不入灰」那个反例。
- **清扫期**：标记已完成，不带 epoch 的年轻条目是清扫游标还没走到的垃圾。tenure 把它们**留在 young 表**，
  由清扫回收（`sweep_chunks(delist_young = true)` 顺手摘表）；晋升的条目因此都带 epoch，`promote_black` 照样成立。
- **卡**：tenure 之后除了上面那些垃圾没有年轻条目，所以不需要为新晋升的老条目置卡。

tenure 的垃圾成了老年代垃圾、活过本周期，由下一个周期回收 —— 这是它与 minor 的唯一行为差别，也是策略只在
minor 的收益远低于 major 时才用它的原因。

### 代价落在 chunk 碎片上，不是浮动垃圾

`13_gc_large_heap` 上切片化 major 的峰值 RSS 比一次性 major 高 30~43%，但**GC 计账的峰值 `used` 一样**（339M vs 355M）。
差在 **chunk 数**：切片与周期期间的保留让幸存者散布在更多 chunk 里，而 chunk 只有全死才回池、TLAB 又只整块取、
不复用块内空洞。⇒ 这笔账属于「空洞复用」那条线，不是切片本身的结构代价。

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

major 的停顿与堆大小脱钩之后，剩余的最大停顿**全部来自 minor**，所以 nursery 按
[停顿预算自适应](gc-tuning.md#按停顿预算自适应-nursery)。本机制还留下一笔未解的账：

- **清扫切片会把下一次 minor 推远**。`rearm_auto_collect` 以*当前* `used` 为锚，而切片释放的字节
  也走 `sub_used_bytes` 落到那里，于是每个切片都把 minor 闸门往后推至多一个闸门。实测
  `z42c.semantics` 打出 `trip minor gate 18.1M grown 30.2M` —— **超调 67%**。

这条需要单独的设计评审（改锚点的原型把 semantics 打到 11.5 ms，但让
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

切片调度（同一文件，切片用工作量预算手工驱动，交错完全确定）：

- 周期跨多个切片、回收垃圾、保留根链
- 真实切片之间的「字段读进寄存器再清字段」：开屏障存活 / 关屏障丢失
- 标记期、清扫期出生的对象都活过本周期，下个周期被回收
- 周期内出生即垃圾的对象（对象 / 数组 / 字符串各一）被周期内的下一次 minor 收掉；周期标记过、minor 够不着的年轻对象同样被收掉
- 清扫到一半：弱读拒绝待清扫的死对象、堆遍历不列出它
- 切片之间跑 minor：屏障记录的年轻对象活过 minor / 关屏障被回收
- 增量清扫释放的槽在下一次 minor 前被复用（young 表摘除的回归测试；关掉摘除即红）
- 死的老对象在脏卡里、子对象先被清扫并复用、再跑 minor（关掉 doomed 跳过即 `use-after-finalize`）
- 调度表逐行；随机写 / 寄存器移动 / minor / 微小切片交错 6000 步后全图可解引用 + 不变量（含 Sweeping 期晋升必带 epoch 的断言）

**模型 D**（`src/runtime/tests/gc_incremental_model.rs`）：9 个槽的抽象堆（含一个只经年轻对象可达的老对象、
逐对象的卡表），collector / minor / 两个 mutator 四条脚本，
**穷举全部交错**（状态记忆化）。每个切片和每个 mutator 步都是整体原子的（切片停世界，mutator 只在 safepoint 之间），
所以「有没有一种顺序能打破不变量」就是枚举顺序 —— 不需要 loom，而且跑在默认测试集里。每一步之后检查：
清扫期间从根和寄存器可达的老对象必须已标记。

模型用开关 `minor_honors_cycle_marks` 同时判定两种 minor 策略：

- **年轻代归 minor 管**（完整策略，即运行时的策略）：年轻条目只有被 minor 自己到达才活过 minor，epoch 戳只当「周期内出生」的标签。
  安全性全绿，并且满足活性断言「出生即垃圾的对象被周期内的 minor 收掉」。
- **`keep_major`**（对照：minor 保留周期已标记的一切）：安全性全绿，活性断言给出反例 ——
  周期内出生即垃圾的对象活过周期内的 minor。

两次年轻代回收中的任意一次或两次换成 **tenure**（`Policy::young`），安全性全绿；终检多给 tenure 的垃圾
一个完整周期。让 tenure 给它晋升的东西置标记位，给出与「只置标记位、不入灰」相同的反例。

在完整策略上逐个关掉 SATB / allocate-black / 队列作 minor 根 / 弱读拒绝 doomed / minor 跳过 doomed 卡 /
老←年轻写的卡屏障，或者让晋升「只置标记位、不入灰」，**每一个都给出反例**；最后一种的反例是：
年轻 X 被提前涂黑后不再被追踪，它的老子对象 O 仍可达却被清掉。
