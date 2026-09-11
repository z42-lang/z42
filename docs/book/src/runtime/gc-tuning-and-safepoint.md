# GC 调参与自动回收 / safepoint 协议

> 对齐：2026-09-11（按 change 倒序）：
> `fix-futile-backoff-stretches-nursery` 徒劳退避改为只在设了软上限时生效 —— p90 停顿 −42%、
> 峰值 RSS −24%，新增「徒劳退避只在有软上限时生效」一节；
> `add-gc-phase-timing` 新增 `Z42_GC_PHASES`（把一次停顿拆成各阶段的耗时），新增「诊断旋钮」一节；
> `fix-primitives-count-as-young` 修「基元被当成年轻对象」—— 脏卡 33 001 → 1–3、中位停顿 −33%，
> 并补上数组 backing 的年龄；
> `perf-bucket-all-blocks-by-chunk` 把变长区 chunk 回收从 O(堆) 降到 O(被回收 chunk 数)，
> 中位停顿 −17%；
> `flip-gc-default-to-generational` 翻默认 + 修软上限在分代下不被执行 + 修「一个 pause 跑两次
> 回收」/「major 不升龄」+ 徒劳退避分级 + gate stage 改跑两种模式；
> `fix-callee-entry-safepoint-drops-args` 补「safepoint 只能放在活值已经是根的位置」一节；
> `fix-gc-budget-not-enforced` 修增长闸门基线 + 退避策略一节；
> 原 change `add-gc-tuning-config`，落地 runtime_review §M3 GC 调参 + §M6 safepoint 协议。
> 代码：`src/runtime/src/config.rs`（knob）、`gc/arc_heap/auto_collect.rs`（自动回收策略）、
> `gc/trace.rs` + `gc/phase_timer.rs`（两个诊断旋钮）、
> `gc/arc_heap/alloc.rs`（压力事件），
> `gc/safepoint.rs`（协作式 safepoint）、`gc/heap.rs`（`MagrGC` trait 协议文档）、
> `interp/exec_support.rs` + `interp/mod.rs`（被调函数入口 safepoint 的插桩点）。

## 为什么

GC 的「何时自动回收」由几个**比率魔数**决定（near-limit 90%、pressure 75%、throttle 10%）。
过去它们直接硬编码在 `alloc.rs` 的条件里，调优实验必须改代码 + 重编 VM。本页记录：

1. 这些比率**收进 `RuntimeConfig`**，通过 `Z42_GC_*` 环境变量 / `[runtime]` TOML 层可调，代码不动；
2. 自动回收触发后**如何流转**——allocator 只置 flag、真正的回收延迟到 mutator 的下一个
   safepoint 执行（**register / defer / fallback 三态协议**），这是过去只散落在源码注释里、无集中说明的部分；
3. 哪些 GC 常量**刻意不做**成运行时 knob（`PROMOTION_THRESHOLD`），及其判据。

## GC 调参 knob（`Z42_GC_*`）

所有 knob 走统一的 `RuntimeConfig` 解析链（**env > `[runtime]` TOML > 内置默认**，见
[load-context.md](load-context.md) 无关；解析在 `config.rs`）。缺失 / 空 / 越界 → 回落默认；
非法值 → 一行 stderr 警告后回落。

| Knob | 默认 | 语义 | 消费点 |
|------|------|------|--------|
| `Z42_GC_NEAR_LIMIT_RATIO` | 0.90 | heap-used 达 max-bytes 上限的此比率 → 触发自动回收 + 发 `NearHeapLimit` 事件 | `arc_heap/auto_collect.rs`、`arc_heap/alloc.rs` |
| `Z42_GC_PRESSURE_RATIO` | 0.75 | heap-used 落在 `[pressure, near)` 区间 → 发 `AllocationPressure` 事件（应低于 near-limit 比率） | `arc_heap/alloc.rs` |
| `Z42_GC_THROTTLE_RATIO` | 0.10 | ⚠️ **已不参与自动回收的触发**（arm-gc-by-default 把闸门换成了相对余量）；保留供未来的去抖策略使用 | — |
| `Z42_GC_PROMOTION_AGE` | 2 | **分代专用**：熬过几次 minor 才晋升到老年代；范围 1–3（年龄只有两位）。**建堆时读一次**，写屏障读的是字段 | `gc/mod.rs` |
| `Z42_GC_LOH_BYTES` | 64K | 变长块走 dedicated chunk 的尺寸门槛（死后内存直接还给分配器）；上界 = 64K bump chunk。**进程级** | `var_region/chunk.rs` |
| `Z42_GC_NURSERY_BYTES` | 32M | **整套策略的计量单位**：自上次回收以来分配这么多就触发 minor（分代）；×4 是 major 余量的下界（两种模式）。买停顿上界的那个旋钮 | `arc_heap/auto_collect` |
| `Z42_GC_MAX_BYTES` | **unset = 无上限** | **软上限，不再是武装开关**（arm-gc-by-default）：设了只压回收余量并加一个近上限触发 | `arc_heap/auto_collect` |
| `Z42_GC_MINOR_THRESHOLD` | 0.75 | minor GC 后年轻代存活比率高于此 → 下次回收立即升级 major | `arc_heap` |
| `Z42_GC_SOFT_THRESHOLD` | 0.80 | 堆压力比率高于此 → `SoftHandle` 弱引用变为可回收 | `gc/soft_registry.rs` |
| `Z42_GC_PAUSE_WINDOW` | 1024 | per-heap 滚动 pause-time 队列容量（entries），clamp 到 `[1, 65536]` | `gc/types.rs` |
| `Z42_SAFEPOINT_THROTTLE` | 1024 | 每线程 safepoint 快路径计数；每 N 次才走真 Mutex 轮询。`1` = 禁节流 | `gc/safepoint.rs` |
| `Z42_GC_MODE` | **`generational-mark-sweep`** | GC 算法：`stw` / `concurrent` / `generational`。默认自 2026-09-10 由 `stw` 改为 `generational`（见下「为什么分代成了默认」） | `gc/mode.rs` |

## 诊断旋钮（`Z42_GC_TRACE` / `Z42_GC_PHASES`）

调参旋钮改的是行为，这两个只**看**行为，默认全关、关掉零成本。

| Knob | 语义 | 消费点 |
|------|------|--------|
| `Z42_GC_TRACE` | 每次回收一行：种类、堆 used 前后、回收字节、停顿 ms、第几个周期；外加近上限 / 超预算两条边沿。关掉时连 observer 都不装 | `gc/trace.rs` |
| `Z42_GC_PHASES` | 把那一行停顿**拆开**：每个阶段一行耗时 + 处理条目数，外加一行「这次回收是被哪个闸门触发的」 | `gc/phase_timer.rs` |

`Z42_GC_PHASES=1` 的一次 minor 长这样（`z42c.semantics --release --no-incremental`）：

