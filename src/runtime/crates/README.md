# crates/

## 职责

本目录按职责包含 VM component、直接编译引用的 VM/配套模块、独立运行时库三类 crate。运行时组合根位于 ../src/lib.rs；是否生成 cdylib 不决定类别，具体边界见[组件化运行时](../../../docs/internals/src/runtime/componentized-runtime.md)。

## 功能索引

| 子目录 | 类型 | 职责与入口 |
|---|---|---|
| z42-interp | component | 基线解释器；VM 通过 components::api 消费，提供 rlib/cdylib |
| z42-profiler | component | CPU 采样与堆分析；通过 components::api 按需准备，默认不创建分析状态 |
| z42-jit | component | Cranelift、tier/OSR 与编译代码；VM 通过 components::api 消费 |
| z42-zpkg | VM 模块 | 直接 Rust 引用的 zpkg/zbc/zsym、静态 IR 和 PackageImage；无 VM 依赖 |
| z42-abi | 配套协议模块 | 直接引用的 no_std C ABI 布局、模块描述符和值；不执行注册 |
| z42-rs | 配套 SDK 模块 | Rust native 编写辅助，封装值/错误/回调，不链接 VM |
| z42-macros | 配套构建模块 | 构建期 proc-macro，供 native SDK 与 builtin 描述使用 |
| z42-compression | 独立运行时库 | 独立压缩 API 与通用 native 描述符，经 loader/interop 或自身 API 接入 |
| z42-repl | 独立运行时库 | 通用输入、模块注册与可选编辑器，经 loader/interop 或自身 API 接入 |

## 依赖关系

执行后端依赖 z42 Core。最终宿主登记具体组件工厂，Core 在装配 VM 时按 `Baseline`、`Optimizing` 等角色绑定接口。后端通过 Core 使用已绑定的能力，例如 JIT 回退经 `z42::exec::invoke` 调用当前 VM 的基线引擎。组件 ID 和装载策略由装配层管理。

后端 crate 之间禁止直接 Cargo 依赖，范围包括 `dev-dependencies`。后端单测通过 Core 接口注入测试实现，真实的跨后端对照测试由 Runtime 集成测试装配组件，再通过 Core 接口调用。

native SDK 的 z42-rs/z42-macros/z42-abi 保持 VM-free，独立模块不会因此引入整台 VM；真正的注册表、查询与调用桥在运行时 interop，装载在 loader。模块编写者可关闭 z42-rs 的默认 types feature，仅使用模块辅助；旧类型/宏 API 默认保留。proc-macro 和 no_std ABI 各有独立编译约束。

## 如何测试验证

```bash
./xtask test runtime
./xtask package sdk --verify
./xtask package runtime --linkage dynamic --profile debug --verify
```

## 关联文档

- [运行时入口](../README.md)
- [宿主入口](../src/host/README.md)
- [组件机制](../../../docs/internals/src/runtime/componentized-runtime.md)
- [嵌入机制](../../../docs/internals/src/runtime/embedding.md)
