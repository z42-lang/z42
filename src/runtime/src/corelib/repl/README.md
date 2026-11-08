# repl — REPL 回调与输入

## 职责

提供补全、按键编辑和输入 builtin；编辑器本体位于独立的 z42-repl cdylib。VM 侧负责回调重入、字符串边界和 plain stdin 降级。

## 功能索引

| 文件 | 职责 |
|---|---|
| `mod.rs` | readline、补全回调与 C trampoline |
| `editing.rs` | 按键回调与关键词设置 |
| `native.rs` | 懒加载编辑器、符号绑定和进程级加载结果缓存 |
| `paths.rs` | 门面注入的候选路径服务 |

## 基础用法

z42 用户继续使用原有 REPL API。Rust 的 `editing::repl_set_keywords` 接收 `&VmContext` 与关键词字符串，返回 `anyhow::Result<()>`；builtin 自动提供当前上下文。

候选路径策略由门面装配，只在首次关键词或 readline 请求加载编辑器时执行。缺库结果也缓存；后续上下文和 worker 复用同一个进程级结果，不重新探测。没有 native-interop 时关键词设置仍为空操作。

## 如何测试验证

运行 `./xtask test` 与 `./xtask test runtime`；定向测试为 `(cd src/runtime && cargo test --lib corelib::repl::)`。SDK 的 REPL 发行夹具由 `./xtask package sdk --verify` 执行。

## 关联文档

- [原生扩展](../../../../../docs/internals/src/runtime/native-extensions.md)
- [原生库布局](../../../../../docs/internals/src/runtime/native-libraries.md)

## 核心文件

`native.rs` 保持 cdylib 生命周期和回调边界；`paths.rs` 只声明路径策略接口，不读取配置。