```text
z42-gc:   trip minor  gate 32.0M x4  grown 160.0M  (last freed 84.2M)
z42-gc:   minor mark                    10.912 ms  (290373)
z42-gc:   minor/scan objects             3.538 ms  (350051)
z42-gc:   minor/promote objects          3.536 ms  (166724)
z42-gc:   minor/tomb objects             8.526 ms  (183327)
z42-gc:   minor/scan arrays              9.463 ms  (624228)
z42-gc:   minor/promote arrays           3.228 ms  (71422)
z42-gc:   minor/tomb arrays              7.051 ms  (552806)
z42-gc:   minor/var sweep               11.772 ms  (803767)
z42-gc:   minor/chunk reclaim            6.373 ms
z42-gc:   minor sweep                   53.625 ms
z42-gc: Cycle used 261.6M -> 115.4M  freed 146.2M  pause 64.6ms  (cycle 8)
```

major 打的是另一组名字：`reset marks` / `full mark` / `sweep` 的四个半程 +
`sweep/var` + `sweep/chunk reclaim` / `age survivors`。

**怎么读这些行**——耗时单独看没有意义，要看它和**条目数的比值**：

- `full mark 25.6 ms (953988)` 是正常的；`full mark 12 ms (84)` 是缺陷——标记只找到 84 个对象
  却走了 12 ms，说明根集合里塞满了不该在那儿的东西（这正是 `fix-primitives-count-as-young`
  的形状：推进标记队列的 125 万个值里 99.996% 是基元）。
- `trip` 行说的不是「花在哪」而是「**为什么是现在**」，它决定了后面所有阶段要啃多大一片年轻代。
  `gate 32.0M x4  grown 160.0M` 里的 `x4` 是徒劳退避的倍数（见下「徒劳退避只在有软上限时生效」）。

这套打点此前是「用时手打、量完删掉」的临时补丁，进出四次（#565 / #566 / #569 / #570 的定位
全靠它）。固定下来是因为**它每次都是定位的第一步**，而重打一遍的成本远高于让它常驻——
常驻的代价只有「关掉时每阶段一个 `Option` 判断」。

## 徒劳退避只在有软上限时生效

自动回收的增长闸门上挂着一个**徒劳退避**倍数：连续几次「几乎没回收到东西」的回收，会把下次
触发所需的增长量翻倍（上限 `MAX_BACKOFF = 64`），一次有效回收清零。它是为一种真实病理加的
——活集合本身就超过了预算，于是每次回收都回收不到东西、堆却还在长，只看增长的闸门会永远
重新武装（实测 `09_alloc_ctorless` 配 64MB 预算：0.29 s 的程序跑 9 分钟没跑完，每 ~6 MB 做
一次 0 字节的 75 ms mark-sweep）。

**但它只在设了软上限（`Z42_GC_MAX_BYTES`）时才该生效**（`fix-futile-backoff-stretches-nursery`,
2026-09-11）。没有软上限时余量是 `live × 0.33`，**随活集合一起长**，回收次数对堆增长已经是对数
的，没有失控可拦；而这时倍数唯一够得着的闸门是 **nursery** ——

> nursery 不是内存闸门，它是「一次 minor 要啃多大一片年轻代」的上界，也就是**停顿上界**。

把它乘大，直接就是把停顿乘大。实测 `z42c.semantics --release --no-incremental`：两次分别只
回收了 8.4 MB / 10.1 MB 的回收把倍数推到 4，于是紧随其后的两次 minor 扫了 96 MB / 160 MB 的
年轻代、停了 **45.4 ms / 64.6 ms**，占整次构建停顿的 39%；同一次跑里闸门正常的 minor 只要
6–22 ms。

而且那两次回收**本来就不算徒劳**：它们发生在编译器正把大部分分配物留在手里的阶段，「回收量
不到半个闸门」正是**高存活率**的样子——对高存活率的回应是「下次多扫四倍」，方向恰好相反。
「minor 不管用了」本来就有专门的机制：`minor_escalation_threshold` 会**升级成 major**，而不是
把 nursery 养大。

| | 墙钟 | 峰值 RSS | 回收次数 | 停顿合计 | 中位 | p90 | 最大 |
|---|---|---|---|---|---|---|---|
| 退避撑大闸门 | 6.79 s | 767 MB | 11 | 266 ms | 20.7 ms | 45.8 ms | 60.8 ms |
| **只在有上限时退避** | 6.82 s | **583 MB** | 17 | 275 ms | **16.0 ms** | **26.4 ms** | **44.1 ms** |

p90 −42%、最大 −27%、峰值 RSS −24%，代价是墙钟 +0.4% 与停顿合计 +3.4%（多了 6 次回收的固定
开销）。RSS 一并降下来是因为被撑大的闸门同时也让堆 used 冲到了 274 MB。

> 三个比率各自独立 clamp 到 `[0,1]`，**不强制跨 knob 排序**（若把 pressure 设得高于 near，
> pressure-事件分支自然变死代码，无害）——保持每个 knob 独立可预测，不做"惊喜"式静默改写。

比率的三处消费点：
- `maybe_auto_collect`（`auto_collect.rs`）：near-limit（触发）+ throttle（去抖）；
- `check_pressure`（`alloc.rs`）：near-limit（发 `NearHeapLimit`）+ pressure（发 `AllocationPressure`）；
- `maybe_reset_near_limit_warned`（`alloc.rs`）：near-limit（回收后 used 降到阈值下 → 复位事件闩，使下次跨阈值能再发）。

三处共用 `runtime_config().gc_near_limit_ratio` 同一比率——保证「发事件」与「复位事件闩」用同一阈值、不错位。

## 增长闸门的基线：必须是「上次回收**结束**时」，不是「上次触发时」

throttle 去抖比 near-limit 触发更容易写错，且错了不会报错、只会**静默不回收**。

触发要同时满足两条：① `used ≥ near_limit_ratio × budget`；② `used` 距上次至少再涨
`throttle_ratio × budget`。**②的基线取哪个时刻，决定了这个预算是不是真预算**：

- ❌ 取**上次 trip 时**的 `used`——那是回收**前**的高水位，而条件 ① 恰好把它钉在
  `near_limit_ratio × budget`。于是再次触发要求
  `used ≥ (near_limit_ratio + throttle_ratio) × budget` = **整个预算**起步，且每回收一次，
  触发点就再自涨一格 `throttle_ratio × budget`。一个能干活的回收器把堆压在预算以下，
  于是第二次回收**永远不来**；真涨上去的场景里，触发点也在无限棘轮，`used` 随之无界增长。
- ✅ 取**上次回收结束时**的 `used`——闸门问的是「上次收完之后又新分配了多少」，触发点稳定
  停在 `near_limit_ratio × budget`，预算才真正封顶。

这个基线不需要往各条回收路径里插钩子：上次 trip 记下的 `used` 减去那次回收 `reclaimed` 的
字节数，就是它收完时的水位（`stats.reclaimed_bytes` 的增量，每条回收路径本就在维护）。

> 实测（`z42c.semantics --release --no-incremental`，256MB 预算）：基线取 trip 时，全程只收
> **一次**——cycle 1 在 230.4MB 处释放 186.3MB，闸门随即要 295.3MB（110% 预算，倍数已被下面
> 的退避翻倍过一次），而这次编译分配 450MB、退出时 `used` 停在 273.6MB，够不着。改成收完时
> 的水位后，同一次编译收 2 次，`used` 全程被压在 230MB 附近。

## 徒劳回收的退避

