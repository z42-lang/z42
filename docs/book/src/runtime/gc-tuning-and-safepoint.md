# GC 调参与自动回收 / safepoint 协议

> 对齐：2026-08-24（change `add-gc-tuning-config`，落地 runtime_review §M3 GC 调参 + §M6 safepoint 协议）。
> 代码：`src/runtime/src/config.rs`（knob）、`gc/arc_heap/alloc.rs`（阈值触发）、
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
| `Z42_GC_NEAR_LIMIT_RATIO` | 0.90 | heap-used 达 max-bytes 上限的此比率 → 触发自动回收 + 发 `NearHeapLimit` 事件 | `arc_heap/alloc.rs` |
| `Z42_GC_PRESSURE_RATIO` | 0.75 | heap-used 落在 `[pressure, near)` 区间 → 发 `AllocationPressure` 事件（应低于 near-limit 比率） | `arc_heap/alloc.rs` |
| `Z42_GC_THROTTLE_RATIO` | 0.10 | 距上次自动回收，heap-used 至少再增长 max-bytes 的此比率，才允许下一次自动回收（去抖，防连发） | `arc_heap/alloc.rs` |
| `Z42_GC_MINOR_THRESHOLD` | 0.75 | minor GC 后年轻代存活比率高于此 → 下次回收立即升级 major | `arc_heap` |
| `Z42_GC_SOFT_THRESHOLD` | 0.80 | 堆压力比率高于此 → `SoftHandle` 弱引用变为可回收 | `gc/soft_registry.rs` |
| `Z42_GC_PAUSE_WINDOW` | 1024 | per-heap 滚动 pause-time 队列容量（entries），clamp 到 `[1, 65536]` | `gc/types.rs` |
| `Z42_SAFEPOINT_THROTTLE` | 1024 | 每线程 safepoint 快路径计数；每 N 次才走真 Mutex 轮询。`1` = 禁节流 | `gc/safepoint.rs` |
| `Z42_GC_MODE` | `stw-mark-sweep` | GC 算法：`stw` / `concurrent` / `generational` | `gc/mode.rs` |

> 三个比率各自独立 clamp 到 `[0,1]`，**不强制跨 knob 排序**（若把 pressure 设得高于 near，
> pressure-事件分支自然变死代码，无害）——保持每个 knob 独立可预测，不做"惊喜"式静默改写。

比率的三处消费点（都在 `alloc.rs`）：
- `maybe_auto_collect`：near-limit（触发）+ throttle（去抖）；
- `check_pressure`：near-limit（发 `NearHeapLimit`）+ pressure（发 `AllocationPressure`）；
- `maybe_reset_near_limit_warned`：near-limit（回收后 used 降到阈值下 → 复位事件闩，使下次跨阈值能再发）。

三处共用 `runtime_config().gc_near_limit_ratio` 同一比率——保证「发事件」与「复位事件闩」用同一阈值、不错位。

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

## 刻意不做：`PROMOTION_THRESHOLD` 不入 config

runtime_review §M3 曾把「晋升阈值 2」列为候选 knob，复核后**刻意不做**，判据同 §M3/M4/M5 的
「无消费者 / 得不偿失即不做」（philosophy.md 最简实现）：

1. **热路径成本**：`PROMOTION_THRESHOLD`（`gc/region.rs`）现为编译期 `const u8`（零成本立即数）。
   它被 `gc/arc_heap/generational.rs::maybe_mark_cross_gen_card`——即 **write barrier override**——每次堆引用写读取。
   改成 `runtime_config()` 读取会给每次引用写注入一次原子 load，为一个几乎无人运行时调的值付热路径代价。
2. **结构不变量而非运营旋钮**：晋升阈值是分代 GC 的结构参数，约 20 处测试以 `for _ in 0..PROMOTION_THRESHOLD`
   的形式把它当编译期常量硬编码——只有它固定才成立。它不是「运营期按 workload 调」的旋钮。

真出现「按 workload 调晋升代数」的需求时，应做成**构造期**（per-heap 一次读取、缓存进 heap 字段），
而非 write-barrier 热路径的运行时读——留待真实需求出现时再评估。

## 关联

- [interp-jit-semantics.md](interp-jit-semantics.md)：safepoint check 在 interp / JIT 的插桩点。
- [heap-diagnostics.md](heap-diagnostics.md)：回收后的堆保留诊断。
- `docs/spec/archive/2026-05-20-add-gc-safepoint/design.md`：safepoint 协议原始设计（Decision 5 = JIT 插桩）。
