# signal_handler — 崩溃报告

## 职责

捕获致命信号并输出运行时栈，平台信号操作由 PAL 提供。

## 功能索引

| 文件 | 职责 |
|---|---|
| `mod.rs` | 信号安装与栈报告 |
| `signal_handler_tests.rs` | 崩溃报告测试 |

## 基础用法

消费者继续从 `z42::signal_handler` 使用原有入口。

## 如何测试验证

运行 `./xtask test` 与 `./xtask test runtime`，涵盖对应 Rust 测试与端到端行为。

## 关联文档

- [运行时架构](../../../../docs/internals/src/runtime/vm-architecture.md)
- [组件化运行时](../../../../docs/internals/src/runtime/componentized-runtime.md)

## 核心文件

`mod.rs` 是该模块的入口，测试保留在独立文件。
