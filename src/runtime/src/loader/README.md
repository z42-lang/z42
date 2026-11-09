# loader/

## 职责

编排包的定位、读取、合并、惰性加载与加载上下文生命周期，以及 native 库候选路径、装入与代码寿命。二进制格式和解码由 z42-zpkg 提供，类型组装与链接状态由 metadata 提供。

## 功能索引

| 入口 | 职责 |
|---|---|
| `native.rs` | 宿主注入的按名称路径服务、PAL 装入、Tier 1 注册入口与库代码保活 |
| `state.rs` | `LoaderState`：每 VM 惰性加载器、上下文、初始化队列与 VFS 依赖 |
| `fusion_inputs.rs` | 私有融合输入与惰性输入来源 |
| `package/` | VFS 读取、LoadedArtifact、模块合并和执行索引准备 |
| `locate/` | namespace 候选索引、依赖定位及 probing 展开 |
| `lazy/` | 惰性包加载、符号路由、注册和调试符号 |
| `context_gc.rs` | 持有上下文身份与关联快照，适配 GC 的标记后观察和清扫后回收 |
| `contexts.rs` | ContextRegistry、assembly 身份和惰性卸载关联 |
| `availability.rs` | 加载期可用性折叠 |

## 基础用法

常用入口为 `load_artifact`、`load_artifact_from_bytes`、`merge_modules` 和 `LazyLoader`，从 `z42::loader` 导入。调用方仍提供 VFS 与搜索目录；公开入口只汇总拥有者，不在 metadata 中保留加载别名。

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

路径服务只在请求具体库时运行。`native::open_library` 在执行 OS 装载前检查 PAL 支持；iOS/WASM 不支持脚本 native 动态库。格式解码归 z42-zpkg，符号配对和 build ID 校验由 loader 执行。
