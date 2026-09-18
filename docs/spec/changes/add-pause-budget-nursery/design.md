# Design: 按停顿预算自适应 nursery

## Architecture

```text
             ┌─ 每次 minor 结束 ────────────────────────────────────┐
             │  pause_us（这次停顿）                                │
             │  consumed_bytes（本次 minor 之前积累的分配量）        │
             └───────────────┬──────────────────────────────────────┘
                             ▼
         cost_ns_per_entry  ← 衰减最大值（升立刻、降 1/16 每次）
         bytes_per_entry    ← EWMA（α = 1/4）
                             │
                             ▼
   budget  = target_us * 1000 / cost_ns_per_entry     ← 一次停顿能扫多少条目
   want    = (budget − survivors) * bytes_per_entry   ← 只有余量能拿去分配
   nursery = clamp(want, cur/2, cur + cur/8), 再 clamp(MIN, MAX)   ← 缩得快、涨得慢
                             │
                             ▼
        auto_collect 的 minor 闸门（`nursery_bytes()`）下次就用它
```

**为什么是「每条目」而不是「每字节」**（实测推翻了初稿的按字节方案）：年轻代表里不只有上一个
nursery 的分配量 —— 一个条目要熬过 `promotion_age` 次 minor 才离表，所以一次 minor 扫的是
「新条目 + 前几次的幸存者」。实测 `13_gc_large_heap` 上一次按 4 MB 增长触发的 minor 仍然标记了
**751 778** 个条目。按字节计量的模型看不见这些幸存者、把它们算作 0，这正是「4 MB 的 nursery
依然打出 36 ms 停顿」的由来。闸门仍按字节触发，所以最后用实测的 `bytes_per_entry` 折回字节。

## Decisions

### D1: 代价模型用「上一次 minor 的停顿 / 消耗字节」，EWMA 平滑

**问题**：单次 minor 的停顿噪声很大（cache 状态、被别的进程抢 CPU、偶发的 chunk reclaim 峰值）。
直接用最后一次的数值会让 nursery 抖动，抖动本身又会让下一次的测量更不可比。

**选项**：
- A. 只用最后一次 → 抖。
- B. 固定窗口平均（如最近 8 次）→ 需要环形缓冲，且对阶段切换（编译器从解析进入代码生成）反应慢。
- C. **EWMA（α = 1/4）** → 一个 u64 字段，对阶段切换 3~4 次 minor 内跟上，噪声被压掉。

**决定**：C。再叠一条「单次变化 ≤ ±50%」的限幅，保证即使某次测量离谱（例如系统被挂起），
nursery 也不会一步跳到极端值。

### D2: 目标是**最大**停顿，所以用观测到的代价上界而不是均值

minor 的停顿里有一部分与 nursery **无关**（chunk reclaim 的 O(chunks)、卡表扫描的固定部分）。
把它们算进「每字节代价」会让模型低估 nursery 的可承受大小；把它们忽略又会让预算被它们吃掉。

**做法一**：代价读数取**衰减最大值**（`max(sample, prev − prev/16)`）而不是均值。要压的是最坏
那次 minor，而均值会被一串便宜的 minor（堆还小、缓存还热）说服去放大 nursery。实测启动阶段
均值读到 13 ns/条、把 nursery 从 16M 涨到 32.9M，等堆长到 300 MB 再读就是 25 ns/条 —— 那一趟的
三次 29~33 ms 停顿就是这么来的。

**做法二**：模型只吃「本次 minor 的停顿」整体，不拆分 —— 于是固定开销自动表现为
「nursery 越小，每条目代价越高」，模型会自己停在「再缩也换不来停顿」的那个点上。
这也是 `MIN_NURSERY` 存在的理由：低于它，固定开销占比过高，缩 nursery 只剩坏处。

