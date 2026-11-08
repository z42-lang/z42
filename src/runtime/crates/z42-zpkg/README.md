# z42-zpkg — 静态产物库

## 职责

解析 zbc、zpkg 和 zsym，提供静态 IR、名称查询、调试记录与不可变包映像。不依赖 VM、执行后端、堆或宿主状态。

## 功能索引

| 入口 | 职责 |
|---|---|
| `src/format/` | magic、依赖记录、严格版本校验与构建标识 |
| `src/reader/` | 产物、节目录、指令、签名、类型与符号 sidecar 解析 |
| `src/ir/` | 静态模块、函数、类型、指令与执行策略枚举 |
| `src/index/` | 名称索引、类型名与型参标记查询 |
| `src/debug/` | 源位置、局部变量、DBUG 记录与帧位置查询 |
| `src/image.rs` | 可跨 VM 共享的不可变 PackageImage |

## 基础用法

组件通过 `z42_zpkg::reader` 解码，再由 `PackageImage::new` 冻结定义。运行时身份、链接缓存、初始化及执行状态属于消费方，不存入本库。

嵌入宿主从运行时的 `host` 门面取得公开接口和类型，例如 `z42::host::PackageImage`；组件库本身不承担宿主装配。

## 如何测试验证

在 `src/runtime/` 运行 `cargo test -p z42-zpkg`。本库是 workspace 默认成员，原有测试随实现迁入，完整 `./xtask test` 会执行其单元测试。

## 关联文档

- [VM 架构](../../../../docs/internals/src/runtime/vm-architecture.md)
- [产物格式](../../../../docs/internals/src/formats/README.md)
- [组件化运行时](../../../../docs/internals/src/runtime/componentized-runtime.md)

## 核心文件

`src/lib.rs` 汇总静态定义；`src/image.rs` 管理共享定义快照。测试保留在各实现同目录的独立文件。