去抖只看**增长**，这在存活集真的超过预算时不够用：每次回收都收不出东西，堆照涨，增长闸门
无限复位。实测 `src/tests/perf/scenarios/09_alloc_ctorless`（150 万对象全存活）配 64MB 预算，
每涨 6MB 就来一次 75ms 的零收益 mark-sweep，0.29s 的程序 9 分钟没跑完。

所以每次**无产出**的回收把闸门要求的增长量翻倍（上限 `MAX_BACKOFF = 64`），一次有产出的回收
复位为 1。「无产出」= 上次回收释放的字节数不足一个增长闸门。软预算于是表现得像软预算：
尽量守住，一旦证明守不住就别再烧 CPU。

⚠️ 产出率读的永远是**上一次 trip 所要求的那次回收**的战果，因此第一次 trip 没有可判之物——
把那里读到的 0 当作「徒劳」，会让退避倍数在第一次回收发生之前就已经是 2。`gc_cycles == 0`
判为中性，不是徒劳。

## 自动回收 / safepoint 三态协议

allocator 判定「该回收了」后**不在分配线程就地回收**（那会让 scanner 与 mutator 的活寄存器读写竞争），
而是走一个由 `MagrGC::set_external_needs_collect_flag` 注册的 `Arc<AtomicBool>` flag。三态：

```text
① Register  VmCore::new 构造后调 set_external_needs_collect_flag(flag)
            把与所有 VmContext 共享的 flag 交给 heap。
            （mock heap / 无跨线程需求的 backing 保持默认 no-op → 永远停在 ③）

② Defer     alloc 时 maybe_auto_collect 判定触发（near-limit ∧ throttle）：
            仅 flag.store(true, Release) 后返回，不在本线程回收。
            ↓ 下一次任意 mutator 的 check_safepoint（函数入口 / 回边 / Call 返回）
            slow-path 用 swap(false, AcqRel) 抢占本轮（首个抢到者赢，其余跳过）
            → request_gc_pause 下做 stop-the-world 回收（scanner 不与 mutator 竞争）

③ Fallback  flag 未注册 → maybe_auto_collect 直接 collect_cycles() 就地回收。
            保留 GC 单测（直接 ArcMagrGC::new() 无 VmCore）的单线程行为。
```

**谁检查 / 何时**：flag 在**分配线程**、alloc 时**置位**；在**mutator 线程**、其节流后的 safepoint 轮询时
**检查并清除**。置位的 flag **从不阻塞 allocator**——回收延迟由 safepoint 节流上界（`Z42_SAFEPOINT_THROTTLE`
× per-iter 成本，默认 ≈50µs）决定，而非分配。该三态协议现集中文档在 `MagrGC::set_external_needs_collect_flag`
的 doc（`gc/heap.rs`），不再散落。

safepoint 本身的相位状态机（`Idle → Requested → Marking`，concurrent 模式多一个 `ConcurrentMarking`）
见 `gc/safepoint.rs` 顶注 + `GcPhase` 文档。

### ⚠️ safepoint 只能放在「活值已经是根」的位置

三态协议保证的是**回收不在分配线程就地发生**，它**不**保证回收发生时每个活值都被根覆盖。
后者是插桩点自己的责任，而且是一条独立的、更容易被违反的不变量：

> **一个 z42 值在跨越 safepoint 时，必须至少被一个 GC 根引用。**
> 只被 Rust 局部（`Vec<Value>` 临时、`&[Value]` 形参、返回值在途）持有的值**不是根**。

`fix-callee-entry-safepoint-drops-args`（2026-09-10）就栽在这条上。被调函数入口的
`check_safepoint` 原本在四个 `exec_function*` 入口里，**都在帧建立之前**：

```text
exec_function(ctx, module, func, args)
    check_safepoint(ctx)          ← 回收发生在这里
    Frame::new(args, …)           ← 参数此刻才被 clone 进 callee 寄存器
    exec_function_body(…)
        push_frame(&frame.regs)   ← 寄存器此刻才成为 GC 根
```

`args` 是**调用方的临时切片**。对 `new T(..)` 而言，那个临时里的 `args[0]` 就是刚分配出来的
接收者——它还没被写进任何寄存器、任何字段，**堆里没有第二条引用**。于是那次回收把它扫掉，
构造函数随后往一个已经死掉的对象上写字段，或者把一个已经死掉的实参写进字段：

```text
z42-gc-probe: STORING A DEAD VALUE at FieldSet: owner=Object[Z42.Syntax.Token]
              slot=2 value=Object[Z42.Core.Span]
  at Z42.Syntax.Token.Token(Token,int,string,Span)
  at Z42.Syntax.Lexer._emit(Lexer,int,string)
```

对外表现是编译器在几百次回收之后崩在一个悬垂引用上（`FieldGet … got Null` /
`__str_hash_code: arg 0 expected string, got Null`）——**和 #537 / #539 一模一样的症状，
但根因完全不在分代那一侧**：STW 模式压小预算（`Z42_GC_MAX_BYTES=8M`）同样复现。

修法：把这一次 check 移进 `exec_function_body`，**紧接在 `push_frame` 之后**。同时
`resolve_function_tokens` 也挪到了 push 之后——它会排空静态初始化器队列、执行 z42 代码，
因此本身能到达一个 safepoint。`Frame::new` 到 push 之间就只剩 `ref`/`out` 的 copy-in，
它只读不分配，够不到 safepoint。

🔑 **两条经验**：
1. **判「谁是根」要看代码，不要看直觉。** 参数「显然活着」——它在调用方的表达式里刚算出来。
   但调用方的**寄存器**才是根，临时 `Vec` 不是；`new` 的接收者甚至连调用方寄存器都还没进。
2. **这不是分代缺陷，是采集频率把它照出来了。** 32M 默认一次构建才 26 次回收，撞不上这个窗口；
   1M nursery 是 365 次，第 128 次撞上。**「只在小 nursery 下复现」不等于「是分代的锅」。**

## GC 的根集合到底有哪些

三条标记路径（`mark_phase` 全量 / `snapshot_roots_into_mark_queue` 并发 / `mark_phase_minor`
分代）各自组装根集合，**三处必须一起改**：

| 根 | 来源 |
|---|---|
| pinned roots | `RcHeapInner::roots`（`pin_root`） |
| **strong GC handles** | `RcHeapInner::handle_slab` 的 strong 槽 |
| external scanner | `VmContext` 的静态字段 / 调用栈帧 / 三个 arena / 内插字符串缓存等 |
| 脏卡（**仅 minor**） | 老条目里可能指向年轻对象的那些 |

⚠️ **`GCHandle.AllocStrong` 曾经锚不住目标**（`fix-strong-handles-are-not-roots`，2026-09-11）。
`handle_slab` 全仓只有 `arc_heap/interface.rs` 的四个 `handle_*` 方法碰过，**没有任何 mark 阶段
扫它**，于是 Strong 与 Weak 的唯一可观察差别只剩「能不能 `downgrade`」——而 `HandleEntry` 的
文档注释写的正是「strong slots … anchor their target across collection」。
**没有任何测试断言过「强句柄能扛住一次回收」**，所以它活了很久。

