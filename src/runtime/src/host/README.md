# host — 共享宿主服务

## 职责

本目录是唯一运行时包 `z42` 的宿主模块，不单独编译。Rust 接口、C ABI、CLI 实现、静态组件登记与动态进程入口都在这里组合，共用运行时服务。根 `main.rs` 仅通过版本化 C 入口加载运行时。

为命令行、Rust 嵌入端和外部 C 宿主提供统一加载、初始化与调用服务。CLI 负责参数和进程策略，C ABI 负责指针、句柄和状态码；产物解析、GC 和执行语义由各底层子系统实现。

## 嵌入接口导出规范

`host` 是运行时面向宿主的唯一公开门面。所有供嵌入调用的接口和类型都必须在这里提供，包括配置、加载与调用、返回值和错误、句柄、回调，以及可选组件的选择与能力查询。类型的实现可以属于独立组件库，由 `host` 统一再导出；宿主代码只依赖 `z42::host`，不直接依赖 VM、解释器、JIT、profiler 或调试组件的内部模块。

CLI 是宿主的一种实现，也遵循同一入口和类型规范。C ABI 导出与 Rust 接口共用宿主服务，公开 C 类型和函数通过随 SDK 提供的宿主头文件使用。

新增或拆分组件时，先明确组件内部契约与宿主 API，再在 `host` 暴露实际需要的入口；不要把组件模块整体公开作为嵌入 API。静态链接和运行时加载使用相同的宿主接口，组件的注册、加载和寿命由运行时管理。

## 功能索引

| 入口 | 职责 |
|---|---|
| `BackendOptions` / `BackendSource` | 再导出统一组件框架的后端选择与能力查询 |
| [`../components/`](../components/README.md) | 组件协议、工厂加载、后端选择和实例管理的统一实现 |
| `ComponentOptions` / `ComponentLease` | 每 VM 组件配置和接口/库寿命租约 |
| `component_ids` / `component_interface` | 查询已装配组件与类型化接口 |
| `LoadedProgram` / `LoadOptions` | 文件或字节加载、宿主 VFS 与 resolver、入口解析、模式选择、重复调用及初始化状态 |
| `artifact.rs` / `load.rs` | 依赖解析、去重、合并、约束和索引准备 |
| `run_app` / `RunOpts` | 一次性应用运行、应用参数、退出报告和进程退出时的释放策略 |
| `ffi.rs` / `config.rs` / `state.rs` | C ABI 适配、配置验证及宿主句柄注册表 |
| `ops.rs` / `marshal.rs` | ABI 操作适配和参数、返回值转换 |
| `ExecMode` / `PackageImage` | 由组件拥有、通过宿主门面提供的执行策略和不可变包定义类型 |
| `vm_defaults.rs` / `extensions.rs` | 默认 VM 服务与扩展输入；worker 继续共享已有 Core |
| `program_builder.rs` | 可用性裁剪、Core/loader/token 准备、组件装配、事件重放与程序发布 |
| `signal.rs` | 致命信号安装、VM 栈捕获和崩溃报告，底层信号原语由 PAL 提供 |
| `resolver.rs` | 宿主包解析接口、C 回调适配和 `read_zpkg_namespaces` 包解析键查询 |
| `services.rs` / `app_state.rs` | 核心库隔离运行服务和每 VM 应用参数 |

## 基础用法

CLI 和 Rust 调用方使用 `host::run_app`，或用 `LoadedProgram::load_file` / `load_bytes` 创建长期存活的程序后调用 `invoke`。两种来源共享产物准备流程。VFS、搜索目录、包 resolver 和依赖预加载策略由 `LoadOptions` 明确传入。

需要进程崩溃报告的宿主调用 `host::signal::install()`，CLI 在进程启动时安装。

Rust 静态宿主先调用 `unsafe register_static_component(id, init)` 登记已链接的独立后端工厂；重复相同登记幂等，冲突替换报错。工厂有效期为进程寿命，VM 实例仍独立创建和释放。C 静态宿主可调用 `z42_host_register_static_component` 登记所链接的工厂，默认解释器组合仍可调用 `z42_linked_components_init`。压缩库另由 `z42_host_register_static_native_library` / Rust 的 `register_static_native_library` 登记；组件链接与登记均由最终宿主决定。动态 SDK 无须这一步，host 按搜索路径发现组件。

LoadOptions.components 选择可选的同构建组件。ComponentOptions::with_static 保存已链接的 init 引用，with_dynamic 指定库与 C 入口并启用组件；with_static_provider / with_dynamic_provider 仅登记依赖来源，未被启用根依赖时不会加载。以上入口均为 unsafe，调用方保证原生代码和协议可信。加载程序时递归解析启用根的依赖闭包，完成校验、依赖排序和实例创建，失败会回滚，成功后注册表冻结。选项克隆共享工厂结果，每个 VM 仍拥有独立实例。component_interface 返回的租约保留 VM、实例和库，直到最后一次接口调用完成。组件原生代码映像由 core 按路径缓存至进程结束，保证线程退出时的 TLS 析构回调仍有效；VM 和 JIT 实例按正常寿命释放。自定义组件清单默认为空；执行后端由 LoadOptions.backends 按执行模式自动选择并注册，JIT 模式同时要求解释器。Auto 优先静态链接，否则查询动态组件；Linked、Dynamic(path) 和 Disabled 明确限制来源。请求不存在、未链接、被禁用或平台不支持的 JIT 均报错，不静默降级。

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

`program.rs` 实现程序加载与调用服务，`ffi.rs` 实现 C ABI。`program_tests.rs` 覆盖输入来源、重复调用、初始化错误和执行模式；`host_tests.rs` 覆盖 ABI 契约。

原生提供方装配由 `builtins.rs` 选择，`extensions.rs` 注册每 VM 状态和 loader 的按名称路径服务。核心库、终端、压缩分别提交描述符；host 注入匹配的 metadata 查询目录和 exec 执行表。
