# 诊断与性能分析（Diagnostics & Profiling）

> 本页讲 z42 VM 运行时**观测面**的实现机制：计数器面、并发探针、以及 safepoint **采样 profiler**
> （z42 函数火焰图 + perfetto 采样 trace）。目标是让接手者不必读大量源码即可理解「为什么这样设计」。
> 完整设计蓝图（含尚未落地的 span/事件流）见 [`docs/design/runtime/diagnostics.md`](../../../design/runtime/diagnostics.md)。

脚本性能分析程序（plan-script-profiling）分四步落地，全部**格式中立**（无 zbc/zpkg bump）：

| 阶段 | 交付 | 落点 |
|------|------|------|
| P0 | `xtask profile` CLI + `z42vm --print-stats-on-exit --stats-format=json` | `scripts/xtask_profile.z42`、`counters.rs` |
| P1a | `ProfileSnapshot`：counter 快照 + 堆派生 allocations + GC 分代 | `counters.rs`、`gc/types.rs` |
| P1b | 并发探针：safepoint park 直方图（常开）+ 用户锁争用（feature-gated）| `gc/safepoint.rs`、`corelib/sync_contention.rs` |
| P1c | `Std.Diagnostics.RuntimeStats.Counters()` 暴露给 z42 脚本 | `corelib/diagnostics.rs`、`z42.diagnostics` |
| **P2** | **safepoint 采样 profiler：z42 火焰图 + perfetto 采样 trace** | **`gc/sampler.rs`** |

---

## 1. 观测面总览（两层成本模型）

z42 的观测遵循**两层成本**原则（design §6）：

1. **常开、原子成本**：计数器（`RuntimeCounters`：builtin/native calls、JIT 编译、异常流量）、
   GC 堆统计（`HeapStats`：allocations、分代 minor/major、reclaimed bytes）、park 直方图。
   这些只在**已经要跑的慢路径**（builtin dispatch、GC 暂停）加一次 atomic add，热路径零成本 → 永远开着。
2. **opt-in、按需 gate**：用户锁争用（编译期 `profile-contention` feature，默认编译掉）、
   **采样 profiler**（运行时 `Z42_SAMPLE_HZ` flag，默认无线程无开销）。

退出时 `app.rs` 把 counter 快照 + 堆派生数 + 并发探针合成一条 `ProfileSnapshot`，
`--stats-format=json` 输出单行 JSON（sentinel key `z42vm_counters`）供 `xtask profile` 抓取。

---

## 2. Safepoint 采样 profiler（P2）—— 核心

### 2.1 动机

`xtask profile --cpu` 原本只有 **samply**（native）：给的是 **Rust / JIT machine** 栈。对「哪个 **z42
函数**吃 CPU」很不直观——JIT code 是 mangled machine frame、interp 全在 dispatch loop 里。P2 补上按
**z42 源函数**聚合的采样火焰图：`Main;foo;bar` 这样的 z42 调用栈 + 采样计数。

### 2.2 机制：复用协作式 safepoint（零信号 / 零 ptrace）

z42 的 GC 是**协作式 safepoint**：每个 mutator 在 backward-branch / call 处调 `check_safepoint`，
经 throttle（默认 ~1/1024）后偶尔进 `check_safepoint_slow` 检查 GC 相位。采样**复用**这个已有轮询——
不引信号处理器（async-signal-safe 限制多）、不 ptrace（跨平台 + 权限麻烦）：

```mermaid
flowchart TD
    T["后台定时线程<br/>(Z42_SAMPLE_HZ 设才 spawn)"] -->|"sleep(1000/hz ms)<br/>只置 flag"| F["sample_pending = true"]
    M["mutator 热执行"] -->|"每 backedge/call"| CS["check_safepoint<br/>(throttle ~1/1024)"]
    CS -->|"偶尔慢路径"| CSS["check_safepoint_slow<br/>(Idle 末)"]
    CSS -->|"sampler.enabled()?<br/>一次 atomic load"| G{"enabled?"}
    G -->|否 (默认)| X["返回，零额外成本"]
    G -->|是| SW{"sample_pending.swap(false)?"}
    SW -->|false| X
    SW -->|true| SN["快照 ctx.call_stack<br/>→ folded 'Main;foo;bar'"]
    SN --> ACC["accum.folded[folded] += 1<br/>(+ trace: intern 帧树 + push (ts,leaf))"]
    EX["退出 (app.rs)"] --> FL["flush_folded → .folded (火焰图)<br/>flush_trace → chrome JSON (perfetto)"]
```

**关键决策**：

- **后台定时线程给时间比例采样**（D2）：`sleep(1000/hz)` 置 flag → 样本按墙钟均匀 → **时间比例**火焰图
  （标准语义）。备选「每 K 次 safepoint 采一次」是执行密度采样，语义不如时间比例直观。
