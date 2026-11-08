# app — 应用执行

## 职责

拥有应用参数与隔离应用服务，提供 CLI 和嵌入端共用的加载与执行入口。公开入口仍为 `z42::app`。

## 功能索引

| 文件 | 职责 |
|---|---|
| `mod.rs` | RunOpts、run 与退出报告 |
| `state.rs` | 每 VM 应用参数 |
| `services.rs` | 核心库应用服务的门面适配 |

## 基础用法

调用 `app::run` 时传入已解析的 `RunOpts`，每次运行的文件访问使用其中的 VFS。

## 如何测试验证

仓库根运行 `./xtask test` 和 `./xtask test runtime`。

## 关联文档

- [组件化运行时](../../../../docs/internals/src/runtime/componentized-runtime.md)
- [嵌入宿主](../../../../docs/internals/src/runtime/embedding.md)

## 核心文件

`mod.rs` 定义应用入口；`state.rs` 与 `services.rs` 分别维护应用状态和服务适配。
