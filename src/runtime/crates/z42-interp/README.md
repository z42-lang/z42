# z42-interp — 解释器组件

## 职责

拥有字节码解释器与基线 Engine，只依赖共享核心和组件协议。

## 功能索引

同一 library target 产出 rlib 和 cdylib，直接导出组件 init 入口。静态宿主显式链接 rlib 并通过 host 登记；动态包在同一 Cargo 图中构建运行时、后端和共享 std，再通过 host 加载。没有后端包装 crate。

src/interp 为解释实现，src/interp/component.rs 提供静态工厂和 ExecutionApi，src/lib.rs 为边界。

## 基础用法

静态通过 z42 的 interp feature 链接；动态由本库直接提供 cdylib，启用 shared-core 后导出组件 C 入口，JIT 回退只经注册表调用。 嵌入调用方统一使用 z42::host，不直接依赖组件内部模块。

## 如何测试验证

静态构建运行 `xtask test` 与 `xtask package sdk --verify`。动态构建运行 `xtask package runtime --linkage dynamic --profile debug --verify`（release 同理），实际搬移后的生产组件测试覆盖生命周期和缺失后端错误。源内白盒测试由主 crate 的同源测试模块执行，不复制实现。

## 关联文档

- [组件化运行时](../../../../docs/internals/src/runtime/componentized-runtime.md)
- [宿主规范](../../src/host/README.md)

## 核心文件

src/interp 为解释实现，src/interp/component.rs 提供静态工厂和 ExecutionApi，src/lib.rs 为边界。
