# host — 共享宿主服务

## 职责

为命令行、Rust 嵌入端和外部 C 宿主提供统一加载、初始化与调用服务。CLI 负责参数和进程策略，C ABI 负责指针、句柄和状态码；产物解析、GC 和执行语义由各底层子系统实现。

## 嵌入接口导出规范

`host` 是运行时面向宿主的唯一公开门面。所有供嵌入调用的接口和类型都必须在这里提供，包括配置、加载与调用、返回值和错误、句柄、回调，以及可选组件的选择与能力查询。类型的实现可以属于独立组件库，由 `host` 统一再导出；宿主代码只依赖 `z42::host`，不直接依赖 VM、解释器、JIT、profiler 或调试组件的内部模块。

CLI 是宿主的一种实现，也遵循同一入口和类型规范。C ABI 导出与 Rust 接口共用宿主服务，公开 C 类型和函数通过随 SDK 提供的宿主头文件使用。

新增或拆分组件时，先明确组件内部契约与宿主 API，再在 `host` 暴露实际需要的入口；不要把组件模块整体公开作为嵌入 API。静态链接和运行时加载使用相同的宿主接口，组件的注册、加载和寿命由运行时管理。

## 功能索引

| 入口 | 职责 |
|---|---|
| `LoadedProgram` / `LoadOptions` | 文件或字节加载、宿主 VFS 与 resolver、入口解析、模式选择、重复调用及初始化状态 |
| `load.rs` / `boot.rs` | 依赖解析、去重、合并、约束和索引准备、可用性裁剪、构造上下文和链接准备 |
| `run_app` / `RunOpts` | 一次性应用运行、应用参数、退出报告和进程退出时的释放策略 |
| `ffi.rs` / `config.rs` / `state.rs` | C ABI 适配、配置验证及宿主句柄注册表 |
| `ops.rs` / `marshal.rs` | ABI 操作适配和参数、返回值转换 |
| `ExecMode` / `PackageImage` | 由组件拥有、通过宿主门面提供的执行策略和不可变包定义类型 |
| `signal.rs` | 致命信号安装、VM 栈捕获和崩溃报告，底层信号原语由 PAL 提供 |
| `resolver.rs` | 宿主包解析接口、C 回调适配和 `read_zpkg_namespaces` 包解析键查询 |
| `services.rs` / `app_state.rs` | 核心库隔离运行服务和每 VM 应用参数 |

## 基础用法

CLI 和 Rust 调用方使用 `host::run_app`，或用 `LoadedProgram::load_file` / `load_bytes` 创建长期存活的程序后调用 `invoke`。两种来源共享产物准备流程。VFS、搜索目录、包 resolver 和依赖预加载策略由 `LoadOptions` 明确传入。

需要进程崩溃报告的宿主调用 `host::signal::install()`，CLI 在进程启动时安装。

一个程序的初始化状态跨调用保留；驱动同一上下文时遵循单执行线程约束。保留托管返回值跨越后续 VM 调用时，调用方须登记 GC 根。

## 如何测试验证

```bash
(cd src/runtime && cargo test --lib host:: -- --test-threads=1)
(cd src/runtime && cargo test --lib --no-default-features host:: -- --test-threads=1)
./xtask test
./xtask package sdk --verify
```

## 关联文档

- [嵌入机制](../../../../docs/internals/src/runtime/embedding.md)
- [嵌入 C ABI](../../../../docs/reference/src/embedding/c-abi.md)

## 核心文件

`runtime.rs` 实现 Rust 服务，`ffi.rs` 实现 C ABI。`runtime_tests.rs` 覆盖输入来源、重复调用、初始化错误和执行模式；`host_tests.rs` 覆盖 ABI 契约。
