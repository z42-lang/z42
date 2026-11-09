# z42-jit — JIT 组件

## 职责

拥有 Cranelift 编译器、tier/OSR、代码页与 JIT Engine；不依赖解释器实现。

## 功能索引

Cargo 的同一 library target 产出 rlib 和 cdylib；静态调用使用 rlib，动态包启用 shared-core 使用 cdylib，无额外包装 crate。普通静态构建的附带 cdylib 不导出组件入口，不作为运行时加载组件交付。

src/jit 为后端实现，src/jit/component.rs 提供工厂与 ExecutionApi；src/jit/config.rs 再导出 core 的输入类型。

## 基础用法

静态通过 z42 的 jit feature 链接；仅桌面 x64/arm64 编译 Cranelift，动态由本库直接提供 cdylib，启用 shared-core 后导出组件 C 入口。 嵌入调用方统一使用 z42::host，不直接依赖组件内部模块。

## 如何测试验证

静态构建运行 `xtask test` 与 `xtask package sdk --verify`。动态构建运行 `xtask package runtime --linkage dynamic --profile debug --verify`（release 同理），实际搬移后的生产组件测试覆盖生命周期和缺失后端错误。源内白盒测试由主 crate 的同源测试模块执行，不复制实现。

## 关联文档

- [组件化运行时](../../../../docs/internals/src/runtime/componentized-runtime.md)
- [宿主规范](../../src/host/README.md)

## 核心文件

src/jit 为后端实现，src/jit/component.rs 提供工厂与 ExecutionApi；src/jit/config.rs 再导出 core 的输入类型。
