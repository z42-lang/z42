# crates/

## 职责

本目录包含可独立编译的执行后端、产物格式、native SDK 和原生扩展。运行时主库入口位于 ../src/lib.rs，host 与 CLI 是该包内部的宿主模块，不在本目录重复包装。

## 功能索引

| 子目录 | 职责 |
|---|---|
| z42-interp | 基线解释器；提供 rlib/cdylib，只依赖运行时与产物格式 |
| z42-profiler | CPU 采样与堆分析，静态登记或按需加载，默认 VM 不创建分析状态 |
| z42-jit | Cranelift、tier/OSR 和编译代码；提供 rlib/cdylib，只依赖运行时与产物格式 |
| z42-zpkg | zpkg/zbc/zsym、静态 IR 和不可变 PackageImage；无 VM 依赖 |
| z42-abi | no_std、零依赖的 C ABI 布局；module.rs 定义模块描述符和值，不执行注册 |
| z42-rs | Rust 类型/模块编写辅助；module.rs 封装调用值、错误和回调，不链接 VM |
| z42-macros | proc-macro；由 Rust 独立编译，供 native SDK 与 builtin 描述使用 |
| z42-compression | C ABI 压缩组件；原生首次使用时动态加载，静态宿主按需链接/登记；默认最终 WASM 编入 |
| z42-repl | registration.rs 提供脚本注册与 console 能力边界，editor.rs 提供可选桌面编辑器 |

## 依赖关系

执行后端依赖 z42；生产运行时不链接具体后端。最终宿主选择并登记所需组件。native SDK 的 z42-rs/z42-macros/z42-abi 保持 VM-free，独立模块不会因此引入整台 VM；真正的注册表、查询与调用桥在运行时 interop，装载在 loader。模块编写者可关闭 z42-rs 的默认 types feature，仅使用模块辅助；旧类型/宏 API 默认保留。proc-macro 和 no_std ABI 各有独立编译约束。

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
