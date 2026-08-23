# Design: safepoint 采样 profiler

> 脚本性能分析程序 P2（最后一个单元）。统一 DRAFT（P1b/P1c/P2）已 User 批准；本文件是 P2 分支。

## Architecture

```
启动 (VmContext::new_internal, 若 Z42_SAMPLE_HZ 设):
  Sampler::start(hz)
    └─ spawn 后台线程: loop { sleep(1000/hz ms); sample_pending.store(true) }   // 只置 flag

热执行 (mutator @ safepoint):
  check_safepoint_slow(ctx)                      [safepoint.rs]
    ├─ (GC 相位处理，P1b park 计时…)
    └─ Idle 路径末：
        if sampler.enabled && sample_pending.swap(false):
            folded = walk ctx.call_stack → "f_bottom;…;f_top"   // VmFrame.func_name
            sampler.accum.lock()[folded] += 1

退出 (app.rs run() 结束):
  sampler.flush_folded(Z42_SAMPLE_OUT | "z42-samples.folded")   // 每行 "folded <count>" → 火焰图
  if Z42_TRACE_OUT 设: sampler.flush_trace(Z42_TRACE_OUT)       // chrome/perfetto JSON（采样型）

xtask profile --cpu:
  samply（native，保留）+ z42-level: Z42_SAMPLE_HZ(+Z42_TRACE_OUT) 跑一遍 →
    inferno(folded)→火焰图 SVG | .folded + 提示；trace JSON → perfetto UI 提示
```

## Decisions

### Decision D1: 复用协作式 safepoint 轮询（不用信号 / ptrace）
DRAFT D5（已定）。z42 已有协作式 safepoint（`check_safepoint` 每 backedge/call，throttle 后偶尔 slow）。
采样**复用**它：后台线程只置 flag，mutator 在 **已经要跑的** `check_safepoint_slow` 里顺带快照栈。
不引信号处理器（async-signal-safe 限制多）、不 ptrace（跨平台 + 权限麻烦）。代价：采样点受限于 safepoint
位置（backward-branch/call/return），JIT 内联段 / 无 safepoint 的紧循环采不到 → 偏差记 Deferred。

### Decision D2: 后台定时线程给时间比例采样（vs 执行计数采样）
选**后台定时线程**（sleep(1000/hz ms) 置 flag）→ 样本 ~按墙钟均匀 → **时间比例**火焰图（标准语义）。
备选「每 K 次 safepoint-slow 采一次」是**执行密度**采样（偏 safepoint 频繁处），语义不如时间比例直观。
线程生命周期：`Z42_SAMPLE_HZ` 设才 spawn；detached，随进程退出而亡（flush 在 run() 结束、进程退出前，无竞争）。

### Decision D3: 默认关，零成本 —— 运行时 flag gate（非 cargo feature）
采样检查在 `check_safepoint_slow`（已被 throttle 到 ~1/1024 频率），加一次 `sampler.enabled` atomic load +
（若开）`sample_pending.swap` → 频率极低、成本可忽略 → **不需要 cargo feature**（区别于 P1b contention 探针
在每次 lock acquire、故需编译期 gate）。`Sampler` 结构常在，`enabled=false` 时 flag 永不置。

### Decision D4: 累加器 = `Mutex<HashMap<String, u64>>`（folded → count）
采样点 mutator 持栈快照拼成 folded 字符串（`func_name` 用 `;` join，栈底在左）→ `accum.lock()` 自增。
`Mutex` 争用极低（采样频率 ~1kHz，每次快照 µs 级）。退出时单线程读出、按 count 降序写文件。
不存原始样本序列（省内存）；folded 聚合足够画火焰图。**per-thread 归属延后**（v1 全局一张图）。

