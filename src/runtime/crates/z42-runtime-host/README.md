# z42-runtime-host — 动态嵌入宿主

## 职责

导出 z42::host 既有 C ABI，不另实现加载或执行流程。

## 功能索引

src/lib.rs 再导出宿主入口；build.rs 设置 ELF SONAME。

## 基础用法

dynamic-component feature 启用；随动态包的 core/std/后端部署，宿主使用包内 z42_host.h。 嵌入调用方统一使用 z42::host，不直接依赖组件内部模块。

## 如何测试验证

静态构建运行 `xtask test` 与 `xtask package sdk --verify`。动态构建运行 `xtask package runtime --linkage dynamic --profile debug --verify`（release 同理），实际搬移后的生产组件测试覆盖生命周期和缺失后端错误。源内白盒测试由主 crate 的同源测试模块执行，不复制实现。

## 关联文档

- [组件化运行时](../../../../docs/internals/src/runtime/componentized-runtime.md)
- [宿主规范](../../src/host/README.md)

## 核心文件

src/lib.rs 再导出宿主入口；build.rs 设置 ELF SONAME。
