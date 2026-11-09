# z42-compression

## 职责

拥有压缩脚本接口、模块注册描述符和算法实现，使用通用 C ABI 与运行时交互，不依赖运行时 crate。

## 功能索引

`native.rs` 提供脚本签名、调用适配及 `z42_compression_module_v1`；`backend.rs` 保留独立传统 C API；`compression.rs` 实现算法与流式句柄。

## 基础用法

`backend` feature 启用完整算法，默认开启。关闭时只链接轻量注册代码，首次调用通过宿主符号解析器转发到完整模块镜像。静态解析器先于 OS 动态加载；模块入口与描述符在两种链接方式中一致。

## 如何测试验证

在 src/runtime 运行 `cargo test -p z42-compression`；仓库根运行 `xtask test stdlib z42.compression` 和完整 `xtask test`。WASM 的 Zstandard 保留不支持错误。

## 关联文档

[压缩模块](../../../../docs/internals/src/runtime/native-ext-loader.md) · [原生模块机制](../../../../docs/internals/src/runtime/native-extensions.md)

## 核心文件

`src/native.rs` 是脚本描述符的唯一来源；`src/compression_tests.rs` 验证算法与传统 C API。C 值由接收方复制，流式句柄由算法后端释放。