🔑 **加根的时候记住 minor 有自己的一套。** 只改全量标记，在默认收集器下等于没改 ——
绝大多数回收是 minor。反过来，修「strong 要锚住」时必须同时有一个 **weak 不许锚**的反向测试，
否则「把每个槽都当根」也能让正向测试变绿，却静默毁掉 `AllocWeak`。

## 分代 minor 的标记不变量：**minor 不给老对象留标记**

`mark_phase_minor` 的根 = 固定根 + external scanner + **脏卡里的每一条**。脏卡的根天然是
**老**对象（写屏障只在 owner 是老、被写值是年轻时才置脏），而 minor 从不清扫老对象。
于是有一条不变量：

> **一次 minor 结束时，堆里不应留下任何被置位的 mark。**
> 年轻的幸存者由 `sweep_phase_young_only` / `VarRegion::sweep_young` 清位；
> 老对象**根本不该被置位** —— minor 不清扫它们，标记它们买不到任何东西。

违反它的后果不是「多留点浮动垃圾」，而是**把还被引用的对象扫掉**：

```
minor N    : 老 owner 作为脏卡根出队 → mark_if_unmarked 置位 → 追踪它的孩子 ✓
（sweep_phase_young_only 只清年轻幸存者的位；老 owner 的位留着）
minor N+1  : 同一个脏卡再次把它入队 → mark_if_unmarked 撞见旧位 → 返回 false
             → 循环 `continue` → **它的孩子一个都没被追踪**
             → 只经由它可达的年轻对象全部未标记 → 当场清掉
```

`Z42_GC_MODE=generational` 因此在**第二次 minor 之后**就开始丢对象，编 `z42c.semantics`
挂在 `__str_hash_code: arg 0 expected string, got Null`（一个老的 `StrMap` 桶数组持有的
年轻 `Str` 被提前回收）。预算越小 minor 越多，128M 必炸、256M 因为只跑得到 2 次 minor 反而侥幸通过。

修法是**老对象直接穿透**：出队时先看年龄，老的不 mark、直接 `trace_children`。终止性仍然成立
—— 老的**孩子**从来不入队（只有 `gen_age < PROMOTION_THRESHOLD` 的才入），所以老对象只可能来自
有限的根集合；同一个老对象在根集合里出现两次的代价是 O(它的字段数)，不是 O(它的子图)。

⚠️ 同一族的第三次了：[gc-tlab-chunk-exclusive.md](gc-tlab-chunk-exclusive.md) 记的
**陈旧 mark 位导致的 use-after-free**（闭包的 `env`）是第二次。
**「mark 位活过了它那一轮回收」是这套 GC 的惯犯 —— 任何新增的「置位但不由本轮清扫负责清位」的
路径，先问它谁来清。**

顺带补上了 `reset_all_marks_in_regions` 漏掉的变长区：它和另外两个 region 一样带 mark 位
（`mark_backing` / `shade_var_newborn` 置位），少这一行就意味着一次中途放弃或换模式的回收
留下的位会让下一次 `mark_phase` 跳过某个闭包的 `env`。

## ⚠️ GC 模式的 CI 覆盖（gate stage `gc modes`）

`Z42_GC_MODE=generational` **曾经从来没有被 CI 跑过**（`grep Z42_GC_MODE` 全仓只有一个
concurrent 的 smoke）——这是 #537 / #539 **三个「丢对象」缺陷能活几个月**的直接原因。
gate 里因此有一个 stage 用高频回收重编 `z42c.semantics`。

**翻默认之后它跑两条腿**：分代成了默认，于是 `stw` 掉进了分代原来那个「没人覆盖」的位置 ——
同一个坑不能踩第二次。

| 腿 | 设置 | 回收次数 | 它防的是 |
|---|---|---|---|
| 默认模式 | `Z42_GC_NURSERY_BYTES=1M`（**`Z42_GC_MODE` 不设**） | ~440 | #557 那类过早回收（需要 ~128 次才显形） |
| STW | `Z42_GC_MODE=stw`、`Z42_GC_MAX_BYTES=32M` | ~67 | 非默认模式的同类缺陷 |

🔑 **默认那条腿故意不设 `Z42_GC_MODE`** —— 它测的就是「默认是什么」；写死模式会让一次
默认翻转悄悄溜过去。**两条腿都断言了一个回收次数下界**：默认被改、或者哪次策略调整让
收集器不再触发，次数会塌掉、stage 变红，而不是继续绿着却什么都没测。

**为什么是编译器自建、不是 golden 套件**：golden 版先做过，**是空转的** ——
把 #537 的缺陷放回去，整套 golden 依然全绿（每个 golden 都是短命小程序，一次 minor 都不做）。
三个缺陷当年都是以「编 z42c.semantics 编到一半崩在悬垂引用上」的形式暴露的，所以就跑那件事。

**为什么默认腿的 nursery 定 1M**：nursery 决定跑多少次回收，而这些缺陷都要**对象熬过好几次
回收**才显形。一次 `z42c.semantics` 构建：

| nursery | 回收次数 | 放回 #539 的缺陷 | 放回 callee-entry safepoint 缺陷 | 干净树 | 墙钟 |
|---|---|---|---|---|---|
| 32M（默认） | 26 | 绿（**抓不到**） | 绿（**抓不到**） | 绿 | 6.8 s |
| 16M | 26 | 红 | 绿（**抓不到**，要 ~128 次） | 绿 | 6.8 s |
| **1M** | **365** | **红** | **红**（第 128 次） | **绿** ← 选它 | 13.4 s |

16M 是上一版的取值，定在那里是因为当时 1M 在干净树上就是红的 —— 那正是
`fix-callee-entry-safepoint-drops-args`（见上「safepoint 只能放在活值已经是根的位置」），
已修；这个 stage 就是防它回归的那道门。多花的 6.6 s 换 14× 的回收次数。

⚠️ **那个缺陷曾被误记为「1M nursery 下分代仍会丢对象」。它跟分代无关** ——
STW 模式压到 `Z42_GC_MAX_BYTES=8M`（199 次回收）同样丢对象。定位它的关键一步是一个
A/B：让 minor 的标记**穿透老对象**（不靠卡表），结果与对照组**逐字节相同** ——
卡表是清白的，问题在根集合。

### 顺带修掉的三处数组写屏障

任何把**堆引用**写进数组元素的路径都必须发写屏障。解释器的 `ArraySet` 与 JIT 的数组存储
helper 一直都发，这三处从来不发：

| 路径 | 说明 |
|---|---|
| `Array.SetValue` | 反射 / `Std.Array` 的单元素写 |
| **`Array.Copy`** | 最尖锐的一处：`perf-bulk-array-copy` 把脚本侧一个 `ArraySet` 循环换成 bulk 原语，而**那个循环每次迭代都发屏障**，bulk 版一次都不发 |
| 经 `ref` 写数组元素 | `RefKind::Array` 的写回 |

屏障的卡键在**数组头自己的条目**上、与元素下标无关，所以 bulk copy 只要有一个堆引用元素
就够了；纯基元的拷贝**不置脏**（有测试盯着——屏障要精确，不能退化成「这个数组被写过」）。

## 自动回收的触发：**每一个阈值都是相对量**（照 Mono SGen）

