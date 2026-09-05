# Proposal: JIT 对「解释器主导」的负载完全不介入（investigate-jit-never-engages-from-interp）

> 状态：📝 调查完成，待 User 裁决方向 | 创建：2026-09-04 | 类型：vm
> **本文只报告实测事实与被证伪的朴素解法，不含实施。**

## Why

**实测**：用 z42c 编译它自己的 `z42c.semantics`（**28 k 行 z42**，单次 13.0 s）做 profile，
7532 个采样的调用树**根部只有一个 `jit_call`**，往下全是
`exec_function_body → exec_function_from_regs` 的深链——**整个编译几乎全程解释执行**。

加临时插桩量化（按函数名去重统计 `jit_unsupported_reason` 的调用）：

> **13 秒的编译里，`jit_unsupported_reason` 只被调用了 1 次。**

也就是说**不是 JIT 翻译不了这些函数，是它们从来没被问过**。

### 机制（代码里写明了的）

`interp/exec_support.rs::try_native_exec` 用的是 `resolve_fn_by_name_peek`，注释原文：

> "Phase 1.5.2: peek (already-compiled?) — **NOT** the tiered resolve. The divert only
> ROUTES already-hot functions to native; tier-up counting belongs to the primary call
> sites (`jit_call` / per-site interp hooks / `jit_obj_new` / vtable)."

而那些 primary call sites **全都要求执行已经在 JIT 代码里**。于是：
**一旦执行掉进解释器，函数粒度上就再也爬不出来**——只有 OSR（循环回边）能逃。
编译器是递归下降、每个函数本身不怎么循环，所以整段 13 秒基本没进过 JIT。

这不是 bug，是 Phase 1.5.2 的**刻意设计**（避免重复计数），但它的副作用是
「解释器主导的负载 = JIT 永不介入」，而**自举编译器正是这类负载**。

## 实测：把 peek 换成 tiered（一个词）的上界与代价

| | z42c 编译 semantics | `xtask test` 全量 |
|---|---|---|
| base（peek-only） | 13.038 s | ~5:00 |
| **tiered，阈值 2（默认）** | **11.921 s（1.09×）** | **6:31（+30%）** |

- JIT 开始考虑 **1000+ 个函数，其中 100% 可翻译**（一个 rejection 都没有）。
- 编译产物 zpkg **字节完全相同**；`bench/scenarios` 全平（它们本来就是 JIT 主导的循环）。
- **但全量测试慢 30%**——正是 Phase 1.5.2 与 #415（阈值 1→2，测试 7:25→5:08）担心的
  「一次性函数的编译税」。测试套件跑的是**大量短命进程**，编译成本永远摊不回来。

### 那 +30% 落在哪(逐 stage 实测)

对 `xtask test` 的每行输出打时间戳,两侧逐段作差(总计 303.3 → 387.3 s,**+84 s / +28%**):

| stage | peek | tiered | Δ |
|---|---:|---:|---:|
| **stdlib [Test]**(约 3630 个 fork 的测试进程) | 81.3 s | 122.4 s | **+41.2 (+51%)** |
| build-runtime 之后到 goldens 之间的准备 | 20.7 s | 40.6 s | **+19.9** |
| e2e cross-zpkg | 23.7 s | 38.2 s | **+14.5** |
| **compiler**(自举不动点,z42c 编译自己) | 30.1 s | 36.8 s | **+6.7 (+22%)** |
| 其余各段 | | | ≤ +2 |

**关键反直觉点**:`compiler` 那段是 z42c 自编译 —— 按机制它「应该」变快(独立测的
13 s semantics 编译确实快了 9%),实际却**慢了 22%**。差别在于:那 13 s 是**一个**进程干
一大堆解释执行的活,编译成本摊得回来;而自举不动点是把活拆成**多个较短的进程**,每个
都从零开始付编译税。

⇒ 判据既不是「程序短」也不是「调用多少次」,而是
**「单个进程会不会解释执行到足以摊回它触发的编译」**。
（旁证:hello 7 ms 与 09_alloc_ctorless 250 ms 实测**都不慢** —— 它们压根不做会触发
tier-up 的那类工作,是不合格的探针。）

### 阈值扫描证明「调参救不了」

| `Z42_JIT_THRESHOLD` | 编译耗时 |
|---|---|
| 2（默认） | 11.921 s |
| 8 | 12.868 s |
| 32 | **13.312 s（比 base 还慢）** |
| 128 | 13.679 s |

阈值一抬收益立刻归零，再抬就**低于 base**——因为高阈值下这个 divert 每次调用都做一次
**完整的 tiered resolve**（注册 lazy slot、走 `jit_unsupported_reason`、加计数）却永远不编译，
纯是白付开销；base 的 peek 反而更便宜。**所以是二选一，没有中间地带。**

## What Changes（候选方向，都需要 User 裁决）

