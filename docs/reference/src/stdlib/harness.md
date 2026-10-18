<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.harness/ -->

# z42.harness —— 测试框架：声明（attribute）、发现（反射）、运行与报告。

> 包路径 `src/libraries/z42.harness/`；命名空间 `Std.Harness`

写法：零参的自由函数或静态方法贴 `[Test]`，工具链编译后由本包的 [`Runner`] 反射
发现并逐条执行；`[Benchmark]` 基准、`[Setup]` / `[Teardown]` 夹具、`[Skip]` /
`[Ignore]` 跳过、`[ShouldThrow<E>]` 期望异常。断言用 `Assert`——它在 z42.core 的
prelude 里（`Assert` / `TestFailure` 不在本包），测试文件不用为本包额外 using 之外的
断言引任何依赖。

组成：

- 声明：`TestAttributes.z42` 的 attribute 族（作者与 runner 共享的契约）；
- 发现：[`TestDiscovery`]——`Assembly.GetFunctions()` + 各类型静态方法，只读 attribute
  活实例，不依赖产物里的测试专用元数据或 VM 测试能力；
- 运行：[`Runner`]（pretty / json 两种输出；`RunModuleResults` 供跨模块聚合）；
- 基准：[`Bencher`] 计时采样与摘要行，`BenchStats` 把摘要行解析成结构化数据；
- 输出捕获：`TestIO`——捕获被测代码的 stdout / stderr，嵌套按栈组合；
- 无反射的命令式 runner：`TestRunner`（Begin / Fail / Summary 三段式）。

## 不支持

- 没有并行测试：条目按全限定名排序顺序执行。
- `[Timeout]` 只做编译期校验（毫秒必须为正），运行期不强制超时。
- 没有 parameterized / 数据驱动测试：`[Test]` 必须零参，参数化要自己写循环。
- `[Setup]` / `[Teardown]` 按命名空间（或同一类型）成组，没有共享实例的类级夹具。
- `TestResult` 没有 duration / 失败位置 / 栈：运行期没有可靠来源，略去而不是编造。
- 报告 JSON 由 `TestReport` 手写（本包零依赖、不引 z42.json），schema 见该类文档。

## BenchStats

`src/libraries/z42.harness/src/BenchStats.z42:10`

基准摘要行（[`Bencher.printSummary`] 的输出）解析出的结构化数据，挂在 `TestResult.bench_stats` 上。行格式：

bench[<label>] min=<n>ns median=<n>ns max=<n>ns mean=<n>ns stddev=<n>ns samples=<n>

`mean=` / `stddev=` 可缺：缺时 `MeanNs` / `StdDevNs` 为 -1（消费方退化为裸比值判定，
不会误报回归）。

```z42
public class BenchStats {
    string Label
    long MinNs
    long MedianNs
    long MaxNs
    long MeanNs
    long StdDevNs
    long Samples
    BenchStats(string label, long minNs, long medianNs, long maxNs, long meanNs, long stddevNs, long samples)
    static BenchStats? parse(string text)
}
```

### string Label

`src/libraries/z42.harness/src/BenchStats.z42:11`


### long MinNs

`src/libraries/z42.harness/src/BenchStats.z42:12`


### long MedianNs

`src/libraries/z42.harness/src/BenchStats.z42:13`


### long MaxNs

`src/libraries/z42.harness/src/BenchStats.z42:14`


### long MeanNs

`src/libraries/z42.harness/src/BenchStats.z42:15`


### long StdDevNs

`src/libraries/z42.harness/src/BenchStats.z42:16`


### long Samples

`src/libraries/z42.harness/src/BenchStats.z42:17`


### BenchStats(string label, long minNs, long medianNs, long maxNs, long meanNs, long stddevNs, long samples)

`src/libraries/z42.harness/src/BenchStats.z42:19`


### static BenchStats? parse(string text)

`src/libraries/z42.harness/src/BenchStats.z42:35`

从 `text`（一个基准的捕获 stdout，可能多行）里解析**最后**一条合法的 `bench[...]` 行；一条也解析不出来返回 null（调用方按「这个基准没有 统计」处理，而不是猜字段）。

| 参数 | 说明 |
|---|---|
| `text` | 基准的完整捕获输出；杂散输出在摘要行之前时取最后一条最稳 |

