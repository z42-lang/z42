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
  sampler.flush_to(Z42_SAMPLE_OUT | "z42-samples.folded")       // 每行 "folded <count>"

xtask profile --cpu:
  samply（native，保留）+ z42-level: Z42_SAMPLE_HZ 跑 → inferno(folded)→SVG | .folded + 提示
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

### Decision D5: perfetto trace 延后（本 change 只做火焰图）
folded-stacks 火焰图是核心价值、一次输出格式。perfetto/chrome trace 是**时间线**格式（每事件带 ts/dur/tid），
需要时间戳化的 span 流（对接 diagnostics.md §4.2 span 基建，那是另一块未落地的地基）→ 工作量翻倍且依赖 span。
**记 Deferred**（`add-perfetto-trace` 后续 change）。**⚠️ 须 User 确认此 scoping**（Open Question）。

### Decision D6: 不动格式 / 不新增 z42 API
纯运行时采样 + 文本 folded 输出 + xtask；无 zbc/zpkg 格式变更、无新 stdlib API → 无 bootstrap 两-nightly 约束。
（注：本 change 基于 origin/main 0.42（#268 格式 bump 后）；采样本身格式中立。）

## Implementation Notes

- **`Sampler`**（`gc/sampler.rs`）：
  ```rust
  pub struct Sampler {
      enabled:        bool,                          // Z42_SAMPLE_HZ 设即 true
      sample_pending: Arc<AtomicBool>,               // 后台线程置，mutator swap(false)
      accum:          Arc<Mutex<HashMap<String, u64>>>,
      _thread:        Option<std::thread::JoinHandle<()>>,  // detached-ish; stop flag 可选
      stop:           Arc<AtomicBool>,
  }
  impl Sampler {
      pub fn start(hz: u32) -> Self { /* spawn timer thread; enabled=true */ }
      pub fn disabled() -> Self { /* enabled=false, 无线程 */ }
      pub fn maybe_sample(&self, ctx: &VmContext) { /* swap flag → walk call_stack → accum */ }
      pub fn flush_to(&self, path: &str) -> std::io::Result<()> { /* 降序写 folded */ }
  }
  ```
  - 栈快照：`for f in ctx.call_stack.lock().iter() { push f.func_name }`，`join(";")`（栈底在左=call_stack
    顺序）。空栈跳过。
  - `maybe_sample` 只在 `enabled` 时进（调用点已 `if sampler.enabled` gate）。
- **safepoint hook**（`check_safepoint_slow` Idle 末，`needs_auto_collect` 处理后）：
  ```rust
  if ctx.core.sampler.enabled {
      ctx.core.sampler.maybe_sample(ctx);
  }
  ```
  注意：不能在持 GC 相关锁时采样；此处已过 GC 分支、Idle，安全。call_stack 锁与 gc_phase 锁无嵌套。
- **config**：`Z42_SAMPLE_HZ`（u32，采样频率，默认 unset=off）、`Z42_SAMPLE_OUT`（路径，默认 `z42-samples.folded`）
  加进 `KNOWN_KNOBS` + `RuntimeConfig`。
- **app.rs**：run() 结束（`print_stats` 附近）：`if enabled { sampler.flush_to(out) }`，打印一行提示到 stderr。
- **xtask profile `_profileCpu`**：现有 samply 分支保留；追加 z42-level：设 `Z42_SAMPLE_HZ` 跑一遍，读出
  `.folded`；`inferno-flamegraph` 在 PATH 则 `inferno-flamegraph < folded > flame.svg`，否则留 `.folded` +
  提示（`cargo install inferno` / flamegraph.pl）。`|| true` 不让 profile 失败。

## Testing Strategy

- **单元测试**（`sampler_tests.rs`）：构造 VmContext + 手动 push 几个 VmFrame 到 call_stack + `Sampler::start`
  或直接置 `sample_pending` → `maybe_sample` → 断言 accum 有对应 folded 键 count>=1；folded 格式（`;` join、
  栈底在左）；空栈不产坏行；`flush_to` 写出降序。
- **端到端**（0.42 seed 就绪后）：一个 `while` 热循环调两层函数的脚本，`Z42_SAMPLE_HZ=2000` 跑 → `.folded`
  含热函数键、且其 count 最高。
- **VM 验证**：`cargo build` + `cargo test --lib`；`xtask test`（完整 GREEN，**格式-bump 期以 CI 两代自举为权威**）。
- **零成本确认**：采样关时 `cargo bench`/thread_scaling 与 baseline 无差（采样检查 gated + throttled）。

## Deferred

- **perfetto / chrome trace `--trace-out`**（`add-perfetto-trace`）：时间线格式，依赖 span 基建（diagnostics §4.2）。
- **per-thread 火焰图归属**：v1 全局；per-thread 分图延后。
- **JIT 内联段采样盲区**：safepoint 只在 backward-branch/call；紧内联循环采不到。
- **原始样本序列 / 时间轴**：只存 folded 聚合，不留时序（perfetto 需要时再加）。
