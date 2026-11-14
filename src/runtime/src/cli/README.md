# cli — 命令行宿主适配

## 职责

实现 z42vm 的参数解析、启动环境和进程策略，通过 `host::run_app` / `RunOpts` 消费 VM 接口。
本目录受 `cli` feature 限制，非 CLI 平台库不编译它或其 Clap/tracing-subscriber 可选依赖。
VM 装配、包准备和执行仍由宿主服务及所属子系统提供。

## 功能索引

| 入口 | 职责 |
|---|---|
| `mod.rs::run_from` | 参数与旋钮来源归并、自省命令、宿主请求和退出处理 |
| `process_startup.rs` | stdlib 定位、日志、panic hook、构建信息与搜索目录 |
| `entry_protocol.rs` | 独立 main 与动态运行时共用的版本化 C 进程入口协议 |
| `runtime_entry.rs` | C 参数转换与 panic 边界，将进程请求交给同一 CLI 适配 |

## 基础用法

动态 main 按同构建身份校验 C 入口后传递参数；静态 CLI 示例调用 `z42::cli::run_from`。
普通嵌入端使用 `host::LoadedProgram` 或 `host::run_app`，无需依赖 CLI。host 不转出 CLI 或其专属进程协议。
`z42vm` binary 只在同时启用 `cli` 与 `component-loading` 时由 Cargo 选择。

## 如何测试验证

仓库根运行 `./xtask test runtime`、`./xtask test`、`./xtask package sdk --verify`。
动态进程和静态宿主由实际打包验证覆盖；`cli_tests.rs` 保留参数优先级和环境发布用例，
PAL launcher 测试覆盖入口的头版本、大小、构建身份和回调校验。

## 关联文档

- [组件与进程入口机制](../../../../docs/internals/src/runtime/componentized-runtime.md)
- [运行时模块边界](../../../../docs/internals/src/runtime/module-boundaries.md)
- [宿主接口](../host/README.md)

## 核心文件

`mod.rs` 是 CLI 适配入口；C 协议与实现分别位于 `entry_protocol.rs` 和 `runtime_entry.rs`，
入口不跨动态库边界传递 Rust 容器、trait 对象或 VM 类型。
