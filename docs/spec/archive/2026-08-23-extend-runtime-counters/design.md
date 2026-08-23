# Design: 扩展运行时计数（P1a）

## Architecture

```
                     mutator threads
   builtin_calls ─┐   native_calls ─┐   exceptions_* ─┐
                  ↓                  ↓                 ↓
            RuntimeCounters (AtomicU64, per VmCore, Relaxed add)
                  │ snapshot()
                  ↓
   ┌──────────────────────────────────────────────┐
   │  app.rs 输出点 (--print-stats-on-exit)          │
   │   counters().snapshot()  +  heap().stats()      │
   │            └──────┬───────────┘                 │
   │              ProfileSnapshot                    │  ← 合并视图
   │        text (Display)  |  to_json()             │
   └──────────────────────────────────────────────┘
                  ↑ HeapStats.{allocations, minor/major_collections, reclaimed_bytes}
            arc_heap.rs 分配路径 & 收集周期 (i.stats.* += …)
```

`RuntimeCounters` 与 `HeapStats` 是**两个独立的计数所有者**（前者 per-VmCore、后者 per-heap）。
P1a 不合并它们，只在**输出点**（app.rs）把两者的快照组装成一个 `ProfileSnapshot` 供展示/序列化。

## Decisions

### Decision 1: allocations —— surface 既有 HeapStats 字段，不在 RuntimeCounters 重复计数

**问题：** profile 需要"分配次数"。`HeapStats.allocations` 已存在且在 `arc_heap.rs:723` 每次分配
bump；memory 早先的 P1 设想是"给 RuntimeCounters 加 allocations 并在分配路径 bump"。

**选项：**
- A（新增 RuntimeCounters.allocations + 分配路径再 bump 一次）：分配热路径多一个原子写；两处计数
  同一事件 → 双源、可能漂移、违反单一真相源。
- B（surface 既有 HeapStats.allocations）：零额外热路径成本；HeapStats.allocations 是唯一 SoT。

**决定：** 选 **B**。符合"根因/最本质方案"——分配计数的 SoT 已在 HeapStats，profile 输出点去读它即可，
绝不为"放进同一个快照结构"而在热路径重复 bump。RuntimeCounters 只负责它独有的 native/exception/builtin/jit
计数。

### Decision 2: 合并快照用 ProfileSnapshot，counters.rs 不依赖 gc 模块

**问题：** JSON/text 输出既要 counter 字段又要堆派生字段，如何组织不引入耦合？

**选项：**
- A（在 app.rs 手写拼 JSON 字符串）：散落、难单测、易和 P0 的 `z42vm_counters` 契约漂移。
- B（`ProfileSnapshot` 结构 + `to_json()`/`Display`，字段为纯 u64，在 app.rs 从 `Snapshot` +
  `HeapStats` 组装）：单点序列化、可单测；counters.rs 不 import gc（字段传值，无类型依赖）。

**决定：** 选 **B**。`ProfileSnapshot { counters: Snapshot, allocations, minor_collections,
major_collections, reclaimed_bytes: u64 }` 定义在 `counters.rs`，`to_json()` 输出 P0 全部键 +
新键（同一 `z42vm_counters` sentinel，超集）。app.rs 在快照点 `ProfileSnapshot::from(snap, heap_stats)`
组装。保持 P0 的 `Snapshot`/`Snapshot::to_json` 不变（内部仍可单独用），ProfileSnapshot 是其超集包装。

### Decision 3: gc_cycles 合计保留，minor/major/reclaimed 为新增并列字段

**问题：** 加了 minor/major 后 gc_cycles 是否冗余 / 是否 minor+major == gc_cycles？

**决定：** `gc_cycles` **语义不变、保留**（= 所有收集调用次数，含 force_collect / 非分代路径）。
minor/major 是 generational 收集器的分代细分，`minor+major` 不保证等于 gc_cycles（force_collect
等路径只 bump gc_cycles）。三者并列、各自独立 bump。避免破坏既有 `gc_cycles` 依赖（safepoint_tests、
Std.GC）。

