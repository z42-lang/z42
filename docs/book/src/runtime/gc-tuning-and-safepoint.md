# GC 调参与自动回收 / safepoint 协议

> 对齐：2026-09-07（change `fix-gc-budget-not-enforced` 修增长闸门基线 + 补退避策略一节；
> 原 change `add-gc-tuning-config`，落地 runtime_review §M3 GC 调参 + §M6 safepoint 协议）。
> 代码：`src/runtime/src/config.rs`（knob）、`gc/arc_heap/auto_collect.rs`（自动回收策略）、
> `gc/arc_heap/alloc.rs`（压力事件），
> `gc/safepoint.rs`（协作式 safepoint）、`gc/heap.rs`（`MagrGC` trait 协议文档）。

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
| `Z42_GC_MODE` | `stw-mark-sweep` | GC 算法：`stw` / `concurrent` / `generational` | `gc/mode.rs` |

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

⚠️ **`Z42_GC_MODE` 默认仍是 `stw`**：今天翻到分代，墙钟和内存**两头都不占优**
（7.34 s / 758.5 MB vs 7.07 s / 756.2 MB）。分代该赢在「minor 便宜所以能勤跑」，
而 z42 的 minor 还背着 O(堆) 的 chunk 回收 —— 那也是 nursery 只能停在 32M（而 Mono 是 4M）
的原因。**顺序是：增量 chunk 回收 → 补 CI 的分代覆盖 → 翻默认。**

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