### Decision D5: perfetto trace 本 change 一并做 —— **采样型**（复用同一采样，非 span 埋点）
**User 裁决 = B（火焰图 + perfetto 一起）**（2026-08-24）。关键 reframing：perfetto/chrome trace 不必依赖
diagnostics.md §4.2 的 **span 埋点**基建（每帧 enter/exit 计时，那才违反默认零成本、才真翻倍）——chrome trace
格式原生支持**采样型** profiling（`ph:"P"` sample 事件 + `stackFrames` 帧树）。故本 change 用**同一次
safepoint 栈快照**喂两个输出：
- **folded → 火焰图**（inferno SVG，聚合，无时间轴）。
- **(时间戳, 栈) 样本序列 → perfetto/chrome trace JSON**（时间线，perfetto UI 渲采样火焰图 over time）。

采样点除累加 folded 计数外，再记一条 `(ts_us, leaf_frame_id)`（`ts_us` = 采样相对启动的微秒；`leaf_frame_id`
= 把当前栈 intern 进帧树后的叶 id）。退出时：
- `Z42_SAMPLE_OUT`（默认 `z42-samples.folded`）永远写 folded；
- `Z42_TRACE_OUT`（默认关）设了才额外写 chrome trace JSON——**trace 记录仅在此 knob 设时开启**（省内存：
  未开时不存 per-sample 时间线，只累加 folded）。

**trace JSON 结构**（chrome legacy JSON，perfetto 直接 import）：
```json
{ "traceEvents": [ {"ph":"P","name":"<leaf>","pid":1,"tid":1,"ts":<us>,"sf":"<id>"}, … ],
  "stackFrames": { "<id>": {"name":"<fn>","parent":"<pid>"}, … } }
```
`stackFrames` 是增量 intern 的帧树（`(parent_id, name)` 去重）；`ph:"P"` 样本引用叶帧 id。
**代价仍是 D1 的采样偏差**（只在 safepoint 采）；perfetto 时间线继承同偏差，不额外引入 span 开销。
**per-thread tid**：v1 全部记 `tid:1`（全局累加，与 folded 同 Decision D4）；per-thread 分轨 Deferred。

### Decision D6: 不动格式 / 不新增 z42 API
纯运行时采样 + 文本 folded 输出 + xtask；无 zbc/zpkg 格式变更、无新 stdlib API → 无 bootstrap 两-nightly 约束。
（注：本 change 基于 origin/main 0.42（#268 格式 bump 后）；采样本身格式中立。）

## Implementation Notes

- **`Sampler`**（`gc/sampler.rs`）：
  ```rust
  pub struct Sampler {
      enabled:        bool,                          // Z42_SAMPLE_HZ 设即 true
      trace_enabled:  bool,                          // Z42_TRACE_OUT 设即 true（额外记时间线）
      sample_pending: Arc<AtomicBool>,               // 后台线程置，mutator swap(false)
      stop:           Arc<AtomicBool>,               // Drop 时置 true 让 timer 线程退出
      start:          std::time::Instant,            // 采样时间戳基准 t0
      data:           Mutex<SamplerData>,
      _thread:        Option<std::thread::JoinHandle<()>>,
  }
  struct SamplerData {
      folded:    HashMap<String, u64>,               // folded stack → count（火焰图）
      // ↓ 仅 trace_enabled 时填充（perfetto 采样时间线）
      frames:    Vec<FrameNode>,                     // intern 的帧树
      frame_ids: HashMap<(u32, Arc<str>), u32>,      // (parent|MAX, name) → node idx 去重
      samples:   Vec<(u64, u32)>,                    // (ts_us, leaf frame id)
  }
  struct FrameNode { name: Arc<str>, parent: u32 }   // parent=u32::MAX 为根
  impl Sampler {
      pub fn start(hz: u32, trace_enabled: bool) -> Self { /* spawn timer 线程; enabled=true */ }
      pub fn disabled() -> Self { /* enabled=false, 无线程 */ }
      pub fn enabled(&self) -> bool { self.enabled }
      pub fn maybe_sample(&self, ctx: &VmContext) { /* swap flag → walk call_stack → folded++ (+trace intern) */ }
      pub fn flush_folded(&self, path: &str) -> std::io::Result<()> { /* 降序写 folded */ }
      pub fn flush_trace(&self, path: &str)  -> std::io::Result<()> { /* 写 chrome JSON（P 事件+stackFrames）*/ }
  }
  ```
  - 栈快照：`for f in ctx.call_stack.lock().iter() { push f.func_name }`，`join(";")`（栈底在左=call_stack
    顺序）。空栈跳过。folded 计数**恒**累加；`trace_enabled` 时再 intern 帧树 + push `(elapsed_us, leaf_id)`。
  - `maybe_sample` 只在 `enabled` 时进（调用点已 `if sampler.enabled()` gate）。