## Bencher

`src/libraries/z42.harness/src/Bencher.z42:18`

一段代码的耗时采样器：预热后采样，给出 min / median / max / mean / stddev / total。

用法：

    var b = new Bencher();              // 自适应采样
    b.iter(() => doWork());
    b.printSummary("doWork");           // bench[doWork] min=... median=... max=...

`[Benchmark]` 基准两种签名等价：零参（自己 new Bencher + printSummary），
或唯一形参 `Bencher`（runner 传入并在跑完时按函数名自动打摘要行）。

> ⚠️ 计时是单线程墙钟（`Clock.MonoNanos()`），不是 CPU 时间；被并发干扰的 基准会反映为 max 偏大，而不是报错。

```z42
public class Bencher {
    int WarmupIters { get; private set; }
    int Samples { get; private set; }
    long MinNs { get; private set; }
    long MaxNs { get; private set; }
    long MedianNs { get; private set; }
    long MeanNs { get; private set; }
    long StdDevNs { get; private set; }
    long TotalNs { get; private set; }
    Bencher()
    Bencher(int warmupIters, int sampleIters)
    void iter(Action body)
    void printSummary(string label)
}
```

### int WarmupIters { get; private set; }

`src/libraries/z42.harness/src/Bencher.z42:19`


### int Samples { get; private set; }

`src/libraries/z42.harness/src/Bencher.z42:20`


### long MinNs { get; private set; }

`src/libraries/z42.harness/src/Bencher.z42:21`


### long MaxNs { get; private set; }

`src/libraries/z42.harness/src/Bencher.z42:22`


### long MedianNs { get; private set; }

`src/libraries/z42.harness/src/Bencher.z42:23`


### long MeanNs { get; private set; }

`src/libraries/z42.harness/src/Bencher.z42:24`


### long StdDevNs { get; private set; }

`src/libraries/z42.harness/src/Bencher.z42:25`


### long TotalNs { get; private set; }

`src/libraries/z42.harness/src/Bencher.z42:26`


### Bencher()

`src/libraries/z42.harness/src/Bencher.z42:33`

自适应采样：预热 10 次；先用一小段 pilot 估出单次耗时，再按约 50ms 的测量 预算选采样数（夹在 [20, 2000]）。快的基准采得多（置信区间紧），慢的采得少 （墙钟时间有上界）。`iter` 之后 `Samples` 是实际用的 n。

### Bencher(int warmupIters, int sampleIters)

`src/libraries/z42.harness/src/Bencher.z42:41`

固定采样：预热 `warmupIters` 次、计时 `sampleIters` 次，不做自适应。

### void iter(Action body)

`src/libraries/z42.harness/src/Bencher.z42:58`

预热 `WarmupIters` 次（不计时），再采样 n 次（n 固定或自适应）。返回后 `MinNs` / `MedianNs` / `MaxNs` / `MeanNs` / `StdDevNs` / `TotalNs` / `Samples` 反映这次采样。

| 参数 | 说明 |
|---|---|
| `body` | 被测代码；每次采样独立计时 |

### void printSummary(string label)

`src/libraries/z42.harness/src/Bencher.z42:141`

打一行人类可读的摘要，格式是 runner 契约的一部分（json 模式下 runner 捕获这行 并经 `BenchStats.parse` 解析成 `TestResult.bench_stats`）：

bench[<label>] min=<n>ns median=<n>ns max=<n>ns mean=<n>ns stddev=<n>ns samples=<n>

| 参数 | 说明 |
|---|---|
| `label` | 摘要行里的名字；`[Benchmark] void f(Bencher b)` 形态由 runner 按函数名传入 |

## BenchHelpers

`src/libraries/z42.harness/src/Bencher.z42:167`


```z42
public static class BenchHelpers {
    static extern object blackBox(object value)
}
```

### static extern object blackBox(object value)

`src/libraries/z42.harness/src/Bencher.z42:175`

「别把这次计算优化掉」的恒等包装（对应 criterion 的 black_box）。目前解释器 不做死代码消除，恒等返回；留着这个形口，用户代码在 JIT 学会消除纯表达式后 无需改动。可无条件使用。

> ⚠️ 形参是 `object` 而非 `<T>`：跨 zpkg 的方法级泛型实参推断尚未就绪， 装箱开销由调用方自负。

