# Design: 脚本性能分析方案

> 配套 [proposal.md](proposal.md)。本文件承载技术设计与决策；各阶段实施时其 change 容器的 design.md
> 细化该阶段实现，本文件是程序级总设计。

## 核心模型：两个世界

一个 .z42 脚本的"性能"必须分两层测，工具与归因方式不同：

```
┌─────────────────────────────────────────────────────────┐
│ script 世界  ——  哪个 .z42 函数 / 行 热、分配多            │
│   归因单位: z42 function / IR ip / 源码行                  │
│   工具:     VM 内部埋点 (采样 profiler / counter)          │
│             + .zsym 离线符号化把 native ip → z42 帧        │
├─────────────────────────────────────────────────────────┤
│ native 世界  ——  VM 实现哪段热 (dispatch / GC / 分配器)    │
│   归因单位: Rust 栈帧                                       │
│   工具:     samply / dhat / perf / Instruments (外部)      │
│             读 DWARF, 零 VM 改动                            │
└─────────────────────────────────────────────────────────┘
```

优化被跑的脚本看 script 世界；优化 VM 本身看 native 世界。P0 主攻 native 世界（外部工具即得），
P2 补 script 世界（需 VM 采样埋点）。

## 三层 × 四维矩阵

| | 堆分配 | CPU 热点 | 多线程 / 并发 | 端到端 |
|---|---|---|---|---|
| **P0 外部**（零改动） | `dhat`(feature) / heaptrack / Instruments Allocations | **samply** / Instruments / perf | samply 分线程 / Instruments Thread States | hyperfine + `/usr/bin/time` RSS |
| **P1 计数**（小埋点） | `allocations` + gc minor/major + reclaimed，暴露 `Std.Diagnostics` | per-函数 call-count（复用 JIT `call_counts`）+ 可选指令计数 | safepoint park 时长 + 三 Mutex 锁等待 | peak-RSS 入 schema |
| **P2 归因/trace** | 采样式分配点归因 | safepoint 采样 → z42 火焰图 | 每线程时间线合并 | perfetto trace 导出 |

## 分维度设计

### 堆内存分配

- **native（P0）**：`dhat-rs` 全局 allocator 包一层，feature `dhat-heap`；跑完出 `dhat-heap.json`，
  Firefox dhat viewer 看分配点归因（哪条 VM 路径分配最多、峰值、总字节）。跨平台首选。平台增强：
  macOS `MallocStackLogging` + `leaks`；Linux `heaptrack` / `valgrind --tool=massif`。
- **counter（P1）**：`allocations` 在 [`gc/arc_heap.rs`](../../../../src/runtime/src/gc/arc_heap.rs)
  分配路径 bump；`HeapStats` 拆 `minor_collections` / `major_collections` / `reclaimed_bytes`
  （generational 内部已有区分，未 surface）；`Std.Diagnostics.counters()` 暴露到脚本自测。
- **script（P2）**：分配 slow-path 采样当前 z42 帧，归因到 .z42 函数。
- 现成 knob：`Z42_STACKALLOC=stats`（栈分配 vs 逃逸到堆比例，[`interp/stack_alloc.rs:176`](../../../../src/runtime/src/interp/stack_alloc.rs)）、
  `GC.WriteHeapSnapshot(path)`、`Z42_LOG=z42::gc=trace`、`Z42_GC_MODE` 切算法对比。

### CPU 热点

- **native（P0）**：`samply record z42vm app.zbc` → Firefox Profiler（Rust 火焰图 + 分线程 +
  反向调用树，读 DWARF，mac/linux 同命令、mac 免 root）。`cargo flamegraph` 出 SVG 备选。
  后端对比：同脚本 `--mode interp` vs `--mode jit` 各跑，验证 JIT 是否真消掉 dispatch。
