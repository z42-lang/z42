# z42-jit-component — JIT 动态入口

## 职责

只包装 JIT 工厂的 C 入口，不复制编译器实现。

## 功能索引

src/lib.rs 导出 z42_jit_init。

## 基础用法

默认不产生后端实现；dynamic-component 选择 shared-core 后由桌面动态打包器统一构建。 嵌入调用方统一使用 z42::host，不直接依赖组件内部模块。

## 如何测试验证

静态构建运行 `xtask test` 与 `xtask package sdk --verify`。动态构建运行 `xtask package runtime --linkage dynamic --profile debug --verify`（release 同理），实际搬移后的生产组件测试覆盖生命周期和缺失后端错误。源内白盒测试由主 crate 的同源测试模块执行，不复制实现。

## 关联文档

- [组件化运行时](../../../../docs/internals/src/runtime/componentized-runtime.md)
- [宿主规范](../../src/host/README.md)

## 核心文件

src/lib.rs 导出 z42_jit_init。