```
   分配 ──► used ≥ next_collect_at ？ ──否──► 什么都不做（一次 relaxed load）
                    │是
              decide_trip()
    ┌───────────────┴────────────────┐
    │ 分代                            │ STW（只有一代）
    │ promoted ≥ allowance → major    │ used − live ≥ allowance → major
    │ 否则 grown ≥ nursery → minor    │
    └────────────────────────────────┘

   allowance(live) = MAX(live × 0.33, nursery × 4)，再被软上限压一道
```

**没有任何一处需要「一个字节预算先存在」** —— 这就是 GC 能默认武装的前提。
`Z42_GC_MAX_BYTES` 因此从「武装开关」降级成**软上限**：不设 = 无上限
（和 Mono 的 `soft_heap_limit` 一样），设了只压 allowance 并额外给一个近上限触发。

两个比例直接取自 Mono SGen（`mono/sgen/sgen-conf.h`）：
`SGEN_DEFAULT_ALLOWANCE_HEAP_SIZE_RATIO = 0.33`（让老年代吃进上次全量回收后活集的
三分之一再扫一次）、`SGEN_DEFAULT_ALLOWANCE_NURSERY_SIZE_RATIO = 4.0`（下界，
否则活集极小的程序会不停回收 —— 「几乎没有」的三分之一还是几乎没有）。

⚠️ **为什么不是「按机器内存比例定一个默认预算」**：相对阈值自适应 —— 10 MB 的脚本和
4 GB 的服务共用同一套参数，而按机器内存的启发式在容器里还会读错。

### `next_collect_at`：默认武装的可负担性全在这里

`maybe_auto_collect` 要读 `inner` 里的水位线，也就是**要拿堆的互斥锁**。默认武装
= 每次分配都拿一次锁，那是 VM 里最热的路径。在这之前，唯一挡住它的就是
「没设预算 ⇒ 永不回收」那条早退。

改法是 Mono 的 `major_collection_trigger_size`：把**下一次该被询问的 `used` 读数**缓存
进一个原子。分配路径于是只剩**一次 relaxed load + 一次比较**，慢路径每个闸门至多进一次。

维护点两处，缺一不可：`maybe_auto_collect` 的**每一条出口**（包括「这次不收」——
否则下一次分配又走进来拿锁），以及 `sub_used_bytes`（四条回收路径唯一的公共汇合点）。
⚠️ **后者必须无锁**：其中三处调用时**正持着 `inner.lock()`**，`parking_lot::Mutex` 不可重入。

### nursery 是整套策略的计量单位

它同时是 minor 的闸门和 major allowance 的下界（×4）。Mono 取 4 MB，因为**它的 minor
是 O(young) 的**；z42 的 minor 还要做一遍 **O(堆)** 的 chunk 回收，所以频繁 minor 在这里
贵得多，默认取 **32M**。

实测（`z42c.semantics --release --no-incremental`，同一 seed，各 3 跑取中位）：

| 配置 | 周期 | 墙钟 | 峰值 RSS |
|---|---|---|---|
| 旧默认（未武装） | 0 | 6.88 s | 1027.4 MB |
| **新默认（无任何 env）** | 4 major | **7.07 s (+2.8%)** | **756.2 MB (−26.4%)** |
| 旧策略 + `MAX_BYTES=128M` | 14 major | 7.90 s | 614.7 MB |
| **新策略 + `MAX_BYTES=128M`** | 12 major | **7.26 s (−8.1%)** | **593.7 MB (−3.4%)** |
| 新默认 + generational | 10 minor / 1 major | 7.34 s | 758.5 MB |

**+2.8% 的墙钟换 −26.4% 的 RSS** —— 而在这之前，不设 `Z42_GC_MAX_BYTES` 的程序
**一次都不回收**。设了软上限时相对策略也更好（墙钟 −8.1%）：allowance 随活集自适应，
不像固定的 `throttle_ratio × limit` 那样在活集变大后仍按同一格触发。

## 为什么分代成了默认（flip-gc-default-to-generational，2026-09-10）

**`Z42_GC_MODE` 的默认已从 `stw` 改为 `generational`。** 这条曾长期不成立，而且原因不是保守 ——
分代当时**两头都输**（7.34 s / 758.5 MB vs STW 的 7.07 s / 756.2 MB）。翻过来靠的是四件事：

| # | change | 它解掉的那一环 |
|---|---|---|
| #552 | 增量 chunk 回收 | minor 的 `O(堆)` 那一趟从停顿里拿掉（中位 −57%） |
| #553 | 一 chunk 32 张卡 + 扫过即清 | 脏卡根集缩 32×（分代中位再 −29%） |
| #555 | CI 补分代覆盖 | 堵上「三个丢对象缺陷活了几个月」的那个洞 |
| #557 | 被调函数入口 safepoint | 唯一剩下的丢对象缺陷 —— **而且它根本不是分代的** |

**实测**（`z42c.semantics --release --no-incremental`，各 3 跑，同一台机同一棵树）：

| | 墙钟 | 峰值 RSS | 回收次数 | 中位停顿 | 最大停顿 |
|---|---|---|---|---|---|
| `stw`（旧默认） | 6.52 s | 949 MB | 3 | 59.4 ms | 84.8 ms |
| **`generational`（新默认）** | 6.74 s（**+3.4%**） | **777 MB（−18.1%）** | 16 | **20.5 ms（−65%）** | 85.5 ms |

**内存和中位停顿两个都赢**，代价是 3.4% 墙钟 —— 多做了 5 倍的回收。最大停顿不动：它由
major 决定，两种模式都要跑 major。

🔑 **这张表和上面那段「两头都不占优」的差别几乎全在 RSS 一栏**，而 RSS 那栏变好不是分代变强了，
是 STW 变差了：`arm-gc-by-default` 之后不设预算的 STW 只跑 3 次 full collection（相对余量
`live × 0.33` 随活集一起长），而分代的 minor 闸门是**绝对**的一个 nursery，所以它按分配量勤跑。
**「谁更省内存」在武装策略换成相对余量之后才倒过来 —— 别拿更早的数据推论。**

⚠️ **`Z42_GC_MAX_BYTES` 与分代闸门的交互**（同一个 change 里修的）：分代的 minor 闸门是
nursery（默认 32M 绝对值，刻意与预算无关），于是一个远小于 nursery 的软上限**根本不会被执行**
—— `next_collect_at` 停在 `live + 32M`，策略在堆冲过上限之前一次都没被问过。
（`decide_trip` 本来会因为 `near_cap` 要一次 major，只是没人问它。）修法是把 minor 闸门改成
`min(nursery, allowance)`，而 allowance 正是软上限压的那个量；没设上限时 allowance ≥ 4 个
nursery，`min` 恒等于 nursery，**默认路径逐字节不变**。
STW 那侧的闸门本来就是 allowance，所以这个洞在它当默认的时候看不见。

### chunk 回收：`all_blocks` 按 chunk 分桶

`reclaim_dead_var_chunks` 要把被回收 chunk 的块从三张表里摘掉。对 `all_blocks` 原本是
`retain`，**每个元素解引用一次 header 读 `chunk_idx`** —— 一次随机访存。实测：为摘掉
~600 个 chunk 扫了 **187 万**个块，**8.1 ms / 9.5 ms 的 minor 停顿**（obj / arr 两个定长区
各只要 0.22 ms —— 成本全在变长区这一趟）。

