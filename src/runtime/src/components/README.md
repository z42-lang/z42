# components — 统一组件框架

## 职责

统一组件协议、静态工厂登记、动态库加载、依赖校验和每 VM 实例管理。`VmCore` 持有实例集合，worker 共享所属 Core 的组件状态。
组件管理属于 VM 内部设施；当前仍存在 host 再导出及消费者穿透子模块的缺口，内部门面尚未收口，详见[模块边界](../../../../docs/internals/src/runtime/module-boundaries.md)。

组件工厂只持有描述符、回调和原生库代码，可以跨 VM 共享；实例、JIT 状态和堆仍按 VM 隔离。这里不实现解释器、JIT 或其他具体组件。

## 功能索引

| 文件 | 职责 |
|---|---|
| `api.rs` | 当前为跨库 C 协议，并非目标中的 VM 内部访问门面；两者待分离 |
| `catalog.rs` | 按启用根加载工厂依赖闭包、缓存与描述符校验、依赖排序和代码所有权 |
| `options.rs` | 启用根与可用提供方配置、事务装配入口和带租约的只读查询 |
| `instance.rs` | 每 VM 创建/销毁、核心与依赖服务、接口租约和反序清理 |
| `profiling.rs` | profiler 按需装配、同核心身份检查与代码寿命 |
| `backends.rs` | interp/JIT 来源选择、能力查询、身份检查和执行接口装配 |
| `linked.rs` | 进程寿命的静态工厂登记；不保存 VM 实例 |
| `identity.rs` | 单份 runtime 映像身份与编译期内存布局校验 |

## 基础用法

当前 VM 装配使用 `ComponentOptions`、`BackendOptions` 与查询接口，部分入口仍经 host 转出。目标访问关系是
VM 消费者通过内部 `components::api` 请求能力，管理实现和具体组件对消费者透明；普通宿主只表达功能请求。
组件实现目前通过 `z42::components::api` 使用同构建协议；该跨库协议与内部门面的拆分尚未实施。
描述符、接口头、构建身份与执行核心身份均在装配时校验。

加载流程为：选择启用根与提供方来源 → 递归加载并校验所需工厂闭包 → 按依赖创建每 VM 实例 → 发布注册表 → 装配执行接口。

`with_static` / `with_dynamic` 选择启用根；`with_static_provider` / `with_dynamic_provider` 只登记可被依赖的来源。未使用的提供方不调用 init、不打开库，也不创建实例。依赖必须有显式来源；不会扫描其他进程工厂或绕过后端禁用策略。普通接口查询保持只读，不触发装配。失败回滚和正常释放均按依赖逆序销毁。`ComponentLease` 保留所属 Core、实例和代码；原生映像的进程寿命缓存由 PAL 管理，防止 TLS 析构跳入已卸载代码。

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

`mod.rs` 汇总组件框架的 Rust 接口；`api.rs` 定义协议，`catalog.rs` 管理工厂代码，`instance.rs` 管理每 VM 状态。构建身份要求同源、同工具链与目标；跨版本组件不兼容。