> ⚠️ **实测结论：这个地板高于 10 ms，nursery 买不到目标值。** 一次 minor 里与年轻代大小无关的
> 工作，大头只有**一个**：卡表扫描的 O(老年代)（`Z42_GC_PHASES` 的 `card seed` 行；
> `z42c.semantics` 最坏一次扫 **280 382** 个老条目，`13_gc_large_heap --large` 单次峰值 **2 088 956**）。
>
> **（2026-09-18 更正）** 这里原本还列了第二个来源「增量 major 的 grey 队列是 minor 的根」
> （`--large` 上 108 142 个老条目，每次 minor 重新遍历一遍），并把它与卡表并称为「两个大头」。
> **那是错的**：`trim-minor-cycle-roots` 去掉了这部分重复遍历，实测总停顿只动 **−1.4%**，
> 最大停顿在噪声内。因为那批老条目**本来就要被卡表扫一遍**，省的只是第二遍；两笔账的比例是
> **6.8%**（整个 run 606 506 vs 8 859 716 个老条目）。同期记下的「原型 −8%」无法复现，
> 也和算术对不上（天花板约 2%）。
> 目标设 10 ms 时模型停在 `z42c.semantics` **15.1 ms** / 合成负载 ~25 ms；继续缩只是让固定成本
> 乘更多遍 —— 压到 4M 下限时 `--large` 的**总停顿反而涨 224%**。真要 10 ms 得把 minor 本身切片化，
> 是独立 change。

### D3: 与晋升年龄的关系 —— 只报告，不联动

`retune-gc-nursery-and-promotion-age` 已经证明「nursery 和晋升年龄是一对」：
nursery 减半 ⇒ 对象要熬过的分配量减半 ⇒ **过早晋升**，老年代被喂爆（实测 16M+age2 的 RSS 是 32M+age2 的两倍）。

本 change **不**自动联动两者（两个自适应回路互相喂输入是不稳定的经典来源）。
取而代之：
- 默认晋升年龄保持 3（今天的值），它已经是上界；
- `Z42_GC_PHASES` 在 nursery 变化时打一行，把「当前 nursery / 晋升年龄 / 本次晋升字节」一起打出来，
  让「缩 nursery 导致晋升暴涨」这件事在诊断里一眼可见；
- 验收表里**必须**包含 RSS 与晋升字节两列（见 Testing Strategy）。

### D4: 徒劳退避只推迟回收，不放大年轻代

今天 `auto_collect` 的退避乘数直接乘在 minor 闸门上：`gate = minor_gate * backoff`。
`13_gc_large_heap` 上退避到 ×16 时，一次 minor 要扫 272 MB 的年轻代 —— **85 ms 停顿**，
正是本 change 要消灭的东西。

**决定**：退避乘数保留（它防的是「徒劳回收把 CPU 烧光」），封顶做成旋钮
`Z42_GC_BACKOFF_CAP`，闸门取 `min(gate * backoff, nursery)` —— 但**默认关**（User 裁决
2026-09-17，基于下表实测）。效果：开了之后退避仍然让回收变稀疏（因为 `next_collect_at` 推得
更远），但**单次 minor 的规模有硬上界**。

**为什么默认关**（3 轮取中位，同一个二进制开关对比）：封顶买到的停顿是实打实的
（`13_gc_large_heap --large` 退避到 ×64 时一次 minor 被塞了 **1.0 GB** 年轻代、停顿 **301 ms**，
封顶后 25 ms），但它的代价正好落在退避当初被写出来要保护的那类负载上：

| | 最大停顿（不封顶 / 封顶） | 墙钟（不封顶 / 封顶） | 峰值 RSS |
|---|---|---|---|
| `z42c.semantics` | −36% / −36% | −0.8% / +0.4% | ≈ 持平 |
| `09_alloc_ctorless` | −31% / −76% | **−8.5% / +94%** | ≈ 持平 |
| `13_gc_large_heap --large` | −42% / −92% | **+15.7% / +81%** | −4% / −15% |
| `12_gc_churn` | ≈ 持平 | ≈ 持平 | +11% / **+20%** |

这些负载的共同点是「几乎什么都不死」，封顶逼回来的每一次回收都是纯浪费。
**只留自适应 nursery（默认）几乎白送**，所以默认取它。

> ⚠️ 这条与 `add-incremental-major-gc` 的 1.10 条目是同一件事：那次「直接删退避」的尝试让
> `09_alloc_ctorless` 墙钟回归 70~160%（回收次数 3 → 9），所以**不能删退避**，只能给它的年轻代规模封顶。

### D5: 关闭开关与手动挡

