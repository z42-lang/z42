# stdlib 内部测试

每个 stdlib 包都有自带的 `[Test]`-注解 z42 测试文件，存于 `src/libraries/<lib>/tests/`。由 `z42-test-runner` 工具调度。

## 命令

```bash
./xtask test stdlib                  # 22 个库全跑
./xtask test stdlib z42.math         # 仅指定库
./xtask test stdlib --jobs 4         # 单元级并行（每批 4 个 unit 同时 compile+run）
./xtask test stdlib --mode jit       # 在 JIT 下跑（见下"执行模式"）
```

## 并行（`--jobs`）

`--jobs N` 是 **unit 级 batch 宽度**：每个库的测试 unit 分批，每批 N 个并发
compile（直接 z42c.driver.zpkg）+ run（仿 VM golden 的 `_runVmBatch`）。重叠掉
每 unit 的 z42.core bootstrap（stdlib 测试的主要耗时）。`test all` 默认传 4。
每个 runner 在其 unit 内串行（不再给 runner 传 `--jobs`）→ `[Setup]`/`[Teardown]`
正常执行（旧的 runner `--jobs` 会强制 subprocess 跳过它们）。

## 执行模式（`--mode interp|jit`）

- **interp（默认）**：runner in-process 跑（`runner.rs` → `interp::run_outcome`），
  `[Setup]`/`[Teardown]` 执行。全平台 `test (<平台>)` job 走这条。
- **jit**：in-process runner 无法驱动 JIT（硬编码 interp），故 `--mode jit`
  **强制 subprocess** —— 每个 test fork `z42vm --mode jit`（复用 z42vm 的
  transitive eager-load + cranelift 路径）。`[Setup]`/`[Teardown]` 在 jit 下
  **不跑**（subprocess 限制）。CI 由独立 `test-stdlib-jit(linux-x64)`（job key: stdlib-jit-consistency）
  job 跑，捕获 stdlib 的 interp/JIT 分歧。

## 测试发现机制

1. 编译期 — 每个测试 `.z42` 文件含 `[Test]` / `[Benchmark]` / `[Skip]` / `[ShouldThrow<E>]` 注解的 free function
2. 编译器把这些 metadata 写到 zbc 的 TIDX section
3. `z42-test-runner` 从 zbc 读 TIDX，**默认 in-process**（R3b：`runner.rs` 直调
   `interp::run_outcome`，共享 z42.core，跑完整 `[Setup]`/`[Test]`/`[Teardown]` 链）。
   `--jobs N`（runner 自身的）或 `--mode jit` 会回退到 **subprocess fork**（每 test
   独立 z42vm 实例，跳过 Setup/Teardown）。xtask 的 `--jobs` 是上层 unit 级并行，与
   runner 自身的 `--jobs` 不同（见上"并行"）。
4. 按 stderr 内容分类 Pass / Skip / Fail

详细设计见 [`docs/design/testing/testing.md`](../../design/testing/testing.md) "R 系列实施进度" 段。

## Runner 输出格式

```bash
./xtask test stdlib                         # 默认按 TTY 自动选 pretty / tap
./xtask test stdlib --filter <SUBSTR>       # 子串过滤（转发给 runner 的 --filter）
./xtask test stdlib --no-build              # 跳过工具链重建（用现有产物）

# 更细的 runner flag（--format pretty|tap|json 等）直接对 z42-test-runner：
artifacts/build/runtime/release/z42-test-runner <unit>.zbc --format tap
```

## 加新测试

```z42
// src/libraries/<lib>/tests/MyFeatureTests.z42
namespace Std.<Lib>.Tests;

[Test]
public static void test_basic_case() {
    var actual = SomeFunc(1, 2);
    Assert.Equal(3, actual);
}

[Test]
[ShouldThrow<ArgumentException>]
public static void test_throws_on_invalid_input() {
    SomeFunc(-1, 0);
}
```

写完后 `./xtask test lib <lib>` 即可发现。

## 与编译器单测的区别

| 维度 | 编译器单测（`unit-tests.md`）| stdlib `[Test]`（本文）|
|------|---|---|
| 写在 | z42c 源码 `[Test]` units / Rust `*_tests.rs` | z42 `*Tests.z42` |
| 测什么 | 编译器内部（lexer / parser / TC / IR），经 z42c 自举不动点 | stdlib 源码（运行时行为）|
| Runner | `./xtask test compiler` / `cargo test` | z42-test-runner |
| 加入 GREEN | ✅ | ✅ |