- **safepoint hook**（`check_safepoint_slow` Idle 末，`needs_auto_collect` 处理后）：
  ```rust
  if ctx.core.sampler.enabled {
      ctx.core.sampler.maybe_sample(ctx);
  }
  ```
  注意：不能在持 GC 相关锁时采样；此处已过 GC 分支、Idle，安全。call_stack 锁与 gc_phase 锁无嵌套。
- **config**：`Z42_SAMPLE_HZ`（u32，采样频率，默认 unset=off）、`Z42_SAMPLE_OUT`（路径，默认 `z42-samples.folded`）、
  `Z42_TRACE_OUT`（路径，默认 unset=不写 trace）加进 `KNOWN_KNOBS` + `RuntimeConfig`（`sample_hz: Option<u32>` /
  `sample_out: Option<PathBuf>`(带默认) / `trace_out: Option<PathBuf>`）。
- **vm_context.rs**：`VmCore.sampler: Sampler`；`new_internal` 按 `runtime_config().sample_hz` → `Sampler::start(hz,
  trace_out.is_some())` 否则 `Sampler::disabled()`。
- **app.rs**：run() 结束（`print_stats` 附近，无条件）：`if sampler.enabled() { flush_folded(sample_out); if trace_out
  设 flush_trace(trace_out) }`，各打印一行提示到 stderr。
- **xtask profile `_profileCpu`**：现有 samply 分支保留；追加 z42-level：设 `Z42_SAMPLE_HZ`(+`Z42_TRACE_OUT`) 跑一遍，
  读出 `.folded`；`inferno-flamegraph` 在 PATH 则 `inferno-flamegraph < folded > flame.svg`，否则留 `.folded` + 提示
  （`cargo install inferno`）；trace JSON 落 `outDir` + perfetto (`https://ui.perfetto.dev`) 查看提示。`|| true` 不让
  profile 失败。

## Testing Strategy

- **单元测试**（`sampler_tests.rs`）：构造 VmContext + 手动 push 几个 VmFrame 到 call_stack + `Sampler::start`
  或直接置 `sample_pending` → `maybe_sample` → 断言 accum 有对应 folded 键 count>=1；folded 格式（`;` join、
  栈底在左）；空栈不产坏行；`flush_to` 写出降序。
- **端到端**（0.42 seed 就绪后）：一个 `while` 热循环调两层函数的脚本，`Z42_SAMPLE_HZ=2000` 跑 → `.folded`
  含热函数键、且其 count 最高。
- **VM 验证**：`cargo build` + `cargo test --lib`；`xtask test`（完整 GREEN，**格式-bump 期以 CI 两代自举为权威**）。
- **零成本确认**：采样关时 `cargo bench`/thread_scaling 与 baseline 无差（采样检查 gated + throttled）。

## Deferred

- **span 埋点型 trace**（每帧精确 enter/exit 计时，diagnostics §4.2）：本 change 的 perfetto 是**采样型**、非埋点型；
  精确 span 时间线依赖 §4.2 基建，仍 Deferred。
- **per-thread 火焰图 / trace 分轨归属**：v1 全局累加（folded 一张图 + trace 全记 `tid:1`）；per-thread 分轨延后。
- **JIT 内联段采样盲区**：safepoint 只在 backward-branch/call；紧内联循环采不到。
