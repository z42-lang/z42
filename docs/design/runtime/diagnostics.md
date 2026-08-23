# 诊断与跟踪（Diagnostics & Tracing）

> **状态：DESIGN（部分已实施，扩展未实施）** · 创建 2026-06-21
>
> 设计 z42 VM 的诊断/跟踪子系统：**诊断事件**（编译、类型加载、GC、deopt、load-context…）、**计数统计**、**时间统计**（含 per-函数编译耗时）。在现有 `observer.rs` / `counters.rs` / `tracing` 地基上补齐缺口。
>
> 与 [tiered-execution.md](tiered-execution.md)（tier 回收 `inspect_artifacts`）、[load-context.md](load-context.md)（`whyRetained` 保留根诊断）、[componentized-runtime.md](componentized-runtime.md)（observer/注册槽、`libz42_debug` 组件）共用同一事件总线。

---

## 1. 现状盘点（已有，扩它而非重造）

| 设施 | 现状 | 文件 |
|---|---|---|
| **RuntimeObserver** 事件流 | 事件：`ModuleLoaded`/`JitModuleCompiled`/`ExceptionThrown`/`ExceptionCaught`/`NativeCallEntered` + `Custom{source,payload}` 逃逸口；`Mutex<Vec<Arc<dyn RuntimeObserver>>>` on VmCore；对标 CoreCLR EventPipe | [observer.rs](../../../src/runtime/src/observer.rs) |
| **GcObserver** | 独立 `GcEvent` 流（与 Runtime 那套**分离**，两套 trait） | [gc/types.rs](../../../src/runtime/src/gc/types.rs) |
| **RuntimeCounters** | `AtomicU64`：`builtin_calls`/`native_calls`/`jit_methods_compiled`/**`jit_compile_us_total`**/`exceptions_thrown`/`exceptions_caught`；`snapshot()` + `--print-counters`；对标 dotnet EventCounters | [counters.rs](../../../src/runtime/src/counters.rs) |
| **tracing** crate | 日志（`tracing::warn!` 等） | Cargo.toml |

**三方面真实状态**：事件=有但薄；计数=已有且有快照（**有必要、且已在**）；时间=仅 `jit_compile_us_total` 一个粗聚合，**per-函数/分位/span 缺失**。

---

## 2. 模型：三个面、两层

三者是一条流水线的三个面：
- **事件（event）**：离散发生点（instant）或区间（span = begin/end + duration）。
- **计数（counter）**：事件的聚合（count / gauge / histogram）。
- **时间（timing）**：span 时长 → 喂 histogram + 作 span 事件字段。

落成**两层**（z42 已是此结构，对标 .NET EventCounters vs EventPipe / JFR）：
- **L1 计数层**：always-on、极廉（`AtomicU64` Relaxed add），无 per-event 成本。常开可观测面。
- **L2 事件层**：opt-in、富信息（字段 + span）、`has_observers` 门控。详细 tracing/诊断。

**分流原则**：高频（每调用/每分配）→ 只进 L1 计数或采样；低频高价值（编译、类型加载、GC、deopt、context）→ L2 全量 emit。

---

## 3. 事件分类与清单（L2）

事件统一为：`{ id, category, kind: Instant|SpanBegin|SpanEnd, ts, thread, fields, span_id? }`。按 category：

| category | 事件 | kind | 关键字段 | 现状 |
|---|---|---|---|---|
| **load** | `ModuleLoaded` | instant | name, byte_size | ✅ 已有 |
| | `ModuleUnloaded` | instant | name, ctx | 🆕 |
| | `ContextCreated` / `ContextUnloadRequested` / `ContextTorndown` | instant | ctx_id, version | 🆕（load-context） |
| **type** | `TypeLoad` | **span** | type_name, ctx, members | 🆕 |
| | `MetadataBuilt` | span | type_name | 🆕 |
| **compile** | `JitCompile` | **span**（begin/end）| func, tier, bytes_in, code_bytes, **duration_us** | ⚠️ 现仅完成事件 `JitModuleCompiled`，**无 begin/无 per-func 时长** |
| | `InterpTierUp` | instant | func, from_tier, to_tier | 🆕（tiered §3） |
| | `Deopt` | instant | func, reason, bci | 🆕（tiered §4） |
| | `Osr` | instant | func, direction(entry/exit) | 🆕（tiered §6） |
| **gc** | `GcCycle` | **span** | kind(minor/major), bytes_before/after, reclaimed | ⚠️ 在 GcObserver，**未并入统一流** |
| | `GcPhase` | span | phase(mark/sweep), stw_us | 🆕 |
| | `SafepointStop` | span | stw_us, threads | 🆕 |
| **exec** | `ExceptionThrown` / `ExceptionCaught` | instant | type, site | ✅ 已有 |
| | `NativeCallEntered` | instant | symbol | ✅ 已有（高频，慎用→见 §6） |
| **native** | `NativeLibLoaded` | instant | path（dlopen libz42_*） | 🆕 |
| **diag** | `RetainDiagnosed` | instant | ctx, retaining_edges | 🆕（load-context `whyRetained`） |
| **\*** | `Custom{source,payload}` | instant | — | ✅ 逃逸口 |

