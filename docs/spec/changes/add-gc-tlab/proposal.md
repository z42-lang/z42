# Proposal: GC 线程本地分配缓冲（TLAB）——让并行编译真加速

## Why

并行编译框架（`ParallelFor` + `--jobs`，PR #333）已就绪，但实测**越多线程越慢**：workspace build 墙钟
jobs=1→35.8s，jobs=8→53.9s，jobs=24→69s。任何线程数都不快。

根因已诊断实证（非 STW 暂停——`Z42_GC_MODE=concurrent` 几乎无效，已证伪）：**每次 `new` 都抢进程全局锁**——
`region_*` bump 锁 ×1 + `inner` stats 锁 ×4–5。N 个并行 mutator 线程全部争抢同一堆的这几把 `parking_lot::Mutex`，
线程越多锁争用越狠。GC 设计注释直言假设 "1-2 mutators"（`arc_heap.rs:326`）。

不修这个，并行编译框架就是死代码（默认串行、`--jobs` opt-in 永远负收益）。修好后并行默认可翻开，编译器性能
整体提升。

## What Changes

给每个 mutator 线程（`VmContext`）一块**线程本地分配缓冲（TLAB）**：线程在锁下**批量领取整个 chunk**，之后在
自己独占的 chunk 里**零锁 bump** 分配对象，chunk 填满才再抢锁领新 chunk。分配热路径从「每对象 5–6 把全局锁」降到
「每 chunk 一次锁」。

- **Region\<T\> TLAB**（定长：`ScriptObject` / `ArrayObj`）：线程独占整个 256-槽 chunk，本地 bump 填充。
- **VarRegion TLAB**（变长：string / closure / array 数据）：线程独占一个 64KB bump chunk，本地 bump 字节。
  编译器 workload string 分配极重，这一半是并行加速的大头。
- **统计原子化（吸收 option B）**：`HeapStats.used_bytes` / `allocations` 改 `AtomicU64`，`record_alloc` 不再
  抢 `inner` 锁；压力检查 / auto-collect 触发降到 chunk 粒度。
- **safepoint 集成**：STW 握手时每个 mutator retire 自己的 TLAB（发布最终填充量 + 把已填槽位批量并回 region
  元数据），collector 看到一致的堆。
- **翻开并行默认**：TLAB 让 jobs-scaling 转正后，`ParallelConfig` 默认 jobs 从 1 改为 `CpuCount`。

> 本 change 是「先 C 再 A」路线的 **C（TLAB 根治）**；后续独立 change 做 **A（codegen 逃逸 arena）**，进一步
> 削减进入 GC 的临时对象量。A 不在本 Scope。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/gc/tlab.rs` | NEW | `Tlab`（obj/arr/var 三块借来 chunk 句柄）+ `ChunkClaim`/`VarChunkClaim`（裸指针 + 本地 next + high-water） |
| `src/runtime/src/gc/types.rs` | MODIFY | `HeapStats.used_bytes`/`allocations` → `AtomicU64`（option B） |
| `src/runtime/src/gc/region.rs` | MODIFY | `borrow_chunk`/`retire_chunk` API + `free_chunk_pool` + 全死 chunk 回收（`alloc`/sweep/card 逻辑不动） |
| `src/runtime/src/gc/var_region.rs` | MODIFY | `borrow_chunk`/`retire_chunk`（64KB）；`alloc`/sweep 逻辑不动 |
| `src/runtime/src/gc/arc_heap.rs` | MODIFY | stats 原子；`free_chunk_pool` 挂载（如需） |
| `src/runtime/src/gc/arc_heap/alloc.rs` | MODIFY | `record_alloc` 去 inner 锁（原子/retire batch）；分配走 TLAB fast path（带 ctx）；strict_oom 退化逐对象锁 |
| `src/runtime/src/gc/arc_heap/interface.rs` | MODIFY | trait alloc 方法带线程上下文，走 ctx TLAB |
| `src/runtime/src/gc/safepoint.rs` | MODIFY | request_gc_pause 握手中各 mutator park 前 retire TLAB |
| `src/runtime/src/vm_context/types.rs` | MODIFY | `VmContext` 加 `tlab` 字段（3 块活跃 chunk 句柄） |
| `src/runtime/src/vm_context/construct.rs` | MODIFY | TLAB 初始化（`new_with_core`/`new`）；drop 前 retire |
| `src/runtime/src/gc/tlab_tests.rs` | NEW | borrow/retire 等价、并发分配、跨线程引用、chunk 回收、strict_oom 退化单测 |
| `src/runtime/src/gc/region_tests.rs` | MODIFY | borrow/retire/回收 单测（跟 region.rs 同步） |
| `src/runtime/src/gc/var_region_tests.rs` | MODIFY | var borrow/retire 单测 |
| `src/compiler/z42c.semantics/src/ParallelFor.z42` | MODIFY | `ParallelConfig` 默认 jobs 1 → `CpuCount`（TLAB 落地并测正后） |
| `docs/book/src/runtime/gc.md`（或对应机制页） | MODIFY | TLAB / chunk 独占机制（borrow/retire/回收 + safepoint + mermaid）+ Deferred（slot 级复用） |
| `docs/roadmap.md` | MODIFY | Deferred Backlog Index 加「TLAB slot 级复用」索引行 |
| `src/runtime/src/gc/README.md` | MODIFY | 功能索引 + 核心文件（新增 tlab.rs） |
| `docs/book/src/SUMMARY.md` | MODIFY | 若新建 book 页需挂入 |

> **GC 遍历侧零改动**：`collect.rs`/`generational.rs`/`observe.rs`/`debug.rs` 对 `region_*` 的 90 处访问**不在 Scope**
> ——chunk 独占保持单 region，sweep/mark/分代/card table 全不动。这正是相对私有 Region 的风险收敛。

**只读引用**：

- `src/runtime/src/interp/exec_object.rs` / `exec_array.rs` / `exec_value.rs` / `exec_call.rs` — 分配 callsite，理解 TLAB 如何接入
- `src/runtime/src/jit/helpers/{object,array,closure}.rs` — JIT 分配 callsite
- `src/runtime/src/corelib/threading.rs` — 线程/VmContext spawn 模型
- `docs/spec/archive/2026-08-29-parallelize-package-compile/` — 并行框架背景 + 实测数据

## Out of Scope

- **option A（codegen 逃逸 arena）**：独立后续 change。
- **无锁 mark_queue / 并发标记默认化**（GcMode::ConcurrentMarkSweep 优化）：本 change 不动 STW 模型本身，只优化
  分配路径；concurrent 模式下的 TLAB 写屏障纪律按现有 barrier 机制沿用。
- **strict_oom 模式的 per-object 精确 refund**：strict 模式下 TLAB 退化为逐对象锁路径（见 design D6），不追求
  strict 模式的并行加速。

## Open Questions

- [x] D1：隔离单位 → **chunk 独占**（线程借共享 region 的整块 chunk 本地 bump；单堆单 GC，GC 遍历/card table 零改动）。
  User 定（先选私有 Region，读 card table 实现后改回 chunk 独占——GC 侧侵入小一个量级、同等性能）。
- [x] 拆 PR 粒度 → **单 change 分 5 阶段、最后一个 PR 落地**。User 定。
- [x] slot 级 free_list 复用 → chunk 级回收先做，slot 级 **Deferred**（D7）。