真正的判据不是「调用了几次」，而是**「这个进程还会活多久、编译成本摊得回来吗」**：

- **方向 1：时间预算**。进程运行超过 X ms 之后才允许 interp 侧触发 tier-up。
  长编译（13 s）吃到全部收益；短测试进程完全不受影响。需要一个单调时钟读，
  在 divert 上是热路径。
  ⚠️ 但逐 stage 数据显示**光靠这个不够**：阈值 128（几乎不编译）时编译负载仍比 base
  慢 5%，说明 tiered resolve **本身**每次调用就比 peek 贵（注册 lazy slot + 全指令
  扫描 + 计数）。任何方案都必须让**不编译的常态路径保持 peek 的成本**。
- **方向 2：编译配额**。每进程 interp 触发的编译数设上限（如 200 个），超了退回 peek。
  比时间更可预测，但上限本身是个魔数。
- **方向 3：只对「已经解释执行了很久」的函数**。给 interp 侧一个**独立的、更高的**
  计数器（不与 JIT 侧共用，避免 Phase 1.5.2 说的重复计数），配合方向 1/2。
- **方向 4：不做**。接受「自举编译器全解释」，把 CPU 预算花在解释器本身
  （`exec_function_body` 自占 36.6%）。

## 已实测证伪的修复形态：「先 peek 再计数 + 时间预算」

方向 1+3 的组合（廉价计数器守门、越线才做完整 resolve、外加进程寿命预算）**已实现并实测，
不成立**。记录形态与原因，避免重走：

```rust
// 试过的形状（jit/frame.rs::resolve_fn_by_name_interp_tiered）
if let Some(e) = self.resolve_fn_by_name_peek(name) { return Some(e); }  // 1. 快路径
let id = self.resolve_id_by_name(name)?;                                  // 2. 拿 id
let n  = self.bump_interp_count(id)?;                                     // 3. 计数
if n < th || n % th != 0 || !budget_reached() { return None; }             // 4. 门
self.resolve_fn_by_id(id as usize)                                        // 5. 越线才编
```

| | 编译 semantics |
|---|---:|
| base（peek-only） | 12.979 s |
| 朴素版（peek→tiered，一个词） | **11.921 s** |
| 本形态 th=2 / budget=400 ms | 12.941 s（**无收益**） |
| 本形态 th=8 / budget=400 ms | 13.458 s |
| 本形态 th=2 / budget=2000 ms | 13.403 s |

**为什么不成立**：`peek` 与 `resolve_id_by_name` **查的是同一张表**（合并模块的
`func_index`，未命中再进 `lazy_table` 的锁）。先 peek 再 resolve 等于**把同一次查找做了两遍**，
而且是在「尚未编译」这条最频繁的路径上。对永远不会越线的函数，每次调用都白付两遍——
朴素版只查一遍，所以反而更快。**「保持 peek 的常态成本」与「在未命中时计数」不能拆成两步做。**

### 中途还踩到的一个实现错误（同样值得记）

第一版只处理合并模块里的函数（`module.func_index.get(name)` 未命中就 return），
而 z42c build 里**编译器自己的包（z42c.semantics / .syntax / .core）全是懒加载的**，
根本不在合并模块——把真正要紧的那批全跳过了，于是"调参无效"看着像阈值问题，实则是漏了半边。
**改这条路时先确认 merged / lazy 两个 id 空间都覆盖到。**

### 下一次该从哪切

计数必须**搭在 peek 已经做的那次查找上**，而不是另起一次：给 peek 加一条能同时返回
「未编译 + 该函数的 counter 位置」的路径（合并路径是 `call_counts[idx]`，懒路径是
`LazySlot::count`，两者都已存在），再叠时间预算。这样常态路径的成本才真正等于今天的 peek。

## Scope

本次调查**未改动任何生产代码**（插桩与两个天花板实验都已 revert）。
方向选定后再开实施 change。

## 附：同一轮里被证伪的两个候选（有数据，别重做）

- **TLS 探针合并（已撤回，不合）**：`_tlv_get_addr` 在本 profile 占 **9.8%**（739/7532），
  合并 TLAB 的双探针 + `str_meta` 的双 thread_local 之后，实测只有
  **−0.26% 指令数 / 1.01× 墙钟**。
  这是同一个坑的**第二次**（第一次在 `09_alloc_ctorless` 上）。
  **结论：`_tlv_get_addr` 的采样数严重高估真实成本，别再照着它改。**
  （该合并本身无害且轻微为正，已随本轮一起提交。）
- **帧登记去锁**：`push_frame` + `pop_frame` 占 4.7%。先做**天花板实验**（直接绕过
  `call_stack` 的锁，不健全但能测上界）：只有 **−1.5% 指令数 / ~2% 墙钟**。
  为 2% 去动 GC root 可见性的正确性关键路径，不划算。