- **counter（P1）**：per-函数调用计数**已存在**——JIT tiering 的
  [`jit/frame.rs`](../../../../src/runtime/src/jit/frame.rs) `call_counts: Vec<AtomicU32>`
  驱动 tier-up，只需 surface 即得 Top-N 热函数。可选 feature-gated 全局指令计数器（默认关）。
- **script（P2）**：safepoint 采样 profiler（见下），出 z42 源码级火焰图。

### 多线程 / 并发

**并发模型事实**（避免对错靶）：z42 **无后台 GC 线程、无后台 JIT 编译线程**。GC 是协作式
safepoint stop-the-world（抢到 CAS 的 mutator 兼职收集），JIT 是调用线程上 lazy 编译。并发瓶颈只有
两处：**① 共享 `VmCore` 的三把 `Mutex`**（`vm_contexts` / `mutexes` / `channels`，
[`vm_context.rs`](../../../../src/runtime/src/vm_context.rs)）**② STW 暂停**。方案对准这两处。

- **native（P0）**：samply / Instruments 分线程时间线看每 worker running vs blocked；
  唯一线程负载 `bench/scenarios/06_thread_scaling.z42`（4 worker × `Channel<long>`）用不同 worker
  数跑画 speedup 曲线；锁竞争 Linux `perf lock` / macOS Instruments System Trace。
- **counter（P1）**：safepoint 命中累计每线程 park 时长（并入 `PauseHistogram`）；三 Mutex 包一层记
  acquire 等待时长 + 争用次数。配合 `Z42_SAFEPOINT_THROTTLE` 量化"节流 vs 暂停延迟"权衡。
- **trace（P2）**：每线程 run / park / GC-work / lock-wait 分段进 perfetto，看清 STW 冻结时刻。

### 端到端

- **P0**：hyperfine 已做 wall-clock；补 peak-RSS —— `/usr/bin/time -l`(macOS) / `-v`(Linux)。
- **P1**：peak-RSS 写进 `bench/results/e2e.json`（schema v2 memory 位）+ 阶段耗时拆分
  （编译 / 加载 / 执行，[`app.rs`](../../../../src/runtime/src/app.rs) 各阶段已有边界）。
- **P2**：`--trace-out` 全程时间线。

## 统一入口 `xtask profile`

对齐现有 `xtask bench`（[`scripts/xtask_bench.z42`](../../../../scripts/xtask_bench.z42)）风格：

```
xtask profile <script.z42> [--mode interp|jit] [-- args]
  --cpu       # samply → 火焰图 (+ P2: 附 z42 级火焰图)
  --heap      # dhat build → dhat-heap.json + GC stats 摘要
  --threads   # 分线程时间线 + safepoint/锁竞争计数 (06 场景可 --workers N)
  --e2e       # hyperfine + peak-RSS + 阶段耗时
  --all       # 四维全跑 → report.md
```

职责：先 z42c 编 `.zbc` → 按维度选工具挂 z42vm → 汇总。平台差异（samply/perf、`time -l`/`-v`）在
脚本内屏蔽，mac/linux 命令一致。

## safepoint 采样 profiler（P2 核心）

复用已有 safepoint 轮询机制，几乎零额外结构：

```
后台计时线程 (周期 T, 如 1ms):
  设置 sample_flag = true

mutator 在 check_safepoint() (本就每隔 Z42_SAFEPOINT_THROTTLE 次轮询一次):
  if sample_flag:
    记录 (thread_id, current_function, ip) 到 per-thread 采样环形缓冲
    sample_flag = false

退出时:
  聚合所有采样 → 折叠栈 (folded stacks)
  .zsym 离线符号化 ip → z42 function/line
  → 火焰图 (script 世界) + perfetto trace
```

优点：safepoint 轮询已在跑（GC 用），采样是顺带；采样点落在 safepoint 边界，天然避开半初始化状态。
代价：采样精度受 `Z42_SAFEPOINT_THROTTLE` 约束（可为 profiling 临时调低）。

## CI 集成

