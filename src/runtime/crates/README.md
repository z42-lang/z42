# crates/

## 职责

本目录包含可独立编译的执行后端、产物格式、native SDK 和原生扩展。运行时主库位于 ../src/lib.rs，统一宿主库与 CLI 的 Cargo 包位于 ../src/host；它们不在本目录重复包装。

## 功能索引

| 子目录 | 职责 |
|---|---|
| z42-interp | 基线解释器；提供 rlib/cdylib，只依赖运行时与产物格式 |
| z42-jit | Cranelift、tier/OSR 和编译代码；提供 rlib/cdylib，只依赖运行时与产物格式 |
| z42-zpkg | zpkg/zbc/zsym、静态 IR 和不可变 PackageImage；无 VM 依赖 |
| z42-abi | no_std、零依赖的 C ABI 布局 |
| z42-rs | native 类型创作 SDK；不链接 VM |
| z42-macros | proc-macro；由 Rust 独立编译，供 native SDK 与 builtin 描述使用 |
| z42-compression | C ABI 压缩扩展，native 动态加载或 wasm 静态链接 |
| z42-repl | host-only 行编辑扩展，通过 C 回调访问运行时 |

## 依赖关系

宿主 z42 依赖 z42 和选定的 interp/JIT；后端依赖 z42。运行时不依赖宿主或具体后端。native 创作 SDK 的 z42-rs/z42-macros/z42-abi 保持 VM-free，独立扩展不会因此引入整台 VM。proc-macro 和 no_std ABI 各有独立编译约束。

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