## BundleCase

`src/libraries/z42.harness/src/BundleRunner.z42:8`

测试束里的一条。`Entry` 为 null 标记单元测试（`[Test]` 模块）；非 null（golden 程序的入口符号）标记整程序 golden。`Zbc` / `Expected` 是相对束目录的文件名； `Requires` 是该条声明的能力（源码 `// requires-caps:` 经 manifest 带来），空 = 无。

```z42
public class BundleCase {
    string Name
    string Zbc
    string Entry
    string Expected
    string[] Requires
    BundleCase(string name, string zbc, string entry, string expected)
    bool IsGolden()
}
```

### string Name

`src/libraries/z42.harness/src/BundleRunner.z42:9`


### string Zbc

`src/libraries/z42.harness/src/BundleRunner.z42:10`


### string Entry

`src/libraries/z42.harness/src/BundleRunner.z42:12`

null → 单元测试；非 null → golden 的入口符号。

### string Expected

`src/libraries/z42.harness/src/BundleRunner.z42:14`

golden 的期望输出文件名（可为 null / ""）。

### string[] Requires

`src/libraries/z42.harness/src/BundleRunner.z42:15`


### BundleCase(string name, string zbc, string entry, string expected)

`src/libraries/z42.harness/src/BundleRunner.z42:17`


### bool IsGolden()

`src/libraries/z42.harness/src/BundleRunner.z42:25`


## BundleRunner

`src/libraries/z42.harness/src/BundleRunner.z42:34`

测试束执行器：golden（整程序，隔离 VM 里跑、比对 stdout）+ 单元（共享 VM 的 `[Test]` 模块，经 [`Runner.RunModuleResults`]）+ 汇总。设备端 test-agent 与 宿主 `z42b test --rid host` 共用这套算法。

manifest 本身是 JSON，但本包零依赖：调用方用自己的 z42.json 把 manifest.json
解析成 `BundleCase[]` 再传进来。

```z42
public static class BundleRunner {
    static TestResult[] RunBundle(BundleCase[] allCases, string bundleDir, string libsDir)
    static int ExitCode(TestResult[] results)
}
```

### static TestResult[] RunBundle(BundleCase[] allCases, string bundleDir, string libsDir)

`src/libraries/z42.harness/src/BundleRunner.z42:44`

跑 `cases` 里的每一条并返回汇总结果（golden 在前、单元在后，缺能力被跳过的 条目排在末尾）。

能力门在加载之前：声明的能力本 VM 缺失的条目直接记 skipped、不加载（加载
本身就可能 trap——没有 native interop 的 VM 上 `[Native]` 门面在解析器里 panic）。

| 参数 | 说明 |
|---|---|
| `allCases` | 束内全部条目 |
| `bundleDir` | 存放 zbc / 期望输出文件的目录 |
| `libsDir` | 隔离 golden VM 的 stdlib 目录（"" → `Z42_LIBS`，否则 `<bundleDir>/../libs`） |

### static int ExitCode(TestResult[] results)

`src/libraries/z42.harness/src/BundleRunner.z42:140`

束结果的退出码：任一条 failed → 1，否则 0。

## InvokeOutcome

`src/libraries/z42.harness/src/Runner.z42:33`

一次调用的结果（z42 无 out 参数，用字段带出）。`Captured` 只在 json 模式下 捕获被测函数的 stdout（供基准摘要行解析），其余情况为 `""`。

```z42
public class InvokeOutcome {
    bool Threw
    Type? Thrown
    string ErrType
    string ErrMsg
    string Captured
    InvokeOutcome()
}
```

### bool Threw

`src/libraries/z42.harness/src/Runner.z42:34`


### Type? Thrown

`src/libraries/z42.harness/src/Runner.z42:35`


### string ErrType

`src/libraries/z42.harness/src/Runner.z42:36`


### string ErrMsg

`src/libraries/z42.harness/src/Runner.z42:37`


### string Captured

`src/libraries/z42.harness/src/Runner.z42:38`


### InvokeOutcome()

`src/libraries/z42.harness/src/Runner.z42:39`


## Runner

`src/libraries/z42.harness/src/Runner.z42:46`

