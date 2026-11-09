# z42-runtime-shared — 动态共享核心

## 职责

动态执行组件共用的 Rust dylib；保证 GC、TLS、类型身份与 VM 实现只有一份。

## 功能索引

src/lib.rs 的 link_anchor 强制宿主与后端连接相同核心。

## 基础用法

由动态打包器使用 prefer-dynamic 在单次 Cargo 图中构建；携带匹配的 Rust std，使用 System 分配器。 嵌入调用方统一使用 z42::host，不直接依赖组件内部模块。

## 如何测试验证

静态构建运行 `xtask test` 与 `xtask package sdk --verify`。动态构建运行 `xtask package runtime --linkage dynamic --profile debug --verify`（release 同理），实际搬移后的生产组件测试覆盖生命周期和缺失后端错误。源内白盒测试由主 crate 的同源测试模块执行，不复制实现。

## 关联文档

- [组件化运行时](../../../../docs/internals/src/runtime/componentized-runtime.md)
- [宿主规范](../../src/host/README.md)

## 核心文件

src/lib.rs 的 link_anchor 强制宿主与后端连接相同核心。
