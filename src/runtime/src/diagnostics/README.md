# diagnostics/

## 职责

持有同一 VM 的共享诊断状态：已退出线程计数、运行时观察者、暂停分布、锁争用统计和 可选分析服务。
`VmCore.diagnostics` 组合这些字段；每个字段沿用自己的同步方式，组合层不增加锁。

## 功能索引

| 入口 | 职责 |
|---|---|
| `counters.rs` | 每线程原子计数、聚合快照与格式化 |
| `sampling.rs` | 门面注入的采样接口、关闭快路与输出委托 |
| `observer.rs` | 运行时事件、观察者注册及回调快照 |
| `mod.rs` 的 `DiagnosticsState` | 六个共享诊断字段与其生命周期 |
| `VmContext::counters_snapshot` | 在同一线程注册表锁下汇总存活与已退出线程计数 |
| `VmContext::add_runtime_observer` / `fire_runtime_event` | 观察者注册与事件发布 |

## 基础用法

通过 `VmContext` 的观察 API 获取计数和注册观察者。线程销毁时先在注册表锁下转移计数，再移除注册，
快照不会重复或遗漏这份计数。采样接口由 `sampling.rs` 定义，门面在开启采样时或首次堆分析请求时装配独立组件；默认不创建分析实现，关闭时只检查不可变布尔值，不进入动态派发。具体分析状态随 VM 释放，组件代码保留到进程退出。输出路径与错误直接透传。实现位于独立 `z42-profiler`，GC 观察者仍由堆持有。

## 如何测试验证

仓库根运行 `./xtask test runtime`；定向运行 `diagnostics::`、`safepoint` 和 `profiler` 的 Rust 测试。

## 关联文档

机制见 [运行时诊断](../../../../docs/internals/src/runtime/diagnostics.md)。

## 核心文件

`mod.rs` 持有状态；`counters.rs` 与 `observer.rs` 定义计数和事件类型，对应独立测试文件为 `counters_tests.rs` 与 `observer_tests.rs`。
