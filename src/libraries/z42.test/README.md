# z42.test

## 职责

z42 标准测试库：给 stdlib 自身和用户脚本提供 `TestIO`（console 捕获）+ `Bencher`（基准测量）+ `TestRunner`（命令式 runner）+ `Runner` / `BundleRunner`（`[Test]` / `[Benchmark]` 发现与调度、`TestReport` 报告），配合 [z42b](../../toolchain/builder/)（`z42b test` / `z42b bench`）运行。
`[Test]` / `[Skip]` / `[Ignore]` / `[Setup]` / `[Teardown]` / `[Benchmark]` / `[ShouldThrow<E>]` / `[Timeout]` 等 attribute 的语法见 `docs/reference/`。

**`Assert` 不在本包**：全仓唯一的 `Std.Assert`（及 `TestFailure` / `SkipSignal`）位于 **z42.core**（`src/Assert.z42` + `src/Failure.z42`），因为断言必须 prelude 可见。本包不提供 Assert 类。

## 功能索引

| 功能 | 入口 |
|------|------|
| 捕获 console 输出 | `TestIO.captureStdout` / `captureStderr` / `captureBoth`（`src/TestIO.z42`） |
| 基准测量 | `Bencher.iter(Action)` / `printSummary` / `Min·Max·Median·Mean·StdDev·Total·Samples`；`BenchHelpers.blackBox`（`src/Bencher.z42`） |
| 命令式 runner（无 lambda） | `TestRunner.Begin` / `Fail` / `Summary`（`src/TestRunner.z42`） |
| `[Test]` / `[Benchmark]` 发现与调度 | `Runner` / `ModuleLoader` / `BundleRunner`；`[Benchmark] void f()` 或 `void f(Bencher b)`（后者编译期 desugar 成前者），与 `[Test]` 同执行路径 |
| 报告 | 默认 pretty（`PASS` / `FAIL` / `SKIP` + `Result:` 汇总）；`z42b {test,bench} --format json` 产 `TestReport`（含 `failure_location` / `stack_trace`，benchmark 条目带 `is_benchmark` + `bench_stats`） |
| 集合契约测试 | `BasicCollectionContract`（`src/Contracts/`） |

## 基础用法

```z42
namespace MyTests;
using Std;
using Std.Test;
using Std.IO;

[Test]
void test_addition() {
    Assert.Equal(4, 2 + 2);
}

[Test]
[ShouldThrow<TestFailure>]
void test_fail_path() {
    Assert.Fail("expected to fail");
}

[Test]
void test_with_capture() {
    var s = TestIO.captureStdout(() => Console.WriteLine("hello"));
    Assert.Equal("hello\n", s);
}

[Test]
void test_with_bench() {
    var b = new Bencher();          // 默认 = 自适应采样（~50ms 预算，n∈[20,2000]）
    var c = new Counter();
    b.iter(() => { c.n = c.n + 1; });
    Assert.True(b.Samples >= 20);   // 显式固定采样用 new Bencher(warmup, samples)
    b.printSummary("counter");
}

class Counter { public int n; public Counter() { this.n = 0; } }
```

命令式 runner（无 attribute，`Main` 返回失败计数作为 exit code）：

```z42
using Std.Test;

void Main() {
    var t = new TestRunner("MyTests");
    t.Begin("Addition");
    try { Assert.Equal(4, 2 + 2); } catch (Exception e) { t.Fail(e); }
    return t.Summary();
}
```

> **`using Std.Test;` 必写，别靠搭便车**。`Assert` 在 z42.core（prelude，免 `using`）；但 `Bencher` / `BenchHelpers` /
> `TestIO` / `BenchStats` 在 z42.test 的 `Std.Test` 命名空间，必须显式 `using Std.Test;`。
> 包激活是**整包**粒度的，只写 `using Std;` 时 `Bencher` 可能编译期静默解析失败、运行期才炸
> （`VCall: … .<unknown>.get_WarmupIters not found`）。机制见
> [project-model.md「激活是整包粒度」](../../../docs/internals/src/compiler/project-model.md)。

已知限制：

- `Assert.Throws(typeName, Action)` 按类型名字符串比对；`Assert.ThrowsAny(Action)` 不断言类型；泛型 `Throws<E>` 待反射能力增强
- z42 lambda 对值类型采用快照捕获语义，要把结果传出 lambda body 须用引用类型（class wrapper / array）
- `BenchHelpers.blackBox` 接 `object` 而非 `<T>`（parser 在表达式上下文不识别方法级显式 generic call）

## 如何测试验证

```bash
xtask test stdlib z42.test          # 本包 [Test]（单元 + runner 行为）
xtask test stdlib mylib             # 跑某个库的 [Test]（默认 in-process VM，保留 [Setup]/[Teardown]）
xtask test stdlib --jobs 0 mylib    # 并行：0 = available_parallelism；N>1 走 subprocess，[Setup]/[Teardown] 不运行
```

## 核心文件

| 文件 | 类型 | 职责 |
|------|------|------|
| `src/TestIO.z42` | `static class TestIO` + `CaptureResult` | console 捕获 |
| `src/Bencher.z42` | `Bencher` / `static class BenchHelpers` | 基准测量与防优化 |
| `src/BenchStats.z42` | `BenchStats` | benchmark 统计（含 `parse`，供 runner 解析 benchmark stdout） |
| `src/TestRunner.z42` | `TestRunner` | 命令式 runner |
| `src/Runner.z42` | `static class Runner` + `InvokeOutcome` | `[Test]` / `[Benchmark]` 调度执行 |
| `src/ModuleLoader.z42` | `static class ModuleLoader` + `TestEntry` | 加载模块并发现测试入口 |
| `src/BundleRunner.z42` | `static class BundleRunner` + `BundleCase` | 嵌入式 test-agent 的 bundle 运行 |
| `src/TestReport.z42` | `static class TestReport` + `TestResult` | pretty / JSON 报告 |
| `src/Contracts/BasicCollectionContract.z42` | `static class` | `IBasicCollection` 契约测试 |

## 依赖关系
依赖 `z42.core` + `z42.io`（`Bencher` 用 `Console.WriteLine`）。纯脚本，无 native 库、无新 builtin。

## 待办
- criterion-style baseline diff
- 类型敏感的泛型 `Assert.Throws<E>(Action)`
- 异步测试 / 参数化测试（待 async/await、collection literals）
