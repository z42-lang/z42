# z42-jit — JIT 组件

## 职责

crate 类型：component。VM 经内部 components::api 消费优化执行能力；本库只通过同构建 ABI 和最小 bridge 连接 VM。

拥有 Cranelift 编译器、tier/OSR、代码页与 JIT Engine。生产路径经 Core 的 `z42::exec::invoke` 回退到当前 VM 绑定的基线引擎。

## 功能索引

同一 library target 产出 rlib 和 cdylib，直接导出组件 init 入口。静态宿主显式链接 rlib 并通过 host 登记；动态包在同一 Cargo 图中构建运行时、后端和共享 std，再通过 host 加载。没有后端包装 crate。

src/jit 为后端实现，src/jit/component.rs 提供工厂与 ExecutionApi；src/jit/config.rs 再导出 core 的输入类型。

独立实现通过 `z42::components::abi` 使用描述符和 C 布局接口，通过 `z42::components::bridge` 校验共享核心身份/布局。核心模块仅私有导入，不由本库再导出。
构造泛型 Type 时通过 bridge 只读借用 Core 已绑定的对象协议，契约属于 object；不穿透 ExecState 或查询组件注册表。

## 基础用法

静态宿主链接本库并登记组件工厂；运行时的 jit feature 只启用中立契约，不链接具体实现。仅桌面 x64/arm64 编译 Cranelift。嵌入调用方统一使用 z42::host，不直接依赖组件内部模块。

## 如何测试验证

静态构建运行 `xtask test` 与 `xtask package sdk --verify`。动态构建运行 `xtask package runtime --linkage dynamic --profile debug --verify`（release 同理），实际搬移后的生产组件测试覆盖生命周期和缺失后端错误。源内白盒测试由本 crate 执行。

## 关联文档

- [Runtime crate 依赖约定](../README.md#依赖关系)
- [组件化运行时](../../../../docs/internals/src/runtime/componentized-runtime.md)
- [宿主规范](../../src/host/README.md)

## 待办

- 将仍通过 `z42-interp` dev-dependency 调用解释器的对照测试迁到 Runtime 集成层，JIT 白盒测试使用 Core 接口注入测试引擎。

## 核心文件

src/jit 为后端实现，src/jit/component.rs 提供工厂与 ExecutionApi；src/jit/config.rs 再导出 core 的输入类型。