分桶（`all_blocks[ci]` = 第 ci 个 chunk 的块）之后这一步是 `all_blocks[ci] = Vec::new()`，
O(被回收的 chunk 数)。遍历的元素总数不变、局部性反而更好（同 chunk 的块地址连续）。
中位停顿 22.1 → **18.3 ms**，最大停顿 −9%，RSS 中性。

⚠️ **分桶会以两种方式把 RSS 吃回去，两个都得堵**（否则净亏）：

| 坑 | 代价 | 修法 |
|---|---|---|
| `Vec` 的翻倍空闲容量 **×每个 chunk 一份** | +20 MB | chunk 填满不再增长时 `shrink_to_fit()`（`retire_chunk` / `bump()` 换 chunk） |
| **`clear()` 保留容量** | +18 MB | `= Vec::new()` —— 每次 minor 回收 ~600 个 chunk，各握 2 KB 不放会永久累积 |

只修第一个，RSS 从 811.8 **只降到 809.8 MB**；主因是第二个。

**还没做**：`purge_blocks` 现在只剩 `free_lists` 要扫（~122 万条目 ≈ 3.2 ms）。它按 size class
组织、不按 chunk 分区，要么给每条目内联 chunk 索引（+4 B/条 ≈ 5 MB），要么改成 pop 时惰性校验。
### ⚠️ 「年轻」的判据必须先问「它是不是一个引用」

`gen_age_of` 对**一切非 GC 引用**答 `0`（`Null`、`I64`、栈句柄——它们没有年龄可言），
而 `0 < PROMOTION_THRESHOLD`。于是三处判据全部失真：

```rust
if Self::gen_age_of(child) < threshold { … }   // Value::Null 也满足！
```

后果按严重程度排：

1. **`refers_to_young` 恒为真** —— 它是 `dirty_cards_for_newly_old_*`（晋升时）和
   `rebuild_card_table`（major 后）共同的判据。只要条目有**一个空槽或一个基元字段**
   就算「指着年轻的东西」⇒ **每张卡都被置脏、而且再也清不掉**
   （#553 的「扫过即清」救不了：扫的时候它照样报告「有年轻的」）。
   实测稳态：**33 001 张脏卡、每次 minor 重扫 203 884 条目，只为找到约 100 个年轻对象。**
2. **标记队列被基元淹没** —— 每次 minor 推入 1 254 097 个值，其中 **1 254 047 个是基元/Null**
   （99.996%），全部被 `mark_if_unmarked` 原样拒绝。

修法就是先问 `is_heap_ref()` ——**写屏障 `debug_assert` 用的就是这个谓词**，两处口径本该一致。
实测（`z42c.semantics`）：脏卡 33 001 → **1–3**，minor mark 12.0 ms → **0.1 ms**，
中位停顿 21.5 → **14.4 ms**，RSS −3.4%，**`freed` 逐周期逐字节相同**。

#### 🔑 去掉一个「几乎总是真」的判据，会暴露所有搭它便车的缺陷

**数组的元素存储块（`region_var` 里的 backing）只靠 `mark_backing()` 活着，而那是
「trace 这个数组」的副作用。** 它不是数组的任何一个 `Value` 孩子，所以任何遍历孩子的检查
都看不见它年不年轻；而 minor 只在卡脏时才 trace 一个老数组。

「卡永远脏」以前意外保证了每个老数组每次 minor 都被 trace。判据改诚实的那一刻，
**young backing 开始在活着的老数组底下被扫掉** —— 症状不是崩溃，而是**自举字节不动点断裂**
（gen1≠gen2，差 167 B），**单测一个都没红**。

⚠️ **这类「把一个近似判据改准」的改动必须连自举不动点一起跑**——单测覆盖不到这种形状。

##### 修法：backing 跟随宿主数组升龄（而不是为它多置一张卡）

两种修法都试过、都量过：

| 修法 | 中位停顿 | RSS | `z42.text.levenshtein` |
|---|---|---|---|
| 让 `refers_to_young` / `seed_card_entry` 也看 backing 年龄 | −33% | −3.4% | **+28%（CI 判红）** |
| **backing 跟随宿主升龄** ← 采用 | **−37%** | +1.6% | +1.6% |

**为什么前者那么贵**：`int[]` / `char[]` 这类基元数组**根本没有 `Value` 孩子**，所以本来就
不置脏卡；加上 backing 检查后它们**开始**置脏卡，还要等 backing 自己熬过 `PROMOTION_AGE`
次 minor 才清得掉。`levenshtein` 每次调用分配 4 个基元数组。

**后者为什么更对**：数组头**独占**它的 backing，两者生命周期完全一致，年龄本就该一致。
在「头被晋升」的那一刻把 backing 抬到同龄，这个洞**从构造上不存在**，不用为它付一张卡。
正确性依据：晋升 ⇒ 本轮被标记过（`iterate_young` 只晋升 marked 的）⇒ 被 trace 过 ⇒
`mark_backing` 已跑过 ⇒ 块是 marked 的，而 `sweep_young` 在同一次 sweep 里**更晚**执行，
会把它升出 young 表而不是回收掉。

**代价**：backing 提前进老年代、只能等 major 回收 ⇒ **RSS +1.6%（+13 MB）**，换 −37% 中位停顿。

### 一个周期跑 minor **或** major，绝不两个都跑

升级启发式（`Z42_GC_MINOR_THRESHOLD`，年轻代存活率高 → 该收老年代了）**曾经在同一个 pause 里
紧接着跑一次完整 major**，源码注释就写着 `Major in same pause window.`。`want_major` 那条
（晋升字节闸门）也一样：想要 major 时仍然先跑一遍 minor。

**那次 minor 是纯白干** —— major 从固定根标记全堆、清扫所有 region，minor 能回收的它全包含。

```text
旧：  minor(全部年轻代)  →  major(全堆)        一个 pause 付两遍
新：  想要 major → 只跑 major
      minor 发现存活率高 → 把 major 排到下一个周期（pending_major）
```

实测 `src/tests/perf/scenarios/09_alloc_ctorless`（存活率 100% 的分配循环，升级**每个周期**都
触发）：每周期 `minor 156.7 ms + major 188.5 ms` = 370 ms 停顿，`freed` 0 字节。

⚠️ **代价是 major 必须自己升龄**（下一节）—— 以前 major 前面永远有一个 minor 替它做。

### major 也要给幸存者升龄

**升龄是唯一能排空 young 表的东西**（幸存者年龄到 `PROMOTION_THRESHOLD` 才离开）。
一旦 major 能单独跑，不升龄就意味着**所有存活条目都留在 young 表里**，下一次 minor
直接重标全堆 —— 实测 1 498 866 条、192.6 ms，本该只有一个 nursery 那么多。

语义上这也更对：**熬过一次 major 和熬过一次 minor 是同样强的长寿证据**。
`age_survivors_after_major` 放在 `rebuild_card_table` **之前**：晋升产生 old→young 边，
而 rebuild 负责记录它们。

### 徒劳退避：「白干一场」比「回收得不够」退得更狠

