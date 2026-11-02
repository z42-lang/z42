# loader/

## 职责

编排包的定位、读取、合并、惰性加载与加载上下文生命周期。二进制格式和解码由 metadata 提供，类型组装与链接状态由 vm/runtime 提供。

## 功能索引

| 入口 | 职责 |
|---|---|
| `package/` | VFS 读取、LoadedArtifact、模块合并和执行索引准备 |
| `locate/` | namespace 候选索引、依赖定位及 probing 展开 |
| `lazy/` | 惰性包加载、符号路由、注册和调试符号 |
| `contexts.rs` | ContextRegistry、assembly 身份和惰性卸载关联 |
| `availability.rs` | 加载期可用性折叠 |

## 基础用法

常用入口为 `load_artifact`、`load_artifact_from_bytes`、`merge_modules` 和 `LazyLoader`，从 `z42::vm::loader` 导入。调用方仍提供 VFS 与搜索目录；公开入口只汇总拥有者，不在 metadata 中保留加载别名。

## 如何测试验证

仓库根运行 `./xtask test runtime`。定向验证：

```bash
(cd src/runtime && cargo test --workspace vm::loader:: -- --test-threads=1)
```

## 关联文档

加载与链接顺序见 [VM 总体架构](../../../../../docs/internals/src/runtime/vm-architecture.md)，上下文与卸载契约见 [加载上下文](../../../../../docs/internals/src/runtime/load-context.md)。

## 核心文件

`mod.rs` 汇总加载入口；各子目录实现与原有测试同目录组织。包定位、读取、初始化准备和卸载协议的先后关系由既有调用流程保持。