反射式测试 runner：并入一个编好的测试 / bench 产物，经 [`TestDiscovery`] 找出 `[Test]` / `[Benchmark]` / `[Setup]` / `[Teardown]`，逐条执行并归类 pass / fail / skip。

```z42
public static class Runner {
    static int RunModule(string path, string format)
    static int ExitCodeFor(int runnable, int failed)
    static TestResult[] RunModuleResults(string path)
}
```

### static int RunModule(string path, string format)

`src/libraries/z42.harness/src/Runner.z42:50`

并入 `path` 处的产物并跑其中每条 `[Test]` / `[Benchmark]`，按 `format` （`"pretty"` | `"json"`）输出。返回进程退出码：0 = 全过；1 = 有失败，或一条 可跑的测试都没发现（拼错 attribute、glob 漏文件都算事故，不与「全过」等价）。

### static int ExitCodeFor(int runnable, int failed)

`src/libraries/z42.harness/src/Runner.z42:104`

退出码决策（纯函数，可直接单测）：一条可跑的测试都没有 → 1（「测试没跑」 不得与「全过」等价）；有失败 → 1；否则 0。

### static TestResult[] RunModuleResults(string path)

`src/libraries/z42.harness/src/Runner.z42:112`

跑 `path` 处产物里的每条 `[Test]` / `[Benchmark]`，**只返回结果、不打印**。 供跨模块聚合：调用方收集多个模块的结果后统一渲染一份合并报告。

## TestAttribute

`src/libraries/z42.harness/src/TestAttributes.z42:15`

一条测试：零参、返回 void 的自由函数或静态方法。

```z42
public sealed class TestAttribute : Attribute {
    TestAttribute()
}
```

### TestAttribute()

`src/libraries/z42.harness/src/TestAttributes.z42:16`


## BenchmarkAttribute

`src/libraries/z42.harness/src/TestAttributes.z42:20`

一条基准：零参，或唯一参数为 [`Bencher`]（`void f(Bencher b)`——runner 传入并自动打摘要行）。

```z42
public sealed class BenchmarkAttribute : Attribute {
    BenchmarkAttribute()
}
```

### BenchmarkAttribute()

`src/libraries/z42.harness/src/TestAttributes.z42:21`


## SetupAttribute

`src/libraries/z42.harness/src/TestAttributes.z42:25`

同命名空间（或同一类型）里每条测试之前执行。

```z42
public sealed class SetupAttribute : Attribute {
    SetupAttribute()
}
```

### SetupAttribute()

`src/libraries/z42.harness/src/TestAttributes.z42:26`


## TeardownAttribute

`src/libraries/z42.harness/src/TestAttributes.z42:30`

同命名空间（或同一类型）里每条测试之后执行（测试失败也执行）。

```z42
public sealed class TeardownAttribute : Attribute {
    TeardownAttribute()
}
```

### TeardownAttribute()

`src/libraries/z42.harness/src/TestAttributes.z42:31`


## SkipAttribute

`src/libraries/z42.harness/src/TestAttributes.z42:36`

跳过本条。`reason` 必填（E0914）；给 `platform` ⇒ 仅在该平台跳过；给 `feature` ⇒ 运行时缺这项能力才跳过（能力集见 `Std.Platform.Capabilities()`，未知能力名按缺失处理）。

```z42
public sealed class SkipAttribute : Attribute {
    string Reason
    string Platform
    string Feature
    SkipAttribute(string reason = "", string platform = "", string feature = "")
}
```

### string Reason

`src/libraries/z42.harness/src/TestAttributes.z42:38`

必填：跳过的原因（报告里随 SKIP 行输出）。

### string Platform

`src/libraries/z42.harness/src/TestAttributes.z42:40`

平台门：等于 `Std.Platform.OS()` 的返回值时跳过。

### string Feature

`src/libraries/z42.harness/src/TestAttributes.z42:42`

能力门：运行中的 VM 不报告这项能力时跳过。

### SkipAttribute(string reason = "", string platform = "", string feature = "")

`src/libraries/z42.harness/src/TestAttributes.z42:43`


## IgnoreAttribute

`src/libraries/z42.harness/src/TestAttributes.z42:51`

无条件跳过（不报理由）。

```z42
public sealed class IgnoreAttribute : Attribute {
    IgnoreAttribute()
}
```

### IgnoreAttribute()

