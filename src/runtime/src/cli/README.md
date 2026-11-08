# cli — 命令行入口

## 职责

z42vm 二进制参数解析和进程启动，调用共享应用入口。

## 功能索引

| 文件 | 职责 |
|---|---|
| `main.rs` | Clap 参数与执行分派 |
| `startup.rs` | 库目录、日志、panic hook 与构建信息 |
| `main_tests.rs` | 参数和启动决策测试 |

## 基础用法

通过 `z42vm --help` 查看命令行入口；二进制由 Cargo 的显式 bin 路径构建。

## 如何测试验证

运行 `./xtask test` 与 `./xtask test runtime`，涵盖对应 Rust 测试与端到端行为。

## 关联文档

- [运行时架构](../../../../docs/internals/src/runtime/vm-architecture.md)
- [组件化运行时](../../../../docs/internals/src/runtime/componentized-runtime.md)

## 核心文件

`main.rs` 是二进制入口，`startup.rs` 提供进程初始化。
