# loader/

## 职责

编排包的定位、读取、合并、惰性加载与加载上下文生命周期，以及 native 库候选路径、装入与代码寿命。二进制格式和解码由 z42-zpkg 提供，类型组装与链接状态由 metadata 提供。

## 功能索引

| 入口 | 职责 |
|---|---|
| `native.rs` | 原生库开库、路径策略、每 VM 映像缓存与符号解析；装载事务归 interop::install |
| `native_policy.rs` | 每 VM 原生代码来源许可与搜索目录，拒绝发生在探测、入口调用和缓存复用之前 |
| `artifact.rs` | 产物加载与可选会话缓存入口、LazyLoader 默认构造 |
| `dependency_candidates.rs` | 按包声明与导入命名空间发现惰性加载候选 |
| `eager.rs` | 沿包声明和命名空间读取传递依赖，复用启动期去重登记 |
| `program.rs` | 使用 resolver 和搜索目录准备 prelude、启动依赖和惰性候选，在 availability 折叠后更新执行索引 |
| `prelude.rs` | 按显式搜索路径读取首个可用的标准前奏包 |
| `ffi.rs` | static/shared native 链接登记，不操作 host 会话或错误槽 |
| `code.rs` | 通用原生映像装入、规范路径缓存、符号访问和进程代码保活；不解析消费者 ABI、不创建 VM 实例 |
| `native_paths.rs` | 组件和独立运行时库共用的通用搜索目录策略 |
| `native.rs` | 宿主注入的按名称路径服务、通用模块及 Tier 1 注册入口，调用 code 装入映像 |
| `linked.rs` | 任意合法逻辑库名的已链接符号解析器与组合根入口查询，先于动态候选；不维护具体库白名单 |
| `state.rs` | `LoaderState`：每 VM 惰性加载器、上下文、初始化队列与 VFS 依赖 |
| `fusion_inputs.rs` | 私有融合输入与惰性输入来源 |
| `package/` | VFS 读取、LoadedArtifact、模块合并和执行索引准备 |
| `locate/` | namespace 候选索引、依赖定位及 probing 展开 |
| `lazy/` | 惰性包加载、符号路由、注册和调试符号 |
| `context_gc.rs` | 保留 VM 上下文回收适配器的公开路径 |
| `image_cache.rs` | 会话级 zpkg 解码缓存（内容哈希键）：已加载即共享定义 Arc，未命中才解码冻结 |
| `contexts.rs` | ContextRegistry、assembly 身份和惰性卸载关联 |
| `runtime_contexts.rs` | RuntimeLoader 的上下文生命周期和反射查询，内部管理注册表锁 |
| `availability.rs` | 加载期可用性折叠 |

## 基础用法

反射的精确类型名通过 `RuntimeLoader::resolve_type` 查询。入口模块优先，未命中时再解析包依赖。已载探测保留独立入口，适用于只读取现有注册的调用方。

常用入口为 `load_artifact`、`load_artifact_from_bytes`、`merge_modules` 和 `LazyLoader`，从 `z42::loader` 导入。调用方仍提供 VFS 与搜索目录；`ModuleSearch.Dirs` 从所属 VM 的加载器查询当前目录快照，worker 共享、不跨 VM 复用；公开入口只汇总拥有者，不在 metadata 中保留加载别名。
`locate::search_paths` 只组合显式入口、额外路径和 libs，并通过传入 VFS 检查目录；进程环境与默认路径由 main/C/Rust/浏览器等来源实现解析。

块索引准备在实际重算每个函数的融合侧表时才调用输入来源。门面绑定进程配置，文件、字节、可用性探测与惰性加载共用该读取策略。每 VM 的加载状态只保存函数指针，构造和解码阶段不因这项注入提前读取融合配置。

## 如何测试验证

仓库根运行 `./xtask test runtime`。定向验证：

```bash
(cd src/runtime && cargo test --workspace loader:: -- --test-threads=1)
```

## 关联文档

加载与链接顺序见 [VM 总体架构](../../../../docs/internals/src/runtime/vm-architecture.md)，上下文与卸载契约见 [加载上下文](../../../../docs/internals/src/runtime/load-context.md)。

## 核心文件

`mod.rs` 汇总加载入口；各子目录实现与原有测试同目录组织。包定位、读取、初始化准备和卸载协议的先后关系由既有调用流程保持。

路径服务只在请求具体库时运行。`native::open_library` 是内部入口，在执行 OS 装载前检查 PAL 支持；iOS/WASM 不支持脚本 native 动态库。可信运行时组件由部署适配器校验后请求 code 装入，不借此扩大脚本权限。格式解码归 z42-zpkg，各消费者负责自己的描述符和身份校验。

模块限定调用首次使用时才请求代码与自描述入口；目录查询不装库。独立模块的 ABI 校验和每 VM 状态归 interop，loader 不点名 compression 等库。
