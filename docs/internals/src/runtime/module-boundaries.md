# 运行时模块边界

> 代码：`src/runtime/src/`、`src/runtime/crates/`
> 相关：[组件化运行时](componentized-runtime.md)、[VM 总体架构](vm-architecture.md)、[嵌入机制](embedding.md)
> 待办：组件公开面与内部门面尚未收口，默认 VM 装配仍位于 host；目标结构未全部实施。

## 概述

目录已经分开，但可见性、状态所有权和访问入口仍跨越目录边界。组件管理属于 VM 内部设施；
普通宿主表达加载、执行模式和诊断请求，VM 使用统一能力接口，具体组件通过装配适配器接入。
文件名与这些职责对应，消费者不感知实现文件移动或组件改名。

## 设计目标与约束

区分三种接口：普通宿主契约、VM 内部访问门面、同构建组件间协议。它们的受众、版本约束与可见性不同。
`components::api` 的目标是 crate 内部能力门面；当前同名文件实际保存跨库 C 协议，仍需拆分，不能把目标当成已实现。
独立 interp/JIT/profiler crate 所需的最小协议仍需跨 crate 可见，但不通过 host 提供，也不包含通用组件管理实现。

当前实际状态与目标分开描述；实施顺序在仓库 roadmap 中维护。收口保持 GC 根、写屏障、暂停协议、
代码租约、VM/worker 身份及明确请求失败的行为；不靠静默降级、兼容别名或逐指令动态查询消除表面引用。

## 方案与决策

### 当前可核对的缺口

| 缺口 | 代码证据 | 影响与目标 |
|---|---|---|
| host 转出内部组件协议 | `host/mod.rs` 的 `pub use crate::components::api::*`、具名管理类型/查询再导出 | 普通宿主可感知描述符、ID、工厂与租约；从普通宿主面移除 |
| 公开签名传递管理状态 | `host/program.rs::LoadOptions` 的 components/backends/profiler 字段；`context/types.rs::VmCore::components` 返回 OnceLock；`LoadedProgram::context` 可访问 Core | 仅删除再导出无法封闭可达面；将功能请求与 VM 装配输入分开，注册槽由组件所有者维护 |
| C 静态装配混入普通宿主头 | `include/z42_host.h` 的 Z42ComponentId、ComponentInit 与 `z42_host_register_static_component`；`host/ffi.rs` | 静态链接胶水必须同轮迁移到明确的内部部署/装配契约，不能只隐藏 Rust 名字 |
| VM 消费者穿透组件子模块 | `host/program_builder.rs` 调 backends::attach；`diagnostics/sampling.rs` 调 profiling::attach | 通过内部 components::api 准备/查询能力；具体组件路径只留在实现与适配器中 |
| 查询仍要求知道实现身份 | `components/options.rs::component_interface(ctx, ComponentId)`；`components/api.rs::RuntimeApi` 按组件 ID 查询 | VM 消费者按稳定能力/版本/角色请求服务；组件 ID 仅由管理层用于来源、实例与依赖绑定 |
| 名为 api 的文件混合两种接口 | `components/api.rs` 定义 ComponentDescriptor、RuntimeApi、ExecutionApi、ProfilerApi 等跨库表 | VM 访问门面与同构建协议分开命名和拥有，不把跨库表直接作为业务调用面 |
| 组件管理与配置相互依赖 | `config/availability.rs` 查询 components；`components/backends.rs::native_paths/attach` 读取 runtime_config | 在装配入口捕获路径/JIT 配置及平台能力，组件管理消费显式输入 |
| 能力查询可能有装载副作用 | `components/backends.rs::auto_available` 在 Android 库名回落时调用 Library::open_component | 分开支持性、已知候选与已装配能力；只在显式准备时装库，保留实际装配校验 |
| 诊断契约反向拥有装配机制 | `diagnostics/sampling.rs` 保存 BackendSource、VmComponents 并触发 attach；`components/profiling.rs` 回到 sampler 注册实现 | 诊断保留观察/采样接口，组件管理拥有来源和代码保活；经窄接口绑定，避免新增 Core 自持有环 |
| 默认 VM 装配由 host 承担 | `host/program_builder.rs`、`vm_defaults.rs`、`extensions.rs`；`host/artifact.rs` 为 LazyLoader 实现构造方法 | host 解析请求，VM 装配层负责配置快照、Core/loader、组件和发布；loader 的构造实现回归其所有者 |
| loader 与 interop 混合机制和编排 | `loader/native.rs` 调 interop 注册/解析，`interop/module.rs` 反向调用 loader::native::resolve_symbol | loader 提供代码/符号来源，interop 负责描述符与编组；由装配协调器连接 |
| 元数据解析直接驱动 VM 加载 | `metadata/resolve/mod.rs` 接收 VmContext，enqueue_type_load/drain_pending_type_loads | 将解析事实与加载编排分开，用最小解析视图和待加载请求连接 |
| 对象表示反向依赖执行和加载域 | `object/value/ref_word.rs::alloc_box_ambient` 访问 exec::ambient；`object/object/object.rs::NativeData` 引用 loader 身份；`object/types/type_desc.rs` 引用 metadata TypeId | 抽出身份/布局词汇，分离编码与需堆的装箱操作；保留屏障协议，不把 object/GC 的必要协作当成可直接删除的依赖 |
| 后端再导出整个 VM 子系统 | interp/JIT/profiler 的 `src/lib.rs` 批量 pub use z42 模块 | 先改为实现内私有导入，再逐步列出确需跨库的最小执行/诊断契约 |

