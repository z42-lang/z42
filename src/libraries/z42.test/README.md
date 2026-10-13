# z42.test

## 职责

z42 的测试库，两半：
- **写测试用**：`TestIO`（console 捕获）、`Bencher`（基准测量）、`TestRunner`（命令式 runner）、集合契约测试；
- **跑测试用**：`Runner` / `BundleRunner`（运行期反射发现 `[Test]` / `[Benchmark]` 并调度）、`TestReport`（pretty / JSON 报告），由 [z42b](../../toolchain/builder/)（`z42b test` / `z42b bench`）与设备端 test-agent 调用。

**不在本包**：`Assert`（及 `TestFailure` / `SkipSignal`）在 **z42.core**（`src/Assert.z42`、`src/Failure.z42`），prelude 可见——断言不只测试代码在用。测试 attribute 类（`TestAttribute` / `BenchmarkAttribute` / `SkipAttribute` / `ShouldThrowAttribute<E>` …）**在本包**（`src/TestAttributes.z42`），测试文件需写 `using Std.Test;`。
attribute 的写法见 `docs/reference/`。

## 功能索引

| 功能 | 入口 |
|------|------|
| 捕获 console 输出 | `TestIO.captureStdout` / `captureStderr` / `captureBoth`（`src/TestIO.z42`） |
| 基准测量 | `Bencher.iter(Action)` / `printSummary` / `Min·Max·Median·Mean·StdDev·Total·Samples`；`BenchHelpers.blackBox`（`src/Bencher.z42`） |
| 命令式 runner（无 lambda） | `TestRunner.Begin` / `Fail` / `Summary`（`src/TestRunner.z42`） |
| bench 摘要解析 | `BenchStats.parse`（`Bencher.printSummary` 输出行的反向解析；执行器据此写 `bench_stats`） |
| 测试 attribute | `[Test]` / `[Benchmark]` / `[Setup]` / `[Teardown]` / `[Skip]` / `[Ignore]` / `[Timeout]` / `[ShouldThrow<E>]`（+ `IExpectsException`）（`src/TestAttributes.z42`）|
| 发现测试 | `TestDiscovery.Discover(path) → TestCase[]`：并入产物，经 `Assembly.GetFunctions()` + 各类型静态方法反射读测试 attribute（`src/TestDiscovery.z42`）|
| 单模块执行 | `Runner.RunModule(path, format)` → 退出码；`Runner.RunModuleResults(path)` → `TestResult[]`（`src/Runner.z42`）|
| bundle 执行 | `BundleRunner.RunBundle(cases, dir, libs)`：golden 经 `ModuleLoader.RunIsolated` 各自隔离、unit 共享 VM、能力门（`src/BundleRunner.z42`）|
| 报告 | 默认 pretty；`--format json` 产 `TestReport.toJson`（`src/TestReport.z42`）|
| 零发现判红 | `Runner.ExitCodeFor(runnable, failed)` |
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

> **`using Std.Test;` 必写，别靠搭便车**。`Assert` 在 z42.core（prelude，免 `using`）；但测试 attribute 与 `Bencher` / `BenchHelpers` /
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
xtask test stdlib z42.test          # 本包 [Test]（含执行器：报告形状、退出码、bundle 能力门）
xtask test stdlib mylib             # 跑某个库的 [Test]（默认 in-process VM，保留 [Setup]/[Teardown]）
xtask test stdlib --jobs 0 mylib    # 并行：0 = available_parallelism；N>1 走 subprocess，[Setup]/[Teardown] 不运行
```

## 关联文档

- 用法与规则（参考手册）：[测试](../../../docs/reference/src/testing.md)
- 设计 / 机制（实现内幕）：[测试框架与 runner](../../../docs/internals/src/testing/framework.md) · [跨平台测试](../../../docs/internals/src/testing/cross-platform.md) · [测试流水线](../../../docs/internals/src/devinfra/test-pipeline.md)

## 核心文件

| 文件 | 类型 | 职责 |
|------|------|------|
| `src/TestIO.z42` | `static class TestIO` + `CaptureResult` | console 捕获 |
| `src/Bencher.z42` | `Bencher` / `static class BenchHelpers` | 基准测量与防优化 |
| `src/BenchStats.z42` | `BenchStats` | benchmark 统计（含 `parse`，供执行器解析 benchmark stdout） |
| `src/TestRunner.z42` | `TestRunner` | 命令式 runner |
| `src/Contracts/BasicCollectionContract.z42` | `static class` | `IBasicCollection` 契约测试 |
| `src/TestDiscovery.z42` | `static class TestDiscovery` + `TestCase` | 运行期反射发现：自由函数 + 静态方法上的测试 attribute → `TestCase`（按全限定名排序）|
| `src/Runner.z42` | `static class Runner` + `InvokeOutcome` | 单模块调度：Setup → 用例 → Teardown、Skip / Ignore 门、`[ShouldThrow<E>]` 按类型可赋值判定 |
| `src/BundleRunner.z42` | `static class BundleRunner` + `BundleCase` | bundle 执行（z42b `--rid host` 与设备 agent 共用） |
| `src/TestReport.z42` | `static class TestReport` + `TestResult` | pretty / JSON 报告 |

## 依赖关系
依赖 `z42.core`（反射、`ModuleLoader`）+ `z42.io`（Console）。纯脚本：发现与调用只用通用反射（`Assembly.GetFunctions` / `MethodInfo.Invoke`），golden 隔离用 core 的 `ModuleLoader.RunIsolated`，不依赖测试专用的 VM 能力。不依赖 z42.json（执行核要随设备端 agent 走，报告 JSON 手搓）。

## 待办
- criterion-style baseline diff
- 类型敏感的泛型 `Assert.Throws<E>(Action)`
- 异步测试 / 参数化测试（待 async/await、collection literals）