原来两者都是 ×2。它们性质不同：白干一场说明活集根本不产生垃圾，而下一次回收要多标记整整一个
闸门的对象、回报仍是零 —— **每多退一格，下一次白干就更贵**。所以
`reclaimed < gate/16` → ×4，`< gate/2` → ×2，其余重置为 1。
1/16 远低于健康回收的回报（健康 minor 能收回大半个 nursery），也远高于 100% 存活时还回来的
那几百字节，两种情形不会重叠。

**三条合起来**（本地双二进制 A/B，base = 翻默认前）：`09_alloc_ctorless` 从
**1.93× 回到 0.885×**（比 STW 还快），其余场景全在 ±3.3% 内，而编译器负载的
RSS −18.2% / 中位停顿 −64% 一分没丢。

## 分代的两个闸门：新生代按分配量，老年代按晋升量

分代堆有两个独立的容量，就该有两个独立的闸门：

```
   分配 ──► used - baseline ≥ NURSERY_BYTES ──────────────► minor
   晋升 ──► promoted_since_major ≥ MAX_BYTES ─────────────► major
```

**为什么不能只用一个。** 原来的唯一闸门是 `used ≥ near_limit × MAX_BYTES`，而
`used_bytes` 记的是**活字节**。minor 回收年轻垃圾把活字节压得很低 —— 于是这个闸门在
分代模式下既不是新生代的容量（要等堆整体接近预算才开），也**看不见老年代的增长**
（死掉的老对象不在活字节里）。实测 `z42c.semantics`：`used` 稳定在 90 MB 上下，
而 RSS 到 1 GB。

**晋升字节数是堆里唯一看得见老年代增长的量**（老年代垃圾的上界就是流进去的量），
而且它的维护点在 minor sweep 里 —— 那里本来就在算「这一轮谁跨过了阈值」，
所以**分配路径零成本**。`Z42_GC_NURSERY_BYTES` 不设时取 `MAX_BYTES / 4`：
比例而非绝对值，同一个设置在任何预算下含义相同。

### ⚠️ minor 也必须还 chunk —— RSS 的主修

`reclaim_dead_chunks` / `reclaim_dead_var_chunks` 原本**只在 major 里调**。minor 把条目
tombstone 掉进 free list，但一块 chunk 都不还 —— 而 TLAB 是**整块**发给 mutator 的，
一阵短命对象通常整块死，正是 minor 最该收的形状。

| 128M 预算 | 周期 | 峰值 RSS |
|---|---|---|
| 分代（minor 不还 chunk） | 15 minor / 1 major | 986.3 MB |
| 分代（minor 还 chunk） | 15 minor / 1 major | **607.9 MB** |

同样的回收次数，**−38%**。「分代内存大是因为 major 跑得不够多」这个直觉是错的。

⚠️ **代价**：这个 pass 是 **O(堆)** 而不是 O(young)，是 minor 停顿的大头（中位 ~30 → ~75 ms），
给「nursery 越小停顿越低」压了一个由堆大小决定的地板。要拿到设计里
「p99 随 nursery 容量线性变化」，得先把它做成增量的（只看这一轮碰过的 chunk）。

### 两处曾经算错的量

- **升级启发式把「晋升」当成了「死亡」**：存活率原本算 `young_after / young_before`，
  而熬过阈值的幸存者**会离开 young 表** —— 阈值是 2，于是幸存者大多被算成死了，
  存活率永远低于 `Z42_GC_MINOR_THRESHOLD`，**升级从没触发过**。
  正确口径是 `1 - 本次 tombstone 数 / 回收前 young 数`。
- **徒劳退避的判据不能跟着增长闸门走**：退避问的是「上一次回收值不值」，固定对着
  `throttle_ratio × limit` 比；增长闸门问的是「再试之前要有多少新分配」。分代下把后者
  换成 nursery（预算四分之一）之后，一次收掉大半个 32M nursery 的**健康** minor 也会被判
  「徒劳」，退避每次翻倍 —— 实测 minor 从 18 次掉到 8 次，堆干脆不收了。

### 实测（`z42c.semantics --release --no-incremental`）

| 配置 | 周期 | minor/major | 停顿中位/最大 | 墙钟 | 峰值 RSS |
|---|---|---|---|---|---|
| 未武装 | 0 | — | — | 8.31 s | 1013.8 MB |
| stw 128M | 14 | 0/14 | 78.7 / 113.5 ms | 7.47 s | 606.8 MB |
| stw 256M | 3 | 0/3 | 131.0 / 138.0 ms | 6.75 s | 763.2 MB |
| **gen 128M** | 15 | 15/1 | **76.7 / 84.1 ms** | 7.33 s | **618.7 MB** |
| gen 256M | 7 | 7/1 | 90.0 / 128.7 ms | 6.96 s | 728.1 MB |

nursery 旋钮（128M 预算）：8M → 27 周期 / 最大停顿 78.5 ms / RSS 609.2 MB；
64M → 8 周期 / 最大停顿 127.8 ms / RSS 679.8 MB。停顿与 RSS 朝相反方向单调走。

## 卡的粒度与寿命：32 张 / chunk，扫过即清（2026-09-10）

`mark_phase_minor` 曾是 minor 里最大的一段（~25 ms）。给它的播种与 BFS 分别计时：

| minor | 固定根 | **脏卡根** | BFS | **最终标记数** |
|---|---|---|---|---|
| 早期 | 2 040 | 27 608 | 9.3 ms | 292 062 |
| **后期** | 2 955 | **518 530** | 20.1 ms | **76** |

**后期的 minor 播了 51.8 万个脏卡根，只为了找到 76 个年轻对象。** 两个原因叠加：

**① 一张卡盖住整个 chunk 的 256 条** —— 一次跨代写就把 256 条全变成根。
修法：`card_dirty` 一直是 `Vec<u32>` 却**只用了 bit 0**，所以 32 张卡 × 每张 8 条
（`CHUNK_SIZE / 32`，整除）**不花一分钱内存**。这是本仓第二次在「已经付过钱的空间」
里找位置（第一次是 #552 把 `chunk_idx` 塞进块头的 `align(8)` padding）——
**先问「现有字段还有空位吗」，往往比加字段快也便宜。**

**② 卡在 major 之前从来不清** —— 而写屏障**和晋升**（#539 的
`dirty_cards_for_newly_old_*`）都在往里加，脏集单调滚到活堆的约 65%。
修法：**扫过即清**。卡的含义是「这里**可能**有跨代边」，不是事实；minor 扫一张卡时
顺便判断它是否还够得到年轻对象，够不到就清掉。安全性来自上一节那三条破口都有人负责：
**写**（写屏障）、**晋升**（`dirty_cards_for_newly_old_*`，与清扫在同一个 STW 窗口里、
在它之后跑）、**major 清卡**（`rebuild_card_table`）。

顺带：播种时**老条目直接穿透**（追踪它、把年轻孩子入队），不再推进 BFS 再出队重来一遍
—— #537 之后 BFS 对老条目本来就只做这件事，挪到播种处省一次 push/pop，
**并且让「这张卡还够不够得到年轻的东西」有地方可判**（BFS 里卡的归属已经丢了）。

**实测**（同 seed）：分代默认 nursery 中位停顿 31.2 → **22.2 ms（−29%）**，RSS 持平；
**STW 一分不动**（57.9 → 57.9 ms）—— 它没有代、不用卡表，这正是改动作用面的证明。

