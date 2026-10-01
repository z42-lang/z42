# 依赖与参考实现

> **触发条件**：要给 Rust VM / native 层加新依赖，或动手实现一个有成熟先例的子系统。
> 原则：优先用成熟开源库，只在无合适库或需要深度定制时自行实现。

z42 自身的编译器、标准库、工具链都是 z42 源码（`src/compiler` / `src/libraries` / `src/toolchain`），
**不引入 C# / .NET 依赖**；下面的推荐只针对 Rust 侧（`src/runtime` 及 native 扩展）。
z42 包之间的依赖按 manifest 声明，见 [docs/reference/](../../reference/)。

## 推荐库（Rust VM）

状态列只标仓库**已采用**的；其余是调研结论，真要引入时仍须按 [philosophy.md](philosophy.md) 评估并在 Cargo.toml 注明用途。

| 用途 | 推荐库 | 状态 | 说明 |
|------|--------|------|------|
| JIT 代码生成 | `cranelift-*` | ✅ 已用 | Bytecode Alliance，Wasmtime 同款（feature `jit`） |
| 二进制格式 | `bincode` | ✅ 已用 | 序列化 `.zbc` |
| 内容哈希 | `blake3` | ✅ 已用 | zbc build_id（split-debug-symbols） |
| AOT / LLVM | `inkwell` | 调研 | LLVM safe bindings for Rust |
| 解析辅助（调试格式） | `nom` 或 `winnow` | 调研 | 文本 IR（`.zasm`）解析 |
| GC（未来沙盒模式） | `gc-arena` | 调研 | arena 式 GC，可选引入 |
| 并发运行时 | `tokio` | 调研 | async VM task 调度 |
| 性能剖析 | `pprof-rs` | 调研 | 火焰图 |
| 文件监听 | `notify` | 调研 | 热更新文件系统事件，跨平台，支持 debounce |

## 参考实现

实现某个子系统前先调研：

| 子系统 | 参考项目 |
|--------|---------|
| 解释器结构 | CPython（ceval.c）、wren、lua 5.4 |
| SSA / IR 设计 | LLVM IR、Cranelift CLIF、QBE |
| 类型推导 | OCaml 编译器、Hindley-Milner 论文实现 |
| JIT 流水线 | LuaJIT、V8 Maglev、JavaScriptCore |
| 模式匹配编译 | `rustc_mir_build`、MLton |
