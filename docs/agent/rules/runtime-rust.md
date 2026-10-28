# Rust VM 开发规范

> 写 `src/runtime` 及 native 扩展时的约定。VM 的机制在书里，入口见[开工导航](../README.md#开工先找-readme-和书)的「虚拟机」行。

## 跑 cargo

- **cargo 只经 xtask 跑，或在 `src/runtime/` 目录里跑，或显式带 `--config src/runtime/.cargo/config.toml`**：仓库根下
  `cargo … --manifest-path src/runtime/Cargo.toml` 读不到这份配置，产物会落进 `src/runtime/target/` 而不是
  `artifacts/build/runtime/`。

## 错误处理

- 可能失败的函数返回 `anyhow::Result<T>`；内部 VM 错误（非用户错误）用 `bail!` / `anyhow!`。
- **非测试代码禁止 `unwrap()` / `expect()`**；测试代码可以用。
- 运行时值类型不匹配时报错，**不做静默转换**。

## 测试文件组织

单元测试放独立文件，不内联在实现文件末尾：`foo.rs` 的测试放同级 `foo_tests.rs`，`foo.rs` 末尾只留
`#[cfg(test)] mod foo_tests;`；集成测试放 crate 级 `tests/`。目的：读实现时不被测试占掉上下文。

## 原生代码与 GC（必须遵守）

触发条件：写 builtin / corelib / native 扩展，或任何让 Rust 代码持有、写入 z42 堆值的地方。四条规则，具体做法、
参照实现与测试形状见[原生代码与 GC 的协作约定](../../internals/src/runtime/native-gc-contract.md)：

1. **根集之外不持有 z42 值**：值优先做成 z42 对象的字段；只能暂存时必须钉住（pin）并由守卫负责释放。
2. **可能长时间不返回的 native 调用必须 park**：park 区间内不分配、不跑 z42 代码、不攥着共享锁。新增会阻塞的
   builtin 要带 park 计数的单测和一次阴性对照。
3. **写堆引用只走带写屏障的原语**，批量写也一样；新增在堆里存引用的布局时，屏障与「开屏障存活 / 关屏障被扫」
   测试随写入原语一起加。
4. **wasm 上不取时钟**：关着的探针不取时钟，始终要跑的计时给 wasm 替代实现。

## 执行模式

- 尚未实现的后端 / 路径**必须**报错（`bail!("... not yet implemented")`），不允许部分实现。
- **实现顺序严格 interp → JIT → AOT**：interp 未全绿不得开始填 JIT / AOT。

## 引入依赖

**触发条件**：要给 Rust VM / native 层加新依赖，或动手实现一个有成熟先例的子系统。

- 优先用成熟开源库，只在无合适库或需要深度定制时自行实现；引入前按 [philosophy.md](philosophy.md) 评估，
  并在 `Cargo.toml` 注明用途。
- 动手前先查已调研的候选库与参考项目：[运行时 · 依赖与参考实现](../../internals/src/runtime/README.md#依赖与参考实现)。