> span 事件成对（`SpanBegin`/`SpanEnd` 共享 `span_id`），消费端可还原 duration、嵌套、火焰图。

---

## 4. 缺口与设计（逐项具体）

### 4.1 🔴 `fire()` 近零成本门控（现有真实开销，必修）
现 `fire()` 每次都 `Mutex::lock` + clone Vec，**空 observer 也付费**（[observer.rs:128](../../../src/runtime/src/observer.rs#L128)）：
```rust
pub fn fire(&self, event: &RuntimeEvent) -> usize {
    let snapshot = { let g = self.inner.lock(); g.iter().cloned().collect() }; // 每次锁+clone
    ...
}
```
**改**：注册表加 `observer_count: AtomicUsize`；`fire` 入口快判：
```rust
pub fn fire(&self, event: &RuntimeEvent) -> usize {
    if self.observer_count.load(Relaxed) == 0 { return 0; }  // disabled = 一次原子 load + 分支
    let snapshot = { ... };  // 仅在有 observer 时锁
    ...
}
```
→ disabled 路径近零成本，emit 点可安心广撒。`category` 级开关同理（per-category `AtomicU32` bitset）。

### 4.2 span 语义（编译耗时一等公民）
现事件全是瞬时/完成；加 `span_begin(id, cat, fields) -> SpanGuard` / `span_end(guard)`：
- `JitCompile` 改成 span：begin 记 `Instant::now()`，end 算 `duration_us` → **per-函数编译耗时**(你要的)。
- 双路输出：详细 span 事件（L2，opt-in）+ 喂 `jit_compile_us` histogram（L1，廉）。

### 4.3 统一 Gc + Runtime 事件流
现 `GcObserver` / `RuntimeObserver` 两套 trait。**桥接成一个消费模型**：保留两个 emit 源（GC 侧仍可独立），但提供统一 sink 接口（`on_event(&Event)` with category），消费者订一次拿全部，按 category 过滤。不强拆 GC 现有内部用法。

---

## 5. 计数统计（L1）：扩它

- **加 kind**（现仅单调 counter）：
  - **counter**（累加）：现有 6 个。
  - **gauge**（瞬时值）：堆占用字节、活跃 context 数、已加载类型数、JIT code cache 字节。
  - **histogram**（分布）：编译耗时、GC 暂停、分配大小。
- **扩覆盖**：`gc_cycles`/`gc_reclaimed_bytes`/`gc_pause_us`(hist)、`types_loaded`、`deopts`、`osr_count`、`allocations`/`alloc_bytes`、`zpkg_loaded`/`zpkg_unloaded`、`contexts_active`(gauge)。
  - **已 surface 到 profile 快照（extend-runtime-counters P1a, 2026-08-23）**：`HeapStats` 新增
    `minor_collections`/`major_collections`/`reclaimed_bytes`（generational 分代拆分；`gc_cycles` 合计语义不变）；
    `--print-stats-on-exit` 输出点用 `counters::ProfileSnapshot` 合并 `RuntimeCounters` 快照 + 上述堆派生字段
    （`allocations` + 分代）成**一行** JSON（`z42vm_counters` sentinel 超集，仅加键→scraper 后向兼容），
    `xtask profile` 展示。仅进 **profile 快照**；暴露到 z42（下条）仍待 P1c。
  - （另：`native_calls`/`exceptions_thrown`/`exceptions_caught` 早于 2026-05-26 已埋点，非"待埋"。）
  - **并发探针（add-concurrency-probes P1b, 2026-08-23）**：ProfileSnapshot 再加两组并发字段。
    ① **park 直方图（常开）**：mutator 在 GC safepoint STW park 的时长，记进 `VmCore.park_histogram`
    （复用 `gc::types::PauseHistogram`），埋点在 `gc::safepoint::park_until_idle`——只在真实 GC 暂停的
    slow path 跑，`check_safepoint` 热路径零成本，故常开无 feature。surface `park_count`/`park_us_total`/
    `park_max_us`。② **用户锁争用（编译期 feature `profile-contention`，默认关）**：`Std.Threading.Mutex`/
    `RwLock` 的阻塞 acquire（`corelib::sync`）在 feature 下先 `try_lock`，miss=争用 → `lock_contentions`++
    + 计时 `lock_wait_us`；默认构建 `#[cfg]`-编译掉探针 → acquire 路径零成本（见 §6 feature-gate）。
    `xtask profile --threads` 展示 park（默认 VM）+ contention（现建 `profile-contention` throwaway VM，
    复用 `--heap` 的隔离 target-dir 手法）。仅进 profile 快照；暴露到 z42 API 延后。
- **暴露到 z42（已落地 expose-diagnostics-counters P1c, 2026-08-23）**：`Std.Diagnostics.RuntimeStats.Counters()`
  返回一个 `Std.Diagnostics.RuntimeCounters` 值对象（11 只读 auto-property = 7 运行时 counter +
  `Allocations` + 3 分代 GC 字段），backing builtin `__diag_counters`（`corelib/diagnostics.rs`，append 到
  `BUILTINS` 末尾，BuiltinId 不移位）。投影来源与 `app.rs` 的 `ProfileSnapshot` 同（`RuntimeCounters::snapshot`
  + `HeapStats`），故脚本内读值与 `--print-stats-on-exit` JSON 一致。z42.core 的 `Std.HeapStats`（7 字段）
  保持不动——全景归 Diagnostics.Runtime（决策 D3）。**无格式 bump**（builtin 运行时解析 + `[Native]` 字符串）。
  - **`allocations` 分配回归 gate**：先 **informational**（CI 打印不 fail，观察 3–5 轮跨 GC-mode 稳定性），
    稳定后由后续 change 转确定性硬 gate（绝对/相对%）。见 `.github/workflows/bench-pr.yml`。

---

## 6. 开销控制（决定能否常开，核心）
1. **两层**：counter 常开（atomic add）；event/span opt-in + `has_observers`/category 快门（§4.1）。
2. **高频事件**（`NativeCallEntered`、每分配、每调用）→ **不逐个 emit**：聚合成 counter 或采样（每 N 次 / 时间窗）；只低频高价值全量。
3. **per-thread 缓冲 + 后台 flush**（JFR 模型）：emit 只写线程本地 ring buffer，后台线程消费/写文件；**热路径不做 IO/格式化**。
4. **feature-gate**：`diagnostics` feature 可完全编译掉 emit 点（极低开销构建，对齐 mobile/wasm interp-only）。
5. **时钟**：disabled 不读时钟；Rust 侧 `std::time::Instant`（无 sandbox `Date` 限制）；极热点可选 coarse/rdtsc。

---

## 7. 暴露面
- **`Std.Diagnostics`**（挂 Std.Runtime 族）：
  - `counters() -> CounterSnapshot`
  - `subscribe(categories, sink)` / `unsubscribe`（事件订阅）
  - `span(category, name) -> SpanHandle` + `end()`（z42 代码自埋点）
  - `whyRetained(ctx)` / `inspectArtifacts()`（转发 load-context / tiered 诊断）
- **CLI**：`--print-counters`（已有）；扩 `--trace=<cat,cat>`、`--trace-out=trace.json`（**perfetto / chrome trace 格式**，火焰图/时间线可视化）。
- **统一总线**：tier 回收、load-context 卸载/保留根、context 生命周期 全 emit 到同一事件流；调试组件 `libz42_debug`（componentized）是一个 sink；可桥到 `tracing`。

---

## 8. 分阶段
1. **地基修缺陷**：`fire()` 近零成本门控（§4.1）+ span 语义（§4.2）+ 统一 Gc/Runtime sink（§4.3）。
2. **埋点**：`JitCompile` begin/end + `TypeLoad` + GC 入统一流 + `Deopt`/`Osr`/`InterpTierUp` + load-context 生命周期。
3. **计数扩**：gauge/histogram kind + 覆盖（§5）+ `Std.Diagnostics.counters()`。
4. **时间**：histogram（p50/p99）+ per-函数编译耗时面板。
5. **trace 输出**：perfetto/chrome JSON + CLI `--trace`。
6. **高频处理**：采样 / feature-gate probe。

---

## 9. 交叉引用
- tier 回收 / `inspect_artifacts`：[tiered-execution.md](tiered-execution.md)
- load-context / `whyRetained` 保留根诊断：[load-context.md](load-context.md)
- observer/注册槽 / `libz42_debug` 组件：[componentized-runtime.md](componentized-runtime.md)
- 当前架构：[vm-architecture.md](vm-architecture.md)