### Decision 4: 埋点点 = 既有 event-fire helper 就地并列

**问题：** native/exception 计数埋在哪最不易漂移？

**决定：** exception 计数并列到 `interp/mod.rs` 现有的 `ExceptionThrown`/`ExceptionCaught`
**event-fire helper** 内（1013 / 1026 行）——事件与计数同一触发点，语义天然一致，未来不会"事件涨了
计数没涨"。native 计数埋在 `exec_native::call_native` 入口（`CallNative` 唯一派发路径）。

## Implementation Notes

- **原子性**：新增/填实的 counter 用 `AtomicU64` + `Ordering::Relaxed`（与 P0 一致，观测-only，不驱动
  控制流）。HeapStats 三字段在 `Inner`（已被 `Mutex`/单线程收集器保护的 stats 结构）里普通 `+=`，
  与既有 `gc_cycles`/`allocations` 同一保护域。
- **收集周期埋点**：`arc_heap.rs` 现有 4 处 `i.stats.gc_cycles += 1`（2331/2405/2430/2458）+
  minor/major escalation 逻辑（2378–2399）。minor bump 于 `run_cycle_collection_minor` 实际执行处、
  major bump 于 `run_cycle_collection_major`、`reclaimed_bytes += freed_bytes` 于周期结算处。实施时
  逐点核对 2320–2460，保证每个收集路径都归类到 minor 或 major（力求不漏、不重）。
- **app.rs 输出点**：`ctx.heap()`（vm_context.rs:1301，返回 `&dyn MagrGC`）→ `.stats()`
  （heap.rs:470）。在 `if opts.print_stats` 块内、`vm.run` 之后 ctx 仍存活，安全。
- **scraper 后向兼容**：xtask_profile.z42 P0 已按键名解析 `z42vm_counters` JSON；新增键只是"多读几个"，
  旧字段位置/名称不变。
- **counters.rs 行数**：现 ~230 行（含内联 tests mod）。加 ProfileSnapshot 后接近软限 300；按
  runtime-rust.md，tests 拆到 `counters_tests.rs`（本 change 顺带做，实现文件末尾 `#[cfg(test)] mod
  counters_tests;`），实现主体保持 < 300。

## Testing Strategy

- **单元测试**（`counters_tests.rs`）：`ProfileSnapshot::to_json` 含全部键 + `z42vm_counters` sentinel +
  单行；从 Snapshot + 堆字段组装正确。
- **单元测试**（`gc/heap_tests.rs`）：HeapStats 三新字段默认 0；构造后可读回。
- **VM 单测**（可选，`gc` 现有 test 基座）：跑一轮 generational 收集，断言 minor/major/reclaimed 变化。
- **~~e2e golden~~ → 改为经验验证 + Rust 集成测试**（2026-08-23 调整）：e2e golden harness
  （`xtask test e2e`）只 diff **stdout**、且不设 `--print-stats-on-exit`，无法断言 stderr 上的计数
  JSON —— 故放弃 golden，改为：① **Rust 集成测试**覆盖收集器计数（`config_stats.rs`
  major_collections、`safepoint_tests.rs` generational minor）；② **经验验证**（开发期，非提交）：
  seed z42c 编 throw+catch+分配脚本 → 新 cargo z42vm `--print-stats-on-exit --stats-format=json`，
  确认 `exceptions_thrown/caught`、`allocations` 非零且新键齐全（已跑：exc=5/5、allocations=200046、
  4 个新键均在）。
- **GREEN**：`cargo build --manifest-path src/runtime/Cargo.toml --release` + `cargo test --lib`
  （counters / gc）+ 完整 `xtask test`（e2e/cross-zpkg/stdlib/compiler/vscode-syntax）。
- **性能**：native/exception 埋点在冷/中频路径（异常、FFI），分配计数复用既有 bump（零新增）；不预期
  可测回归。热路径 perf 风险集中在 P1b（Mutex 探针），本 change 无。