这些是源码中的具体依赖，不是 Cargo 生产依赖环的证明。生产 core 不依赖具体后端，测试 dev-dependency 和
测试门面的内联路径另行统计。Rust import 的简单文本扫描还会漏掉分组导入、别名和宏，并误收测试/注释；
架构检查需要结合可见性、实际消费者和构建配置。

### 目标访问关系（未全部实施）

```mermaid
flowchart TD
    H[host：宿主请求与结果] --> V[VM 装配与程序会话]
    V --> A[components::api：内部能力门面]
    E[VM 服务准备与诊断请求] --> A
    A --> M[组件管理：来源、依赖、实例与寿命]
    M --> P[最小同构建协议与适配器]
    P --> I[interp / JIT / profiler 实现]
    V --> L[loader / interop 协调]
    V --> C[Core 与子系统状态]
```

图中的箭头表示准备/访问流程；具体组件通过协议回调注册能力，不表示 core 在 Cargo 中链接具体实现。
执行热路径在准备阶段获得 Engine 等能力，后续直接调用已绑定接口。组件门面不成为转出全部 VM 类型的新总括模块。

仅将 `backends::attach` 换成 `api::attach(INTERP_COMPONENT)` 仍会泄露具体实现身份，不能达到实现透明。
目标接口按执行角色、堆快照、CPU 采样等稳定能力请求服务；管理层将其绑定到选定提供方与版本，
再在内部使用组件 ID 构造依赖闭包和实例。基线与优化引擎属于不同语义角色，即使接口形状相同也不会混选。
未配置的多提供方歧义与能力缺失均显式报错；能力查询仍不触发绑定、装库或创建实例。

### 状态与协议归属

| 所有者 | 保存/提供 | 消费者需要什么 |
|---|---|---|
| host | 宿主配置、句柄、回调及输入输出适配 | 功能级请求与结果，不是组件注册表 |
| VM 装配层 | 构造输入、启动顺序和失败发布边界 | 接受显式配置及服务，产生可调用程序 |
| components 管理实现 | 能力到提供方的绑定、工厂缓存、依赖闭包、每 VM 实例与代码保活 | 内部 api 的准备/能力查询；不让消费者传实现 ID，不返回可修改的 OnceLock |
| exec / diagnostics | 引擎与观察/采样的消费契约 | 装配时绑定的接口；不决定组件库路径或具体实现 |
| 同构建协议层 | 描述符、版本、身份、接口表及最小桥接 | 仅给 VM 与组件适配器；不是普通宿主 SDK 扩展面 |
| loader / interop | 代码来源/符号与描述符/编组分别拥有 | 窄接口连接，不互相承担完整装入事务 |
| object / GC / threads | 表示、屏障、分配、暂停与根协议 | 共享必要数据契约，通过回调适配，逐步减少实现穿透 |

## 实现

### 文件命名与职责映射

同名并不自动等于重复实现：PAL 的 unix/windows/wasm 文件是同一契约的平台镜像，保留对称命名。
下表是职责收口后的候选名，未改名的文件仍在左列路径；最终名称随对应抽象同轮确认，不先制造兼容转发层。

| 当前文件/文件组 | 实际职责 | 候选名称/拆分方向 |
|---|---|---|
| host/config.rs、corelib/core/config.rs、gc/config.rs、JIT config.rs | C 宿主配置、托管查询、GC 输入、JIT 输入 | host_abi_config.rs、runtime_config_builtins.rs、gc_config.rs、jit_config.rs |
| host/state.rs、interop/state.rs | 宿主会话；原生模块/类型/buffer 状态 | host_session.rs、native_resources.rs |
| threads/state.rs、loader/state.rs、exec/state.rs、metadata/state.rs | mutator、加载、执行、链接状态 | mutator_state.rs、loader_state.rs、execution_state.rs、link_state.rs |
| 四处 builtins.rs | 组装提供方、声明导出、名称目录、执行派发 | builtin_registry_builder.rs、core_builtin_descriptors.rs、builtin_symbol_catalog.rs、builtin_dispatch.rs |
| components/api.rs、backends.rs | 跨库协议；来源选择/平台发现/装配 | 内部 api 门面与 component_abi 分开；source/discovery/attachment 按职责拆分 |
| loader/native.rs、exec/native.rs | 原生库装入/注册；native 调用服务 | native_library_loader.rs、native_module_registration.rs、native_call_services.rs |
| host/profiling.rs、components/profiling.rs | 宿主诊断请求；profiler 装配 | diagnostic_requests.rs、profiler_attachment.rs |
| host/load.rs、program_builder.rs、vm_defaults.rs | 包准备、程序/VM 装配和默认策略 | 职责先迁到 VM 装配层，再按 program_preparation、vm_assembly、default_services 命名 |
| context/types.rs | Core、线程视图和布局偏移 | 按 vm_core、vm_context、context_layout 拆分，保持布局约束 |

`*_config`、`*_policy`、`*_registry` 等后缀表达不同职责。目录本身已足够限定的局部文件不为全仓唯一名机械改名；
对外引用面收口后，具体实现文件重命名只影响其模块声明、适配器和同目录测试。

## 边界与限制

本页描述当前缺口和目标，不表示公开面已被封闭或架构检查已实现。当前 host、组件模块和后端仍有上表所列公开路径。
普通宿主 API 收口涉及 Rust 签名、C 静态装配头、launcher/workload 胶水与真实 SDK 消费者，需同轮迁移和验证。
GC/对象、线程/执行及元数据共享的数据契约需要单独建模，不能通过把全部目录改为私有或抽一个万能 trait 修复。