`src/libraries/z42.harness/src/TestAttributes.z42:52`


## TimeoutAttribute

`src/libraries/z42.harness/src/TestAttributes.z42:56`

超时（毫秒，必须为正：E0917）。**目前只有编译期校验，runner 运行期不强制。**

```z42
public sealed class TimeoutAttribute : Attribute {
    int Milliseconds
    TimeoutAttribute(int milliseconds = 0)
}
```

### int Milliseconds

`src/libraries/z42.harness/src/TestAttributes.z42:58`

超时毫秒数（必须为正）。

### TimeoutAttribute(int milliseconds = 0)

`src/libraries/z42.harness/src/TestAttributes.z42:59`


## ShouldThrowAttribute

`src/libraries/z42.harness/src/TestAttributes.z42:63`

期望抛出 `E`（或其子类型）。runner 经 [`IExpectsException`] 取到 `E`，不必知道类型实参。

```z42
public sealed class ShouldThrowAttribute<E> : Attribute, IExpectsException {
    ShouldThrowAttribute()
    Type? ExceptionType()
}
```

### ShouldThrowAttribute()

`src/libraries/z42.harness/src/TestAttributes.z42:64`


### Type? ExceptionType()

`src/libraries/z42.harness/src/TestAttributes.z42:65`


## ShouldThrowAttribute

`src/libraries/z42.harness/src/TestAttributes.z42:69`

漏写类型实参的 `[ShouldThrow]` 落到这里（E0913 照报）；它不期望任何具体类型。

```z42
public sealed class ShouldThrowAttribute : Attribute, IExpectsException {
    ShouldThrowAttribute()
    Type? ExceptionType()
}
```

### ShouldThrowAttribute()

`src/libraries/z42.harness/src/TestAttributes.z42:70`


### Type? ExceptionType()

`src/libraries/z42.harness/src/TestAttributes.z42:71`


## IExpectsException

`src/libraries/z42.harness/src/TestAttributes.z42:75`

「期望抛出某类型」的 attribute 统一口径（runner 只认这个接口）。

```z42
public interface IExpectsException {
    Type? ExceptionType()
}
```

### Type? ExceptionType()

`src/libraries/z42.harness/src/TestAttributes.z42:76`


## TestCase

`src/libraries/z42.harness/src/TestDiscovery.z42:6`

一条被发现的测试 / bench / 生命周期函数。

```z42
public class TestCase {
    string Qualified
    int Kind
    MethodInfo Method
    bool Skipped
    bool Ignored
    string? SkipReason
    string? SkipPlatform
    string? SkipFeature
    bool ShouldThrow
    Type? Expected
    TestCase(string qualified, int kind, MethodInfo method)
    bool IsTest()
    bool IsBenchmark()
    bool IsSetup()
    bool IsTeardown()
    bool IsSkipped()
    bool IsIgnored()
    bool HasShouldThrow()
    string DeclaringType()
    void Invoke()
    bool TakesBencher()
}
```

### string Qualified

`src/libraries/z42.harness/src/TestDiscovery.z42:8`

报告用的名字：`<命名空间或类型>.<函数名>`。

### int Kind

`src/libraries/z42.harness/src/TestDiscovery.z42:10`

1=Test, 2=Benchmark, 3=Setup, 4=Teardown.

### MethodInfo Method

`src/libraries/z42.harness/src/TestDiscovery.z42:11`


### bool Skipped

`src/libraries/z42.harness/src/TestDiscovery.z42:12`


### bool Ignored

`src/libraries/z42.harness/src/TestDiscovery.z42:13`


### string? SkipReason

`src/libraries/z42.harness/src/TestDiscovery.z42:15`

`[Skip]` 的三个实参；没给的为 null。

### string? SkipPlatform

`src/libraries/z42.harness/src/TestDiscovery.z42:16`


### string? SkipFeature

`src/libraries/z42.harness/src/TestDiscovery.z42:17`


### bool ShouldThrow

`src/libraries/z42.harness/src/TestDiscovery.z42:19`

`[ShouldThrow<E>]`：贴了即 true；`Expected` 为 E（漏写类型实参时为 null）。

### Type? Expected

`src/libraries/z42.harness/src/TestDiscovery.z42:20`


### TestCase(string qualified, int kind, MethodInfo method)