⚠️ **试过并放弃的一版**：把「脏卡里的**年轻**条目」从根里去掉（它按理不是根 ——
那是 chunk 粒度时代的产物）。结果停顿没再多赚，而小 nursery 档 RSS +6%。
对照实验（保留旧播种、只改粒度与清扫）与本版的 **`gc_reclaimed_bytes` 逐字节相同** ——
**回收的是同一批对象**，差别只是标记顺序影响了后续的槽位复用与整块回收，即**分配局部性**。
去掉年轻根是一次**语义**改动（改的是「收什么」），要单独立项、单独拿证据。

## 卡表的不变量：老条目指着年轻的，卡就必须是脏的

分代 minor 的正确性完全压在这一句上：

> **一个存活的老条目，只要还指着任何年轻的东西，它所在 chunk 的卡就必须是脏的。**

因为 minor 的标记只从三种种子出发（固定根 / external scanner / **脏卡里的活条目**），
而老的孩子从不入队。老对象若既不是根、卡又是干净的，它的年轻孩子就没有任何人去标记
—— 当场被扫，而它还被引用着。

这句话有**三个**可能被破坏的时刻，缺一处就丢对象：

| # | 时刻 | 谁负责 |
|---|---|---|
| 1 | 往老对象里**写**一个年轻引用 | 写屏障 `maybe_mark_cross_gen_card` |
| 2 | 一个持有年轻引用的对象**被晋升成老的** | minor sweep 的 `dirty_cards_for_newly_old_*` |
| 3 | major **清空**卡表，而 old→young 边还在 | `rebuild_card_table` |

**破口 2（晋升造边）**：父对象比子对象先分配 → 先老。它跨过 `PROMOTION_THRESHOLD` 的
那一瞬间就成了「老对象持有年轻对象」，而当初那次写是 young→young，屏障正确地什么都没做。
实测形状：老的 `Z42.IR.StrMap`（age 2）持有它后来才扩容出来的桶数组（age 1），
数组被 minor 扫掉，`ContainsKey` 读到 `Null`。
补法 = 晋升时补做屏障的活：**只检查真正跨阈值的条目**，且**只有确实还指着年轻对象的才置脏**
—— 卡是 **chunk 粒度**的（256 条一格），无条件置脏会把下一次 minor 变成全堆扫描。

**破口 3（major 清卡）**：major 扫完全堆后把卡全清。可 major **不做晋升**（只 mark + sweep），
所以扫完之后年轻对象仍然年轻、老对象仍然指着它们 —— 卡却没了。
补法 = 清完之后按存活的图**重建**（一次 major 一遍，换之后 10–20 次 minor 的最小脏集）。
不清也是正确的，但脏集只增不减，minor 会一路退化成全堆扫描。

⚠️ **固定根上的老对象不踩破口 2** —— minor 会「穿透」老根去追踪它的孩子（见上一节）。
只有**图中间**的老对象（既不是根、又只靠卡）才会。**写这类回归测试时 owner 必须藏在一个
root 后面，直接 pin owner 的测试是空转的。**

变长区不需要同样的处理，这是论证过的、不是遗漏：变长块里唯一有出边且自身 `gen_age`
参与追踪判定的是 `Closure`，而 `ClosureData` **创建后不可变**、`env` 数组必然先于闭包块存在
—— 闭包块永远不会比它的 `env` 老，「老父亲 → 年轻儿子」这个方向构造不出来。
`Str` / `ArrayPrim` 是叶子；`ArrayValue` / `ArrayStruct` 走的是数组**头**的年龄，
由上面那两个 helper 覆盖。

## `PROMOTION_THRESHOLD` 怎么变成旋钮的：构造期读取，不是热路径读取

`docs/spec/archive/…/runtime_review §M3` 曾把「晋升阈值 2」列为候选 knob，
**2026-09-05 一度裁定刻意不做**，两条理由都成立：

1. **热路径成本**：它被 `gc/arc_heap/generational.rs::maybe_mark_cross_gen_card`
   —— 即**写屏障** —— 每次堆引用写读取。改成 `runtime_config()` 读取会给每次引用写
   注入一次全局查找。
2. **约 20 处测试**以 `for _ in 0..PROMOTION_THRESHOLD` 的形式把它当编译期常量硬编码。

**2026-09-08（add-promotion-age-knob）按当时就写下的折中落地**：做成**构造期**读取，
而不是 write-barrier 的运行时读。

- `Z42_GC_PROMOTION_AGE` 在**建堆时读一次**（`gc::promotion_age_from_config`），
  值分发给 `ArcMagrGC` / `Region<T>` / `VarRegion` 各存一份 `promotion_age: u8`；
- 写屏障读的是 `self.promotion_age` —— 一个**普通字段**，不是原子、不是全局，
  热路径成本为零；
- `PROMOTION_THRESHOLD` 常量**保留**，语义从「值」变成「默认值」，
  那 20 处测试一行不用改；
- 值**在堆的生命周期内不可变** —— 这正是三份缓存副本能安全存在的原因。

**范围**：`1..=MAX_GEN_AGE`（当前 3）。上界是硬的 —— 年龄打包在
`GcBlockHeader::type_tag` 的两个空闲位里，3 是能表示的最大值。0 会让一切在第一次 minor
就晋升（等于没有年轻代）。越界**警告并 clamp**，不是饱和：一个饱和的年龄永远
「到不了」，于是什么都不会被晋升。

> 原来那句「`PROMOTION_THRESHOLD` 可用 `Z42_GC_TENURE` 配」的注释是**假的** ——
> 那个环境变量在全仓从未存在过。

## 大对象门槛 `Z42_GC_LOH_BYTES`：进程级，因为 TLAB 快路径手里没有堆

变长块的尺寸超过这个门槛就走 **dedicated chunk**（按 payload 精确定尺的独立 malloc），
死后内存**直接还给分配器**（见 [gc-tlab-chunk-exclusive.md](gc-tlab-chunk-exclusive.md)
的 chunk 三种归宿）。门槛降低 → 更多块走这条路 → 更快归还，代价是每块一次 `malloc`。

⚠️ **它是进程级的 `static`，不是 per-heap 字段** —— `class_for` 被
`arc_heap/alloc.rs` 的**无锁 TLAB 快路径**调用，那里既没有 region 也没有堆引用。
一个进程里的多个 VM 共享同一个设置（和它还是 `const` 的时候一样）。
在 VM 构造时 `set_loh_bytes` 一次，热路径上只剩一次 relaxed load ——
**实测代价 +0.023%（78.504 → 78.522 G 指令，三次取中位），落在噪声里**。

**上界硬顶在 `CHUNK_BYTES`（64 KB）**：比一个 bump chunk 还大的块根本没法 bump 分配，
调高只会把块路由到不存在的地方。

## 关联

- [interp-jit-semantics.md](interp-jit-semantics.md)：safepoint check 在 interp / JIT 的插桩点。
- [heap-diagnostics.md](heap-diagnostics.md)：回收后的堆保留诊断。
- `docs/spec/archive/2026-05-20-add-gc-safepoint/design.md`：safepoint 协议原始设计（Decision 5 = JIT 插桩）。