- **默认关 = 运行时 flag gate，非 cargo feature**（D3）：采样检查落在**已 throttle** 的 slow path，
  只多一次 `sampler.enabled()` atomic load；`Z42_SAMPLE_HZ` 未设时无后台线程、flag 永不置 → **零成本**。
  （区别于 P1b 锁争用探针在**每次 lock acquire**、故需编译期 feature gate。）
- **累加器 = `Mutex<HashMap<folded, u64>>`**（D4）：采样点持栈快照拼 folded 字符串自增；争用极低
  （~kHz 采样、每次 µs 级）。v1 **全局一张图**（多线程栈混在一起）；per-thread 分轨 Deferred。
- **采样偏差**（D1，记 Deferred）：只在 safepoint 采 → JIT 内联段 / 无 safepoint 的紧循环采不到。

### 2.3 两路输出，一次采样

采样点除累加 folded 计数外，若 `Z42_TRACE_OUT` 设，再用**同一次栈快照**记一条 `(ts_us, leaf_frame_id)`：

- **folded → 火焰图**：`flush_folded` 按 count 降序写 `frame1;frame2 <count>` 每行（inferno /
  flamegraph.pl 标准输入）。`inferno-flamegraph` 渲成 SVG。
- **(ts, 栈) → perfetto/chrome trace**：`flush_trace` 写 chrome legacy JSON。

### 2.4 perfetto trace 是**采样型**（非 span 埋点）

chrome/perfetto trace 常见于「每帧 enter/exit 精确计时」的 **span 埋点**——那需要给每次调用插桩、
违反默认零成本、且依赖 design §4.2 尚未落地的 span 基建。z42 的 perfetto 输出**绕开**它：chrome trace
格式**原生支持采样** profiling（`ph:"P"` sample 事件 + `stackFrames` 帧树），故用同一次采样的栈快照
即可，零额外热路径成本。

```json
{ "traceEvents": [ {"ph":"P","name":"bar","pid":1,"tid":1,"ts":100,"sf":"2"}, … ],
  "stackFrames": { "0":{"name":"Main"}, "1":{"name":"foo","parent":"0"},
                   "2":{"name":"bar","parent":"1"} } }
```

`stackFrames` 是**增量 intern** 的帧树：每层用 `(parent_id, name)` 去重，共享前缀（如 `Main;foo`）只存一份；
`ph:"P"` 样本引用叶帧 id。perfetto UI（<https://ui.perfetto.dev>）直接 import 渲成**采样火焰图 over time**。
时间线继承 2.2 的采样偏差，不额外引入开销。内存有 `MAX_TRACE_SAMPLES`（10M）兜底，超出停止记 trace
样本（folded 仍完整）并一次性 stderr 警告。

### 2.5 env knobs

| knob | 作用 | 默认 |
|------|------|------|
| `Z42_SAMPLE_HZ` | 采样频率（Hz，≥1 开启）；`0`/非法 → 关 + 警告 | unset = 关（零成本）|
| `Z42_SAMPLE_OUT` | folded 输出路径 | `z42-samples.folded`（仅采样开时写）|
| `Z42_TRACE_OUT` | chrome/perfetto trace 路径；设了才记时间线 | unset = 不写 trace |

`xtask profile --cpu <script>` 现在两层都跑：samply（native）+ 用 `Z42_SAMPLE_HZ=4000`(+`Z42_TRACE_OUT`)
跑一遍 → `inferno-flamegraph` 渲 SVG（缺则留 `.folded` + 安装提示）+ perfetto trace 产物（镜像 `--heap`
的 dhat 产物模式；缺工具永不让 profile 失败）。

---

## 3. 并发探针（P1b）与计数暴露（P1c）速览

- **park 直方图**（常开）：`VmCore.park_histogram` 在 `gc::safepoint::park_until_idle` 计时——只 GC 暂停
  slow path 跑，热 `check_safepoint` 零成本。surface `park_count`/`park_us_total`/`park_max_us`。
- **用户锁争用**（`profile-contention` feature）：`corelib::sync` 的 `Std.Threading.Mutex`/`RwLock` acquire
  经 `try_lock` miss 判争用 → `lock_contentions`/`lock_wait_us`；默认构建 `#[cfg(not)]` 零分支。
- **`Std.Diagnostics.RuntimeStats.Counters()`**：11 只读属性（7 counter + allocations + 3 分代）暴露给 z42
  脚本；backing `__diag_counters` builtin 按名投影。**注意命名**：新 stdlib 简名须先 grep `src/libraries/`
  确认不撞 prelude（尤 `z42.core`）——冲突可能不报编译错，而在 inline 语境**静默误绑 Null**（P1c 大坑）。

---

## 4. 交叉引用

- 设计蓝图（含 span/事件流/统一总线的未落地部分）：[`docs/design/runtime/diagnostics.md`](../../../design/runtime/diagnostics.md)
- 堆保留诊断（whyRetained）：[heap-diagnostics.md](heap-diagnostics.md)
- safepoint / GC 暂停机制：[GC]() ·（`gc/safepoint.rs`）
- interp/JIT 语义单一真相源：[interp-jit-semantics.md](interp-jit-semantics.md)
