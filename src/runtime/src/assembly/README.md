# assembly — 运行时装配

## 职责

选择具体引擎、核心库和扩展，组装主 VM；共享应用与宿主的启动步骤。该目录属于 crate 门面，底层协议仍由各子系统定义。

## 功能索引

| 文件 | 职责 |
|---|---|
| `context.rs` | 主 VM 构造与状态接线 |
| `engine.rs` | 按 feature 注册具体执行引擎 |
| `extension.rs` | 冻结每 VM 扩展状态 |
| `builtin.rs` | 组装 builtin 目录 |
| `object_protocol.rs` | 绑定对象协议 |
| `loader.rs` | 读取加载配置并提供融合输入 |
| `boot.rs` | 应用与宿主共用的启动步骤 |

## 基础用法

通过既有 `VmContext` 构造入口完成装配；worker 共享原有 Core，不重新选择组件。

## 如何测试验证

仓库根运行 `./xtask test` 和 `./xtask test runtime`。

## 关联文档

- [组件化运行时](../../../../docs/internals/src/runtime/componentized-runtime.md)
- [嵌入宿主](../../../../docs/internals/src/runtime/embedding.md)

## 核心文件

`context.rs` 完成主上下文构造；`boot.rs` 保持应用与宿主启动路径一致。
