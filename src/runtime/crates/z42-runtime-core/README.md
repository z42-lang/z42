# z42-runtime-core — 共享运行时核心

## 职责

拥有 VM、对象、GC、PAL 与加载/链接/共享执行机制；不依赖门面、corelib 或具体后端。

## 功能索引

src/vm、src/object、src/gc、src/pal；src/identity.rs 提供物理核心身份和编译期 ABI 摘要。

## 基础用法

由门面通过 RuntimeInputs 注入 builtin、扩展、引擎与配置；worker 共享 Core，不重新装配。 嵌入调用方统一使用 z42::host，不直接依赖组件内部模块。

## 如何测试验证

静态构建运行 `xtask test` 与 `xtask package sdk --verify`。动态构建运行 `xtask package runtime --linkage dynamic --profile debug --verify`（release 同理），实际搬移后的生产组件测试覆盖生命周期和缺失后端错误。源内白盒测试由主 crate 的同源测试模块执行，不复制实现。

## 关联文档

- [组件化运行时](../../../../docs/internals/src/runtime/componentized-runtime.md)
- [宿主规范](../../src/host/README.md)

## 核心文件

src/vm、src/object、src/gc、src/pal；src/identity.rs 提供物理核心身份和编译期 ABI 摘要。

Component contracts belong to components::api. The core owns descriptors, C-layout interface tables, execution registration contracts and same-build identity generation; host re-exports the embedding surface. No separate component protocol crate is required.

The same library target provides rlib and Rust dylib. Static consumers use rlib; desktop dynamic packs select dylib with prefer-dynamic and carry matching shared std. Targets without Rust dylib support retain rlib. The facade itself provides the C embedding library.
