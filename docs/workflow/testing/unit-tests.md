# 编译器 / VM 单元测试

编译器 `z42c` 由 z42 自身编写，正确性由 **z42c 自举不动点 + `[Test]` units** 保证；VM（Rust）单测用 `cargo test`。

## 编译器单测（z42c 自举）

```bash
./xtask test compiler       # 建后端 3 包 + 不动点（gen1==gen2 逐字节）+ [Test] units
```

覆盖：lexer / parser / type-check / IR-gen 经「z42c 自编译自身 + 重建产物逐字节一致」端到端验证，外加编译器源码里的 `[Test]`-注解 units。这条是 GREEN gate 的编译器关。机制见 [`bootstrap.md`](bootstrap.md) 与 [`docs/design/compiler/self-hosting.md`](../../design/compiler/self-hosting.md)。

## VM 单测（Rust）

```bash
cargo test --manifest-path src/runtime/Cargo.toml
```

> 注意（memory `reference_ci_rust_unit_tests_windows_only`）：CI 只在 Windows 腿跑 `cargo test`，易静默腐烂——改 ClassDesc / 反射 / 版本相关代码后**本地必跑** `cargo test`；version bump 还要更 `zbc_reader_tests` 的 version-pin 测试。

## 跑单个 / 部分 test

```bash
# Rust 侧按名字过滤
cargo test --manifest-path src/runtime/Cargo.toml <substr>
```

> 单个编译产物对比（golden）见 [`vm-tests.md`](vm-tests.md)。

## 加新测试

- 编译器内部行为 → 在 `src/compiler/` 对应模块加 `[Test]`-注解 free function（经 `./xtask test compiler` 发现）。
- VM（Rust）单元 → `src/runtime/src/*_tests.rs`。
- 端到端编译产物 → VM golden（见 [`vm-tests.md`](vm-tests.md)）。

参见 [`.claude/rules/workflow.md`](../../../.claude/rules/workflow.md) "测试要求"。
