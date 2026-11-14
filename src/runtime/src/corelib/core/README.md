# corelib/core/

## 职责

实现标准库的基础 builtin，包括值操作、反射、线程与同步、运行时查询。实现经门面注册到 VM 的 builtin 表。

## 功能索引

| 功能 | 入口 / 文件 |
|---|---|
| 值转换、字符串、数组、数学与字符 | `convert.rs`、`string.rs`、`array.rs`、`math.rs`、`char.rs` |
| 对象与反射、程序集加载上下文 | `object.rs`、`reflection/`、`assemblyloadcontext.rs` |
| 线程、Monitor 与争用探针 | `threading.rs`、`monitor.rs`、`sync_contention.rs` |
| GC、配置、诊断与计时 | `gc.rs`、`config.rs`、`appprops.rs`、`diagnostics.rs`、`bench.rs` |
| 加密与符号可用性 | `crypto.rs`、`symavail.rs` |

## 如何测试验证

在仓库根运行 `./xtask test runtime`。定向运行本组 Rust 单元测试可用以下命令。

```bash
(cd src/runtime && cargo test --lib corelib::core::)
```

## 关联文档

- [Native ABI](../../../../../docs/internals/src/runtime/native-abi.md)
- [同步原语](../../../../../docs/internals/src/runtime/sync-primitives.md)

## 核心文件

`config.rs` 的模块目录查询从所属 VM 加载器取快照，`config_tests.rs` 覆盖 VM 隔离与 worker 共享。

`mod.rs` 汇总基础能力模块，各实现旁保留对应测试；反射子目录按查询、成员访问、调用和加载职责组织。
