# z42-repl

## 职责

拥有 Repl 脚本接口描述、每 VM 回调配置、通用输入和可选终端编辑/历史后端；不依赖运行时 crate。编译求值属于 z42.scripting，静态值反射属于 corelib。

## 功能索引

`native.rs` 导出 `z42_repl_module_v1` 并使用 z42-rs 的通用 HostApi；`backend.rs` 及 helper、editing、highlight、history 实现终端编辑器。关闭 `backend` feature 时不编译 rustyline。

## 基础用法

静态宿主调用模块入口取得 C 描述符；动态宿主从镜像解析同名入口。通用配置和 plain 输入不要求终端后端。终端支持与历史支持分别查询，历史还要求文件系统；缺库时退回 plain 输入。

## 如何测试验证

在 src/runtime 运行 `cargo test -p z42-repl` 和 `cargo test -p z42-repl --no-default-features`；完整验证运行仓库根 `xtask test`。

## 关联文档

[原生模块机制](../../../../docs/internals/src/runtime/native-extensions.md) · [Repl](../../../../docs/internals/src/toolchain/repl.md)

## 核心文件

`src/native.rs` 拥有模块描述符及状态生命周期；`src/native_tests.rs` 覆盖配置隔离、能力边界与回调错误。跨 C 边界只借用同步调用数据，状态由创建它的模块释放。
