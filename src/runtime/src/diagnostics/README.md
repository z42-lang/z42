# diagnostics/

## 职责

持有同一 VM 的共享诊断状态：已退出线程计数、运行时观察者、暂停分布、锁争用统计和 可选分析服务。
`VmCore.diagnostics` 组合这些字段；每个字段沿用自己的同步方式，组合层不增加锁。

## 功能索引

| 入口 | 职责 |
|---|---|
| `counters.rs` | 每线程原子计数、聚合快照与格式化 |
| `sampling.rs` | 采样状态、惰性后端准备及输出委托 |
| `prepare_profiler` / `profiler_loaded` / `write_heap_snapshot` | Rust 宿主功能入口，宿主根直接导出 |
| `retention.rs` | 堆引用和保活根查询、目标识别与 GC 暂停 |
| `observer.rs` | 运行时事件、观察者注册及回调快照 |
| `exit_report.rs` | 普通返回和托管退出共享的进程级统计输出与采样刷盘 |
| `crash_report.rs` | 登记进程中的 VM Core，通过 PAL 安装信号处理并报告 VM 栈 |
| `mod.rs` 的 `DiagnosticsState` | 六个共享诊断字段与其生命周期 |
| `DiagnosticsState::snapshot` | 在同一线程注册表锁下汇总存活与已退出线程计数 |
| `add_runtime_observer` / `emit_runtime_event` | 给单个 VM 注册观察者和按需构造事件 |

## 基础用法

计数与观察契约通过 `diagnostics::{RuntimeCounters, CounterSnapshot, ProfileSnapshot, RuntimeEvent, RuntimeObserver}` 消费；`counters.rs`、`observer.rs` 是私有实现。

通过 diagnostics 的入口获取计数和注册观察者。退出报告与托管诊断共用 VM 计数和堆统计的采样入口。线程销毁时先在注册表锁下转移计数，再移除注册，
快照不会重复或遗漏这份计数。VM 装配通过 `prepare_sampling` 在发布前准备已开启的采样。堆快照与引用查询经 diagnostics 按需准备分析后端。关闭时 safepoint 只检查不可变布尔值。具体分析状态随 VM 释放，组件代码保留到进程退出。输出路径与错误直接透传。实现位于独立 `z42-profiler`，GC 观察者仍由堆持有。

## 如何测试验证

仓库根运行 `./xtask test runtime`；定向运行 `diagnostics::`、`safepoint` 和 `profiler` 的 Rust 测试。

## 关联文档

机制见 [运行时诊断](../../../../docs/internals/src/runtime/diagnostics.md)。

## 核心文件

`mod.rs` 持有状态；`counters.rs` 与 `observer.rs` 定义计数和事件类型，对应独立测试文件为 `counters_tests.rs` 与 `observer_tests.rs`。
