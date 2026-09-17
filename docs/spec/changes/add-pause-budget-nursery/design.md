# Design: 按停顿预算自适应 nursery

## Architecture

```text
             ┌─ 每次 minor 结束 ────────────────────────────────────┐
             │  pause_us（这次停顿）                                │
             │  consumed_bytes（本次 minor 之前积累的分配量）        │
             └───────────────┬──────────────────────────────────────┘
                             ▼
                   cost_ns_per_byte  ← EWMA（α = 1/4）
                             │
                             ▼
   next_nursery = clamp(target_us * 1000 / cost_ns_per_byte, MIN, MAX)
                  且 |next − cur| ≤ cur/2        ← 单次变化不超过 ±50%
                             │
                             ▼
        auto_collect 的 minor 闸门（`nursery_bytes()`）下次就用它
```

**为什么是「每字节」而不是「每条目」**：闸门是按**分配字节**触发的（`grown >= nursery`），
所以预算必须换算回字节才能直接设闸门。条目数只是中间量（实测 ~63 ns/条、~48 B/条），
而且对象大小随负载变化 —— 按字节测量把这个差异自动吸收掉。

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

**做法**：模型只吃「本次 minor 的停顿」整体，不拆分 —— 于是固定开销自动表现为
「nursery 越小，每字节代价越高」，模型会自己停在「再缩也换不来停顿」的那个点上。
这也是 `MIN_NURSERY` 存在的理由：低于它，固定开销占比过高，缩 nursery 只剩坏处。

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

**决定**：退避乘数保留（它防的是「徒劳回收把 CPU 烧光」），但**闸门取 `min(gate * backoff, pause_budget_nursery)`**。
效果：退避仍然让回收变稀疏（因为 `next_collect_at` 推得更远），但**单次 minor 的规模有硬上界**。

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

| 指标 | 门槛 |
|---|---|
| `z42c.semantics` 最大停顿 | **≤ 10 ms**（当前 17~19） |
| `13_gc_large_heap` 两档最大停顿 | **≤ 10 ms**（当前 82~89 / 288~346） |
| `09_alloc_ctorless` 最大停顿 | 显著下降（当前 104~118），墙钟回归 ≤ 5% |
| 总停顿 | ≤ +10% |
| 墙钟 | ≤ +2% |
| 峰值 RSS | ≤ +5%（**含晋升字节列**，见 D3） |
| 编译产物 | 逐字节一致 |

**正确性**：`xtask test` GREEN；`Z42_GC_SLICE_MS=0.05` + 自适应 nursery 跑全套 stdlib 测试
（两个自适应回路同时工作时的交互）。
