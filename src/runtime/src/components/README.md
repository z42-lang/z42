# components — 统一组件框架

## 职责

提供 VM 内部能力门面、已链接工厂登记、组件契约校验和每 VM 实例管理。动态代码通过 loader 请求装入，默认原生路径归 loader；组件层不保存独立的映像缓存。`VmCore` 持有实例集合，worker 共享所属 Core 的组件状态。
组件管理及其子模块在 crate 内部可见，VM 消费者经 `components::api` 准备和查询能力；普通宿主经 `host` 加载和执行程序，详见[模块边界](../../../../docs/internals/src/runtime/module-boundaries.md)。

组件工厂只持有描述符、回调和原生库代码，可以跨 VM 共享；实例、JIT 状态和堆仍按 VM 隔离。这里不实现解释器、JIT 或其他具体组件。

## 功能索引

| 文件 | 职责 |
|---|---|
| `api.rs` | 内部准备协调、执行角色绑定/附着/查询及 profiler 来源配置 |
| `catalog.rs` | 按启用根准备工厂依赖闭包、缓存与描述符校验、实例依赖排序和 loader 代码句柄 |
| `options.rs` | 启用根与可用提供方配置、事务装配入口和带租约的只读查询 |
| `instance.rs` | 每 VM 创建/销毁、核心与依赖服务、接口租约和反序清理 |
| `profiling.rs` | profiler 按需装配、同核心身份检查与代码寿命 |
| `backends.rs` | interp/JIT 来源选择、候选发现、目标能力和初始引擎注册表 |
| `ffi.rs` | 最终链接组合的 C 登记契约（声明在 `../../include/z42_components.h`）；返回组件状态，不访问 host 错误槽 |
| `linked.rs` | 进程寿命的静态链接与共享链接工厂登记；不保存 VM 实例 |
| `execution_roles.rs` | 基线/优化角色到提供方的不可变绑定，实例创建前验证接口 |
| `abi.rs` / `bridge.rs` | 同构建 C 协议、核心身份和布局、对象服务借用及引擎、tier、profiler 后端登记 |

## 基础用法

VM 消费者通过内部 `components::api` 准备和查询执行角色。分析操作由 diagnostics 提供，profiler 组件由这里装配。执行角色绑定在实例创建前验证，
消费者不传具体提供方 ID；执行热路径使用一次装配的引擎。组件管理类型不从 host 转出；宿主功能请求由私有 VM 装配输入适配。
独立组件通过 `z42::components::abi` 使用同构建协议。`components::bridge` 提供核心身份、布局、对象服务借用和后端登记入口。组件装配先校验身份并保留代码，再调用附着回调。解释器和 JIT 登记所属 VM 的引擎，JIT 运行时还通过桥发布按需构建的 tier。
最终 Rust 组合根使用 `abi::register_static_component` 或 `abi::register_shared_component`；C 声明在 `include/z42_components.h` 维护，由 native 聚合进既有的 `z42_host.h` 发布；调用方不单独包含组件头。两者仅登记工厂，不调用工厂或创建 VM。
描述符、接口头、构建身份与执行核心身份均在装配时校验。

装配按每 VM 策略分别检查静态链接、共享链接和运行时装入，详见
[组件三种使用形态](../../../../docs/internals/src/runtime/componentized-runtime.md#组件三种使用形态)。

加载流程为：选择启用根与提供方来源 → 请求 loader 准备代码并校验所需工厂闭包 → 按语义依赖创建每 VM 实例 → 发布注册表 → 装配执行接口。

`with_static` / `with_dynamic` 选择启用根；`with_static_provider` / `with_dynamic_provider` 只登记可被依赖的来源。未使用的提供方不调用 init、不打开库，也不创建实例。依赖必须有显式来源；不会扫描其他进程工厂或绕过后端禁用策略。普通接口查询保持只读，不触发装配。失败回滚和正常释放均按依赖逆序销毁。按组件 ID 的身份与接口查询仅为内部测试构建提供；生产 VM 通过 `components::api` 的能力入口消费组件，host 不导出查询或租约。内部 `ComponentLease` 保留所属 Core、实例和代码；原生映像的进程寿命缓存由 loader/code 管理，防止 TLS 析构跳入已卸载代码。

通用接口只传 C 布局、函数指针与不透明句柄，不跨边界转移 Rust 容器所有权或 unwind。静态登记与动态库入口都要求可信的原生回调；创建失败返回的非空状态也必须能由销毁回调清理。

## 如何测试验证

在 `src/runtime/` 运行：

```bash
cargo test -p z42 --lib components:: -- --test-threads=1
```

协议、后端选择、静态工厂、依赖、回滚和库保活的既有单测集中在本目录。完整覆盖运行 `./xtask test runtime`；实际发布库的验证运行 `./xtask package runtime --linkage dynamic --profile debug --verify`。

## 关联文档

- [组件化运行时](../../../../docs/internals/src/runtime/componentized-runtime.md)
- [嵌入机制](../../../../docs/internals/src/runtime/embedding.md)
- [构建身份生成](../../build.rs)

## 核心文件

`mod.rs` 声明私有管理子模块；`api.rs` 提供 VM 内部门面，`catalog.rs` 管理工厂代码，`instance.rs` 管理每 VM 状态。构建身份要求同源、同工具链与目标；跨版本组件不兼容。

原生来源准入由 loader 的 NativeLoading 负责：静态链接、共享链接、运行时装入分别检查。工厂缓存命中之前仍检查每 VM 策略；拒绝不会写入共享失败缓存。profiler 的来源和实例保存在组件所有的 ProfilingState，诊断后端先释放，再释放组件实例。
