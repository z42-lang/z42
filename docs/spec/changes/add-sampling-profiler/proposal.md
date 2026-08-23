# Proposal: safepoint 采样 profiler —— z42 调用栈火焰图

## Why

脚本性能分析程序已有 CPU（samply，**native** 栈）/ heap（dhat）/ 并发（park+contention）/ e2e（wall+RSS）
+ counter 面，但缺一块：**z42 层的 CPU 火焰图**。samply 给的是 **native**（Rust/JIT machine）栈——对定位
「哪个 z42 函数吃了 CPU」不直观（JIT code 的符号是 mangled machine frame，interp 更是全在 dispatch loop 里）。
需要一个按 **z42 源函数**聚合的采样火焰图：`Main;foo;bar` 这样的 z42 调用栈 + 采样计数。

不做的后果：`xtask profile --cpu` 只能看 native 火焰图，z42 用户看不到「我的哪个函数热」。

## What Changes

**safepoint 采样 profiler**（复用现有协作式 safepoint 轮询，零信号/ptrace）：

1. **采样基建**（runtime）：`VmCore` 挂一个 `Sampler`——一个后台**定时线程**按 `Z42_SAMPLE_HZ`（默认关）
   的频率置一个 `sample_pending: AtomicBool`；mutator 在 `check_safepoint_slow`（Idle 路径）见 flag 时
   **快照当前 z42 调用栈**（走 `ctx.call_stack` 的 `VmFrame.func_name`），拼成 folded stack 字符串
   累加进 `VmCore.sampler` 的 `HashMap<folded, count>`，清 flag。**默认关时零成本**（无后台线程、flag 永不置、
   `check_safepoint_slow` 一次 atomic load）。
2. **退出时输出**（app.rs）：运行结束把累加的 folded stacks 写到 `Z42_SAMPLE_OUT`（默认 `z42-samples.folded`），
   格式 `frame1;frame2;frame3 <count>` 每行——inferno / flamegraph.pl 的标准输入格式。
3. **xtask profile --cpu 增强**：现有 samply（native）保留；新增 **z42-level 火焰图**——用 `Z42_SAMPLE_HZ`
   跑一遍收 folded stacks，有 `inferno` CLI 则渲成 SVG，否则落 `.folded` 文件 + 查看提示（镜像 `--heap` 的
   dhat 产物模式）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/gc/sampler.rs` | NEW | `Sampler` 结构：后台定时线程 + `sample_pending` flag + folded-stack 累加器 + 输出 |
| `src/runtime/src/vm_context.rs` | MODIFY | `VmCore` 加 `sampler: Sampler`；`new_internal` 按 `Z42_SAMPLE_HZ` 启动 |
| `src/runtime/src/gc/safepoint.rs` | MODIFY | `check_safepoint_slow` Idle 路径：见 `sample_pending` → 快照 call_stack → 累加 |
| `src/runtime/src/gc/mod.rs` | MODIFY | `pub mod sampler;` |
| `src/runtime/src/config.rs` | MODIFY | `KNOWN_KNOBS` 加 `Z42_SAMPLE_HZ` / `Z42_SAMPLE_OUT` |
| `src/runtime/src/app.rs` | MODIFY | run() 结束 flush sampler 的 folded stacks 到输出文件 |
| `src/runtime/src/gc/sampler_tests.rs` | NEW | 采样累加 + folded 格式单测（手动置 flag + 造 call_stack） |
| `scripts/xtask_profile.z42` | MODIFY | `_profileCpu` 加 z42-level 火焰图（inferno / folded 产物） |
| `docs/book/src/runtime/diagnostics.md` | NEW | **知识上浮**：诊断/profiling 机制页（counter/park/contention/采样统一落 book）|
| `docs/book/src/SUMMARY.md` | MODIFY | 挂新页 |
| `docs/design/runtime/diagnostics.md` | MODIFY | §7 采样/trace 标注落地 + 指向 book 新页（迁移期） |
| `src/runtime/src/gc/README.md`（若有） | MODIFY | 功能索引加 sampler（如涉及） |

**只读引用**：`src/runtime/src/exception/mod.rs`（`VmFrame.func_name/line`）、`src/runtime/src/gc/safepoint.rs`
（`check_safepoint`/`throttle_n` 机制）、`counters.rs`（P1a/P1b 快照范式）。

## Out of Scope

- **perfetto / chrome trace（`--trace-out`，diagnostics.md §7）**：不同输出格式（时间线事件，非聚合火焰图）。
  本 change 交付 **folded-stacks 火焰图**（核心价值）；perfetto trace 记 **Deferred**（后续 change）。
- **per-thread 火焰图归属**：v1 全局累加（多线程栈混在一起）；per-thread 归属 Deferred。
- **暴露到 z42 脚本 API**：采样只经 env + xtask，不新增 `Std.Diagnostics` API。
- **JIT-code 采样精度**：safepoint 只在 backward-branch/call 处，JIT 内联/无 safepoint 段采不到——记 Deferred。

## Open Questions

- [x] 采样机制 → DRAFT D5：后台线程置 flag + mutator 在 safepoint 快照（复用协作式轮询）。
- [x] 知识上浮落点 → User 已授权按 book 结构选：新建 `docs/book/src/runtime/diagnostics.md`。
- [ ] **perfetto trace 是否本 change 做**：倾向 **Deferred**（folded 火焰图是核心；perfetto 是另一输出格式、
      工作量翻倍）。**实施前请 User 拍板**：只做火焰图（推荐）vs 火焰图+perfetto 一起。
