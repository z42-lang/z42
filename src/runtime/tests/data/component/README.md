# component — 组件框架测试库

## 职责

A real cdylib using only z42-runtime::components::api. It owns fixture state and uses runtime service callbacks; it never creates a VM or accesses heap/thread state. The same source also supplies static registration tests.

## 功能索引

src/lib.rs 提供两个互有生命周期关系的计数组件、VM 身份回调、实例/初始化/销毁计数，以及错误描述符和部分创建失败入口。

## 基础用法

由宿主组件测试构建和加载，不作为生产库打包。

## 如何测试验证

在 src/runtime/ 运行 `cargo test --lib host::component_tests -- --test-threads=1`。测试在 `artifacts/build/runtime/tests/data/component/<build-id>/` 使用独立 target-dir 离线构建本库，并实际加载生成的动态库；缺少构建工具或产物会失败，不跳过。

## 关联文档

- [组件化运行时](../../../../../docs/internals/src/runtime/componentized-runtime.md)

## 核心文件

Cargo.toml 是独立夹具 workspace，src/lib.rs 同时覆盖静态、动态、依赖和失败场景。
