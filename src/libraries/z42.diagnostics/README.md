# z42.diagnostics

## 职责
`Std.Diagnostics` 命名空间——运行时可观测性 stdlib。三个面：**日志门面**（`Log`，level filter，
输出 stderr）、**堆保留诊断**（`Heap`，「某对象为什么活着 / 被谁钉住」反向图查询）、**运行时计数**
（`RuntimeStats.Counters()`，把 VM 的 counter + 堆派生快照暴露给 z42 脚本）。

**不包含**：日志 sink 路由（文件 / syslog / OTel）、JSON 日志格式化、async / batch buffering；
完整引用链（堆诊断 L3）；并发探针 / 采样 profiler（脚本性能分析程序 P1b/P2，另库/另面）。

## 功能索引
| 功能 | 入口 / 文件 |
|------|-----------|
| 全局日志（TRACE…ERROR + level filter + logfmt 字段） | `Log.z42` 的 `Log.Info(...)` / `Log.*(msg, LogFields)` |
| 结构化日志字段 builder | `LogFields.z42` 的 `.Add(k, v)` |
| 堆直接引用者查询（L1） | `Heap.z42` 的 `Heap.DirectReferrers(obj) → Retainer[]` |
| 堆保留根查询（L2，类别级 GC 根） | `Heap.z42` 的 `Heap.RetainingRoots(obj) → RootRef[]` |
| 运行时计数快照暴露给脚本 | `RuntimeStats.z42` 的 `RuntimeStats.Counters() → RuntimeCounters` |

## 基础用法

```z42
using Std.Diagnostics;

// 日志
Log.SetMinLevel(LogLevel.DEBUG);              // 默认 INFO
Log.Info("server ready on port " + port.ToString());

// 堆保留诊断（诊断非热路径，每次查询先触发一次 full GC）
Retainer[] refs = Heap.DirectReferrers(suspect);
RootRef[]  roots = Heap.RetainingRoots(suspect);

// 运行时计数（脚本自省 alloc / GC / 异常 / JIT 开销，无需外部 scrape stderr JSON）
RuntimeCounters c = RuntimeStats.Counters();
Console.WriteLine("allocations so far: " + c.Allocations.ToString());
```

## 如何测试验证

```bash
xtask test stdlib z42.diagnostics    # 本库全部 [Test]（log / heap / runtime counters）
```

RuntimeStats.Counters 的投影单测（Rust 侧 append-only 注册）：`cargo test --manifest-path
src/runtime/Cargo.toml --release --lib diagnostics`。

## 关联文档
- 日志设计：`docs/design/stdlib/diagnostics.md`（Deferred 段）
- 堆保留诊断：change `add-heap-retention-diagnostics`（已归档）
- 运行时计数暴露：change `expose-diagnostics-counters`（脚本性能分析 P1c）；机制见
  `docs/design/runtime/diagnostics.md` §5
- 计数来源（VM 侧）：`src/runtime/src/counters.rs`（`RuntimeCounters`/`ProfileSnapshot`）、
  `src/runtime/src/corelib/diagnostics.rs`（`__diag_counters` builtin）

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/LogLevel.z42` | `LogLevel` static — TRACE/DEBUG/INFO/WARN/ERROR 常量 + `Name(int)` |
| `src/Log.z42` | `Log` static — `Trace/Debug/Info/Warn/Error/SetMinLevel/GetMinLevel/IsEnabled` + 每 level 一个 `(msg, LogFields)` 重载 |
| `src/LogFields.z42` | `LogFields` builder — chainable `.Add(key, value)` → logfmt 后缀（自动 escape）|
| `src/Heap.z42` | `Heap` static — `DirectReferrers`（L1）/ `RetainingRoots`（L2）堆反向图查询 |
| `src/Retainer.z42` / `src/RootRef.z42` / `src/RootKind.z42` | 堆诊断结果对象（VM-written）|
| `src/RuntimeStats.z42` | `RuntimeStats` static — `Counters()` 运行时计数快照入口（`[Native("__diag_counters")]`）|
| `src/RuntimeCounters.z42` | `Counters()` 返回类型 — 11 只读 auto-property（7 counter + allocations + 3 分代 GC）|

## 依赖关系
`z42.core`（基础类型）+ `z42.io`（`ConsoleError` + `Ansi` 颜色）+ `z42.time`（ISO8601 时间戳）。
