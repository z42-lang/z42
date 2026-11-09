# interop — 原生能力注册与调用桥接

## 职责

描述、注册和查询 native 提供方、函数与类型，并实现 Tier 1 C ABI 的编组与调用适配。目录不实现压缩或终端业务，也不负责库路径探测与动态装入。

## 功能索引

| 入口 | 职责 |
|---|---|
| `providers.rs` | `NativeProvider`、事务式 `RegistryBuilder` 与不可变 `NativeRegistry`；统一生成 builtin 查询目录和执行表 |
| `module.rs`、`native_call.rs`、`native_value.rs` | 通用模块描述符校验、每 VM 实例、C 值复制及脚本回调 |
| `state.rs` | 每 VM 的模块实例、native 类型与 pin buffer；worker 共享所属 VM 的状态 |
| `tier1/registry.rs` | 校验类型描述符、签名并构建 libffi 调用记录 |
| `tier1/call.rs`、`dispatch.rs`、`marshal.rs` | 已注册类型方法的参数转换、调用和结果转换 |
| `tier1/exports.rs`、`error.rs` | C 注册/查询入口、活动 VM 和线程错误通道 |

## 基础用法

提供方提交名称和 `BuiltinDesc` 切片。`RegistryBuilder::register` 在修改前检查完整切片，空名称、重复提供方或同一提供方内重复函数名称均报错；失败不发布部分条目。`freeze` 产生不可变快照，`resolve(provider, name)` 只查询指定提供方。不同提供方允许同名导出；存在歧义的裸名不解析，避免按装配顺序覆盖。

独立 crate 通过 `RegistryBuilder::register_module` 提交版本化 C 描述符，静态和动态入口共用校验及调用路径。默认提供方由 `host/builtins.rs` 选择：`z42.core`、`z42_repl` 与 `z42_compression`。corelib 只维护自身描述符。metadata 的 `BuiltinCatalog` 保存名称/ID/折叠事实，exec 通过同序的执行表调用；ID 不跨进程持久化。

Tier 1 外部类型仍按 `(module, name)` 在每 VM 注册。`loader::native::load_library` 装入库并在指定 VM 上调用注册入口；PAL 负责操作系统原语。

## 如何测试验证

在 `src/runtime/` 运行 `cargo test -p z42 --lib interop::`，完整验证运行 `./xtask test`。

## 关联文档

- [原生提供方与扩展](../../../../docs/internals/src/runtime/native-extensions.md)
- [原生 ABI](../../../../docs/internals/src/runtime/native-abi.md)

## 核心文件

`providers.rs` 管理内置函数提供方快照；`state.rs` 管理每 VM 的外部类型与 buffer。二者均不承担脚本类型身份（归 metadata）或动态库代码寿命（归 loader）。

## 当前边界

带 `lib` 但不带 `type` 的脚本声明仍生成按名 builtin，外部类型声明使用 `CallNative`。C 入口 `z42_invoke` / `z42_invoke_method` 尚未实现反向调用；通用模块 ABI 的 `HostApi.invoke_script` 已实现同步脚本回调。旧 Tier 1 类型 ABI 仍独立，不能据此视为已实现其反向入口。