- **保留** hyperfine 时间 gate（现状 >10% fail），认清它跨 runner 噪声大、只挡大回归。
- **新增确定性 gate（P1 关键收益）**：`allocations` 计数回归门禁。分配次数可复现 → 可设严格阈值，
  比 wall-time 可靠。这是让 CI 真正挡住性能回归的一刀。
- peak-RSS 先 informational 观察一轮再转 gate。
- **samply / dhat 不进 CI**（太慢、火焰图是交互产物）——本地深挖专用，符合"确定性进 CI、噪声留本地"。

## Decisions

### Decision 1: 外部 profiler 选 samply + dhat 作跨平台基座
**问题**：mac 用 Instruments / dtrace、Linux 用 perf / heaptrack，命令与产物格式不一，方案会分叉。
**决定**：samply（CPU）+ dhat（heap）作**跨平台基座**——同一命令在 mac+linux 都跑、都出可对比的
Firefox Profiler / dhat viewer 产物；平台专有工具（Instruments / perf / valgrind）作**增强**而非依赖。
**理由**：`xtask profile` 要跨平台一致；samply/dhat 读 DWARF、零 VM 改动、mac 免 root。

### Decision 2: P0 零 VM 改动先行，P1/P2 才动 runtime
**决定**：P0 只加 toolchain + Cargo feature（dhat），不碰核心 VM；P1/P2 才是 vm 类变更。
**理由**：P0 立即可用（对任意 .z42 出火焰图 + 堆报告），且不触发 spec-first 重流程；把"能用"和
"深挖"解耦，先交付价值。

### Decision 3: 确定性 counter 进 CI，wall-time 保留但认清其局限
**决定**：CI 硬 gate 用 `allocations`（确定性），wall-time 保留为软信号。
**理由**：wall-time 跨 runner ±60% 抖动，假阳性多；分配次数可复现，是可靠回归门。

### Decision 4: P2 采样复用 safepoint，不引独立采样线程打断 mutator
**问题**：常规采样 profiler 用信号 / ptrace 打断线程读栈，与 VM 的 GC safepoint 语义可能打架。
**决定**：采样点搭在 safepoint 轮询上，后台线程只置 flag、不直接读 mutator 栈。
**理由**：safepoint 边界天然安全（无半初始化帧），复用已有机制，几乎零额外成本；对齐
diagnostics.md §4 "近零成本门控"设计意图。

## 与既有设计的关系（规范冲突预防）

- [`docs/design/runtime/diagnostics.md`](../../../../docs/design/runtime/diagnostics.md) §5（counter
  扩展）/ §7（`--trace-out` perfetto）是 P1/P2 的**既有设计蓝图**，本方案是其落地路线，不另立设计。
- 按 doc-system **D2**，`docs/design/` 不再更新：P2 实现时 diagnostics.md 的知识**上浮到
  `docs/book/`** 新机制页，原文随迁移清理——避免同一设计两处并存（见 proposal Open Questions）。
- [`docs/design/testing/exec-profile-matrix.md`](../../../../docs/design/testing/exec-profile-matrix.md)
  的 profile 描述符（mode×platform×caps）复用：`xtask profile` 的产物也带 profile，防跨画像误比。

## Testing Strategy

- P0：`xtask profile examples/<x>.z42 --all` 端到端跑通、产物生成（火焰图 / dhat json / report.md）；
  mac + linux 各验一次命令一致。
- P1：新 counter 单测（分配计数在已知分配数的脚本上精确匹配）；`Std.Diagnostics` [Test] 用例；
  确定性——同脚本两次 run 的 `allocations` 逐字节一致。CI gate 用一个已知分配基线脚本。
- P2：采样 profiler 在 CPU-bound 脚本（`01_fibonacci`）上产出的火焰图热函数 = 预期热点；
  trace 导出符合 perfetto schema、可在 ui.perfetto.dev 打开。
- 全程守 `xtask test` GREEN；P1/P2 改 runtime 需 `cargo build` + `cargo test --lib`。