`src/libraries/z42.harness/src/TestDiscovery.z42:22`


### bool IsTest()

`src/libraries/z42.harness/src/TestDiscovery.z42:29`


### bool IsBenchmark()

`src/libraries/z42.harness/src/TestDiscovery.z42:30`


### bool IsSetup()

`src/libraries/z42.harness/src/TestDiscovery.z42:31`


### bool IsTeardown()

`src/libraries/z42.harness/src/TestDiscovery.z42:32`


### bool IsSkipped()

`src/libraries/z42.harness/src/TestDiscovery.z42:33`


### bool IsIgnored()

`src/libraries/z42.harness/src/TestDiscovery.z42:34`


### bool HasShouldThrow()

`src/libraries/z42.harness/src/TestDiscovery.z42:35`


### string DeclaringType()

`src/libraries/z42.harness/src/TestDiscovery.z42:38`

所属命名空间（自由函数）或类型（静态方法）的全限定名——`[Setup]` / `[Teardown]` 按它成组。

### void Invoke()

`src/libraries/z42.harness/src/TestDiscovery.z42:49`

调用（无接收者）。被调函数里抛出的异常按原类型传出。

零参形态直接调（`[Test]` / `[Setup]` / `[Teardown]` 及自建 [`Bencher`] 的
`[Benchmark]`）；`[Benchmark] void f(Bencher b)` 形态由这里建 [`Bencher`] 传入，
跑完按函数名打摘要行（与自建形态手写 `b.printSummary("f")` 输出同一行）。

### bool TakesBencher()

`src/libraries/z42.harness/src/TestDiscovery.z42:62`

恰好一个 `Std.Harness.Bencher` 形参（编译期 E0912 已保证 `[Benchmark]` 只有零参与这一种形状）。

## TestDiscovery

`src/libraries/z42.harness/src/TestDiscovery.z42:78`

运行期反射发现测试：把编好的测试 / bench 产物并入当前 VM，经反射找出贴了 `[Test]` / `[Benchmark]` / `[Setup]` / `[Teardown]` 的函数，连同 `[Skip]` / `[Ignore]` / `[ShouldThrow<E>]` 一起读成 [`TestCase`]。

发现面 = 产物的自由函数（`Assembly.GetFunctions()`）+ 各类型的静态方法（`Type.GetMethods()`）；
只读 attribute 活实例，不依赖编译器写进产物的测试专用元数据，也不依赖测试专用的 VM 能力。
结果按全限定名排序：同输入同输出，便于比对报告。

```z42
public static class TestDiscovery {
    static TestCase[] Discover(string path)
}
```

### static TestCase[] Discover(string path)

`src/libraries/z42.harness/src/TestDiscovery.z42:80`

并入 `path`（.zpkg / .zbc）并发现其中的测试。加载失败抛异常。

## CaptureResult

`src/libraries/z42.harness/src/TestIO.z42:4`

一次双通道捕获的结果。

```z42
public class CaptureResult {
    string Stdout { get; private set; }
    string Stderr { get; private set; }
    CaptureResult(string stdout, string stderr)
}
```

### string Stdout { get; private set; }

`src/libraries/z42.harness/src/TestIO.z42:5`


### string Stderr { get; private set; }

`src/libraries/z42.harness/src/TestIO.z42:6`


### CaptureResult(string stdout, string stderr)

`src/libraries/z42.harness/src/TestIO.z42:8`


## TestIO

`src/libraries/z42.harness/src/TestIO.z42:21`

捕获被测代码写到 stdout / stderr 的输出。

每次捕获压入一个线程局部缓冲；捕获可嵌套，按栈组合——内层只看到内层的输出，
外层只看到内层窗口之外执行期间产生的输出。

> ⚠️ `body` 抛出时缓冲同样会卸载（不泄漏 sink），异常原样透传；拿到返回值 只在成功路径上。

```z42
public static class TestIO {
    static string captureStdout(Action body)
    static string captureStderr(Action body)
    static CaptureResult captureBoth(Action body)
}
```

### static string captureStdout(Action body)

`src/libraries/z42.harness/src/TestIO.z42:23`

捕获 `body` 执行期间写到的 stdout，返回捕获文本。

### static string captureStderr(Action body)