- `Z42_GC_PAUSE_TARGET_MS=0` → 自适应关闭，`nursery_bytes()` 回到常量（今天的行为）。
- 显式设置 `Z42_GC_NURSERY_BYTES` → **自适应自动关闭**（手动挡优先），并在 `Z42_GC_TRACE` 里打一行说明，
  免得有人同时设两个然后困惑于「我设的 nursery 怎么没生效」。
- wasm32：`now_us` 在 wasm 上是单调计数器而非微秒（既有实现），代价模型因此在 wasm 上无意义 ⇒
  wasm32 目标下自适应默认关闭（编译期 `cfg`），走固定 nursery。

## Implementation Notes

- `PauseBudget` 是一个纯数据结构 + 纯函数（`observe(pause_us, consumed_bytes) -> next_nursery`），
  单测直接喂序列验证收敛/限幅/关闭，不需要跑 GC。
- 热路径不变：`nursery_bytes()` 仍然是一个 `AtomicU64` 的 relaxed load，自适应只在 minor 结束时写一次。
- 常量初值：`MIN_NURSERY = 2 MB`、`MAX_NURSERY = 64 MB`、`α = 1/4`、单次限幅 ±50%。
- 「消耗字节」取 `used_before_minor - baseline`（`auto_collect` 已经在维护这个水位），不新增计数器。

## Testing Strategy

**单测**（`pause_budget_tests.rs`，纯函数）：
1. 稳态收敛：喂「代价恒定」的序列，nursery 收敛到 `target/cost` 并停住；
2. 限幅：喂一个离谱的测量（停顿 ×100），nursery 单次最多减半；
3. 夹紧：极小/极大代价分别撞到 `MIN` / `MAX`；
4. 关闭：target=0 时 `observe` 不改变 nursery。

**回归测试**：`auto_collect_tests.rs` 加一条 —— 退避 ×16 时 minor 闸门不超过停顿预算 nursery（D4）。

**端到端验收**（两个二进制交错 ×3，与 main 对比）：

初稿的门槛是「全线最大停顿 ≤ 10 ms」。实测证明 nursery 买不到（见 D2 的告示），
**User 裁决 2026-09-17：M3 收在「最大停顿显著下降且吞吐不回归」，10 ms 留给后续的增量 minor**。
下表是默认配置（`Z42_GC_BACKOFF_CAP` 关）的实测门槛与结果，3 轮取中位、两个二进制交错：

| 指标 | 门槛 | 实测 |
|---|---|---|
| `z42c.semantics` 最大停顿 | 显著下降 | **23.5 → 15.1 ms（−36%）** ✅ |
| `z42c.semantics` 墙钟 | ≤ +2% | **−0.8%** ✅ |
| `09_alloc_ctorless` 最大停顿 / 墙钟 | 下降 / ≤ +5% | **−31% / −8.5%** ✅ |
| `13_gc_large_heap` 两档最大停顿 | 显著下降 | **−18~−32% / −42~−46%** ✅ |
| `13_gc_large_heap` 两档墙钟 | ≤ +2% | **+17~+32% / +16~+21%** ⚠️ 见下 |
| `12_gc_churn` 最大停顿 / RSS | ≈ 持平 / ≤ +5% | **−1~+28% / +11~+14%** ⚠️ 见下 |
| 峰值 RSS（其余） | ≤ +5% | `semantics` −2.8~+0.6%、`a09` ±0~+3.8% ✅ |
| 编译产物 | 逐字节一致 | ✅ |

⚠️ 都落在合成负载上（区间是两轮独立测量的跨度，`13_gc_large_heap` 的墙钟离散度本来就大），
方向可解释：自适应把 nursery 压小 ⇒ minor 变密 ⇒ 高存活负载上多做了功（两档 `large_heap` 的墙钟），
以及晋升更早（`12_gc_churn` 的 RSS，正是 D3 预告的过早晋升）。
真实负载 `z42c.semantics` 三项全过，`09_alloc_ctorless` 三项也全过。

**正确性**：`xtask test` GREEN；`Z42_GC_SLICE_MS=0.05` + 自适应 nursery 跑全套 stdlib 测试
（两个自适应回路同时工作时的交互）。
