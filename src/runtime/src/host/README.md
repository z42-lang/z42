# host — 运行时宿主入口

## 职责

这里提供一次性应用运行、Rust 嵌入接口和 C ABI。Rust 和 C 共用进程级嵌入会话，每个加载的程序拥有独立的 VM。运行时根的 `main_startup.rs` 和 `main_command.rs` 负责 CLI 参数与启动环境。VM 管理程序生命周期，加载器准备产物与依赖，执行层调用入口，诊断模块生成退出报告。

## 功能索引

| 功能 | 入口 |
|---|---|
| 一次性运行应用 | `host::run_app` 和 `RunOpts`。调用方提供 VFS、搜索目录、执行模式和应用参数 |
| Rust 嵌入 | `host::rust::{Host, HostConfig, HostError, MapResolver, SearchPathsResolver}` |
| C 嵌入 | [`z42_host.h`](../../include/z42_host.h) 声明会话、句柄、回调与状态码。宿主统一包含 [`z42.h`](../../include/z42.h) |
| 直接加载并持有程序 | `host::{LoadOptions, LoadedProgram, execution_available}`。程序实现见 [`vm/program.rs`](../vm/program.rs) |
| 包解析 | `host::resolver::{ZpkgResolver, read_zpkg_namespaces}`。loader 读取解析键，C 入口负责指针、回调和线程局部错误 |
| 崩溃报告和 profiler | `host::signal::install`。profiler 的准备、查询和快照函数由 [diagnostics](../diagnostics/README.md) 提供，经宿主根导出 |

## 基础用法

一次性运行通过 `host::run_app` 消费显式的 `RunOpts`，设置应用参数、调用入口并报告退出。
C 和 Rust 嵌入入口共用默认路径与应用侧车的准备逻辑。各程序按所属 VM 的选项报告退出。

需要多次调用时，Rust 使用 `Host::new → load_zbc → resolve_entry → invoke`，由 `Host` 的 Drop 关闭会话。C 使用 `z42_host_initialize → z42_host_load_zbc → z42_host_resolve_entry → z42_host_invoke → z42_host_shutdown`。关闭使会话发出的句柄失效，重新初始化会建立新的会话。

静态宿主通过 [components](../components/README.md) 登记链接的组件工厂。分发头 `z42.h` 汇总宿主和其他模块的 C 声明。

## 如何测试验证

```bash
(cd src/runtime && cargo test --locked -p z42 --lib host:: -- --test-threads=1)
(cd src/runtime && cargo test --locked --no-default-features -p z42 --lib host:: -- --test-threads=1)
(cd src/runtime && cargo test --locked -p z42 --lib vm::program_tests:: -- --test-threads=1)
./xtask test
./xtask package sdk --verify
./xtask package runtime --linkage dynamic --profile debug --verify
```

宿主测试覆盖 ABI、会话生命周期、回调重入和一次性运行。VM 程序测试覆盖加载来源、入口执行和重复调用。

## 关联文档

- [嵌入 C ABI 契约](../../../../docs/reference/src/embedding/c-abi.md)
- [嵌入宿主实现](../../../../docs/internals/src/runtime/embedding.md)
- [运行时模块边界](../../../../docs/internals/src/runtime/module-boundaries.md)

## 核心文件

| 文件 | 职责 |
|---|---|
| [`mod.rs`](mod.rs) | 宿主根导出，包括 VM 程序接口及 `resolver`、`signal` 模块 |
| [`app.rs`](app.rs) | 一次性应用运行，串起加载、应用参数、入口调用和退出报告 |
| [`rust.rs`](rust.rs) | 安全 Rust 宿主接口、类型化错误与内置包解析器 |
| [`ffi.rs`](ffi.rs) | C 导出函数、指针校验与不透明句柄 |
| [`abi_config.rs`](abi_config.rs) | C 配置布局、校验和回调适配 |
| [`session.rs`](session.rs) | 共享会话、句柄注册表和加载、解析、调用事务 |
| [`marshal.rs`](marshal.rs) | 宿主值与运行时值转换 |
| [`error.rs`](error.rs) | 宿主状态码和 C API 的线程局部错误 |
| [`app_tests.rs`](app_tests.rs) | 一次性运行与退出报告 |
| [`embedding_tests.rs`](embedding_tests.rs) | 嵌入接口和共享会话 |