`src/libraries/z42.harness/src/TestIO.z42:35`

只捕获 stderr；stdout 照常穿透到进程输出。

### static CaptureResult captureBoth(Action body)

`src/libraries/z42.harness/src/TestIO.z42:47`

双通道各自独立捕获。

## TestResult

`src/libraries/z42.harness/src/TestReport.z42:6`

一条测试 / 基准的运行结果。没有 duration / 失败位置 / 栈字段：进程内 runner 没有可靠来源，略去而不是编造。

```z42
public class TestResult {
    string Name
    string Status
    bool IsBenchmark
    string Reason
    BenchStats Stats
    TestResult(string name, string status, bool isBenchmark, string reason, BenchStats stats)
}
```

### string Name

`src/libraries/z42.harness/src/TestReport.z42:8`

全限定名：`<命名空间或类型>.<函数名>`。

### string Status

`src/libraries/z42.harness/src/TestReport.z42:10`

`"passed"` | `"failed"` | `"skipped"`。

### bool IsBenchmark

`src/libraries/z42.harness/src/TestReport.z42:11`


### string Reason

`src/libraries/z42.harness/src/TestReport.z42:13`

仅失败 / 跳过时非 null。

### BenchStats Stats

`src/libraries/z42.harness/src/TestReport.z42:15`

仅基准且摘要行解析成功时非 null。

### TestResult(string name, string status, bool isBenchmark, string reason, BenchStats stats)

`src/libraries/z42.harness/src/TestReport.z42:17`


## TestReport

`src/libraries/z42.harness/src/TestReport.z42:32`

runner `--format json` 模式的报告序列化。本包零依赖，JSON 手写而非经 z42.json。 schema：

{ "tool":"z42b", "module":"<path>",
      "summary":{"total":N,"passed":P,"failed":F,"skipped":S},
      "results":[ {"name":..,"status":..,"is_benchmark":..[,"reason":..][,"bench_stats":..]} ] }

```z42
public static class TestReport {
    static string toJson(string artifact, TestResult[] results)
    static string resultJson(TestResult r)
    static string esc(string str)
}
```

### static string toJson(string artifact, TestResult[] results)

`src/libraries/z42.harness/src/TestReport.z42:34`

渲染一份 JSON 报告。形参名是 `artifact` 而非 `module`——`module` 是 z42 保留字。

### static string resultJson(TestResult r)

`src/libraries/z42.harness/src/TestReport.z42:66`


### static string esc(string str)

`src/libraries/z42.harness/src/TestReport.z42:98`


## TestRunner

`src/libraries/z42.harness/src/TestRunner.z42:15`

不依赖 attribute / 反射的命令式单元测试 runner：`Begin` / `Fail` / `Summary` 三段式。

每条测试一对 `Begin(name)` + `try { ... } catch (Exception e) { Fail(e); }`：

    var t = new TestRunner("MyTests");
    t.Begin("Addition");
    try { Assert.Equal(4, 2 + 2); } catch (Exception e) { t.Fail(e); }
    return t.Summary();   // 退出码 = failed 计数

`Begin` 隐式结算上一条（未被 `Fail` 标记即 PASS）；`Summary` 结算最后一条并打印汇总。
适合手写 main 的小产物；常规测试用 `[Test]` + [`Runner`]。

```z42
public class TestRunner {
    TestRunner(string contextName)
    void Begin(string name)
    void Fail(Exception e)
    int Summary()
}
```

### TestRunner(string contextName)

`src/libraries/z42.harness/src/TestRunner.z42:23`


### void Begin(string name)

`src/libraries/z42.harness/src/TestRunner.z42:36`

开始一条测试；有进行中的条目则先结算它。

### void Fail(Exception e)

`src/libraries/z42.harness/src/TestRunner.z42:50`

标记当前条目失败（在 catch 处调用），打印失败原因。

打印用 `ToString()` 而非 `Message`：`Assert` 抛的 `TestFailure` 里
`Message` 只是概括语（"values not equal"），actual / expected 要经
`ToString()` 才带得出来；裸 `Exception` 的 `ToString()` 同样不丢信息。

### int Summary()

`src/libraries/z42.harness/src/TestRunner.z42:57`

结算最后一条（若有）、打印汇总。返回 failed 计数，适合直接做进程退出码。

