# host — 共享宿主服务

## 职责

本目录是唯一运行时包 `z42` 的宿主模块，不单独编译。组合 Rust 接口与 C ABI，共用运行时服务；静态组件登记归 components，原生库登记归 loader。
命令行参数和启动环境由 Core 的 cli feature 下私有 main_command/main_startup 处理，不经 host 导出。main 只加载 Core 的进程入口；命令行实现通过本目录的加载/执行接口使用 VM。

为命令行、Rust 嵌入端和外部 C 宿主提供统一加载、初始化与调用服务。CLI 负责参数和进程策略，C ABI 负责指针、句柄和状态码；产物解析、GC 和执行语义由各底层子系统实现。

普通宿主使用加载、调用、配置、句柄和回调等功能接口，不导出组件管理类型和按 ID 查询。
`LoadOptions` 只表达功能请求与加载环境；组件装配输入保持 VM 内部。
VM 内部组件门面、跨库协议与普通宿主契约的区别及当前缺口见[运行时模块边界](../../../../docs/internals/src/runtime/module-boundaries.md)。

普通宿主的 C 声明位于 [`../../include/z42_host_api.h`](../../include/z42_host_api.h)，由 `include/z42_host.h` 统一聚合发布；C ABI 的 Rust 实现收在 `c/` 子目录，经 host 根再导出。

## 功能索引

| 入口 | 职责 |
|---|---|
| `LoadOptions.allow_profiling` / `prepare_profiler` / `profiler_loaded` / `write_heap_snapshot` | 分析能力许可、按需准备、状态查询和快照输出 |
| [`../components/`](../components/README.md) | VM 内部能力门面、工厂加载、后端选择和实例管理 |
| `LoadedProgram` / `LoadOptions` | 文件或字节加载、宿主 VFS 与 resolver、入口解析、模式选择、重复调用及初始化状态 |
| `execution_available` | 按执行模式查询可用性，不暴露组件身份，不装库或创建 VM |
| `load.rs` | 宿主 resolver、依赖准备和统一 VM 装配请求；产物机制由 loader 提供 |
| `run_app` / `RunOpts` | 一次性应用运行、应用参数、退出报告和进程退出时的释放策略 |
| `c/ffi.rs` / `c/config.rs` / `c/state.rs` | C ABI 与不透明句柄定义、配置验证及句柄注册表；Rust 消费者从 host 根导入 Z42Host/Z42Module/Z42Entry |
| `c/ops.rs` / `c/marshal.rs` | ABI 操作适配和参数、返回值转换 |
| `ExecMode` / `PackageImage` | 由 z42-zpkg 拥有、通过宿主门面提供的执行模式和不可变包定义类型 |
| `signal.rs` | 致命信号安装、VM 栈捕获和崩溃报告，底层信号原语由 PAL 提供 |
| `resolver.rs` | 宿主包解析接口、C 回调适配和 `read_zpkg_namespaces` 包解析键查询 |
| `services.rs` / `app_state.rs` | 核心库隔离运行服务和每 VM 应用参数 |

## 基础用法

CLI 和 Rust 调用方使用 `host::run_app`，或用 `LoadedProgram::load_file` / `load_bytes` 创建长期存活的程序后调用 `invoke`。两种来源共享产物准备流程。VFS、搜索目录、包 resolver 和依赖预加载策略由 `LoadOptions` 明确传入。
一次性调用的 `RunOpts` 只接收模式、VFS、搜索目录、resolver 等功能输入，在 host 内部转换为加载请求，不携带组件管理状态；run_app 不读取 CLI 搜索配置或隐式补默认目录。main 解析环境变量、相对路径和 stdlib 路径；C/Rust 便利适配可选择各自的进程配置策略，浏览器显式传入页面 VFS 与目录。

需要进程崩溃报告的宿主调用 `host::signal::install()`，CLI 在进程启动时安装。

静态装配由最终组合根显式完成，不属于普通 host 接口。协议类型从 `z42::components::abi` 获取。Rust 静态宿主先调用 `unsafe components::abi::register_static_component(id, init)` 登记已链接的独立后端工厂；重复相同登记幂等，冲突替换报错。工厂有效期为进程寿命，VM 实例仍独立创建和释放；组件登记返回组件状态，不使用 host 错误槽。C 静态宿主通过统一 native 头 `z42_host.h` 可调用 `z42_component_register_static` 登记所链接的工厂，默认解释器组合仍可调用 `z42_linked_components_init`。压缩库另由 `z42_native_register_static` / Rust 的 `loader::linked::register_static_library` 登记；组件链接与登记均由最终宿主决定。动态 SDK 无须这一步，host 按搜索路径发现组件。

`LoadOptions` 只接收执行模式、原生来源许可/搜索目录、诊断许可和包加载环境。`native` 的三个 allow 字段区分静态、共享链接和运行时装入；`allow_profiling=false` 拒绝可选分析能力，普通执行不提前加载 profiler。组件提供方选择与实例装配归 VM 内部，不经 host 公开管理类型。

一个程序的初始化状态跨调用保留；驱动同一上下文时遵循单执行线程约束。保留托管返回值跨越后续 VM 调用时，调用方须登记 GC 根。

## 如何测试验证

```bash
(cd src/runtime && cargo test --lib host:: -- --test-threads=1)
(cd src/runtime && cargo test --lib --no-default-features host:: -- --test-threads=1)
./xtask test
./xtask package sdk --verify
./xtask package runtime --linkage dynamic --profile debug --verify
```

## 关联文档

- [嵌入机制](../../../../docs/internals/src/runtime/embedding.md)
- [嵌入 C ABI](../../../../docs/reference/src/embedding/c-abi.md)

## 核心文件

`program.rs` 实现程序加载与调用服务，`c/ffi.rs` 实现 C ABI。`program_tests.rs` 覆盖输入来源、重复调用、初始化错误和执行模式；`host_tests.rs` 覆盖 ABI 契约。

核心 builtin 描述由 corelib/builtins.rs 提供，interop 通用注册器冻结匹配的查询目录和执行表；host 不列举或预登记独立库。vm/extensions.rs 注入每 VM 状态和 loader 的按名称路径服务。独立库首次限定模块调用才经 loader 读取库自身描述并由 interop 校验登记，未使用时不加载或创建状态。

profiler 默认按需装配。CPU 采样开启时在 VM 发布前验证，堆分析首次使用时验证；实例与来源归 components，diagnostics 通过 VM 注入的准备回调消费分析能力。缺失时明确报错，不返回空分析结果。
