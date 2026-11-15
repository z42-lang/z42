# z42-compression

## 职责

crate 类型：独立运行时库。算法和独立 C API 可脱离 VM 使用；VM 通过 native/interop 接入，由 loader 提供代码，不使用 components 的 VM 组件工厂。

拥有压缩脚本接口、模块注册描述符和算法实现，使用通用 C ABI 与运行时交互，不依赖运行时 crate。

## 功能索引

`registration.rs` 从同一份签名生成接口声明和实现描述符，导出 `z42_compression_module_v1`；`c_api.rs` 保留独立 C API；`compression.rs` 实现算法与流式句柄。

## 基础用法

`backend` feature 启用完整算法，默认开启。关闭时仅提供不可调用的显式接口声明；动态 Core 不依赖该声明 crate，也不预登记 compression。脚本首次调用 compression 模块时，经 loader 找到其自描述入口，由 interop 校验并登记；未使用时不探测、装入、调用描述入口或创建状态。静态解析器先于 OS 动态加载；模块入口与描述符在两种链接方式中一致。

## 如何测试验证

在 src/runtime 运行 `cargo test -p z42-compression`；仓库根运行 `xtask test stdlib z42.compression` 和完整 `xtask test`。WASM 的 Zstandard 保留不支持错误。

## 关联文档

[压缩模块](../../../../docs/internals/src/runtime/native-ext-loader.md) · [原生模块机制](../../../../docs/internals/src/runtime/native-extensions.md)

## 核心文件

`src/registration.rs` 是脚本描述符的唯一来源；`src/compression_tests.rs` 验证算法与传统 C API。C 值由接收方复制，流式句柄由算法后端释放。

链接组合所需的 C 符号解析器声明位于本 crate 的 `z42_compression.h`，由 native 统一头聚合；普通宿主无需额外包含模块头。
