# diagnostics/

## 职责

持有同一 VM 的共享诊断状态：已退出线程计数、运行时观察者、暂停分布、锁争用统计和 CPU 采样器。
`VmCore.diagnostics` 组合这些字段；每个字段沿用自己的同步方式，组合层不增加锁。

## 功能索引

| 入口 | 职责 |
|---|---|
| `mod.rs` 的 `DiagnosticsState` | 六个共享诊断字段与其生命周期 |
| `VmContext::counters_snapshot` | 在同一线程注册表锁下汇总存活与已退出线程计数 |
| `VmContext::add_runtime_observer` / `fire_runtime_event` | 观察者注册与事件发布 |

## 基础用法

通过 `VmContext` 的观察 API 获取计数和注册观察者。线程销毁时先在注册表锁下转移计数，再移除注册，
快照不会重复或遗漏这份计数。采样器实现位于 `profiler/cpu.rs`，GC 观察者仍由堆持有。

## 如何测试验证

仓库根运行 `./xtask test runtime`；定向运行 `counters`、`observer`、`safepoint` 和 `profiler` 的 Rust 测试。

## 关联文档

机制见 [运行时诊断](../../../../../docs/internals/src/runtime/diagnostics.md)。

## 核心文件

`mod.rs` 持有状态；计数与事件类型分别在 `src/runtime/src/counters.rs` 和 `src/runtime/src/observer.rs`。
