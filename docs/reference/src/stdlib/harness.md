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

[`src/libraries/z42.harness/src/BenchStats.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/BenchStats.z42#L10)

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

成员：[`Label`](#Std.Harness.BenchStats.Label) · [`MinNs`](#Std.Harness.BenchStats.MinNs) · [`MedianNs`](#Std.Harness.BenchStats.MedianNs) · [`MaxNs`](#Std.Harness.BenchStats.MaxNs) · [`MeanNs`](#Std.Harness.BenchStats.MeanNs) · [`StdDevNs`](#Std.Harness.BenchStats.StdDevNs) · [`Samples`](#Std.Harness.BenchStats.Samples) · [`BenchStats`](#Std.Harness.BenchStats.ctor(string,long,long,long,long,long,long)) · [`parse`](#Std.Harness.BenchStats.parse(string))

<a id="Std.Harness.BenchStats.Label"></a>
### string Label

[`src/libraries/z42.harness/src/BenchStats.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/BenchStats.z42#L12)

基准名（`bench[<label>]` 方括号里的文本）。

<a id="Std.Harness.BenchStats.MinNs"></a>
### long MinNs

[`src/libraries/z42.harness/src/BenchStats.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/BenchStats.z42#L14)

最小耗时（纳秒）。

<a id="Std.Harness.BenchStats.MedianNs"></a>
### long MedianNs

[`src/libraries/z42.harness/src/BenchStats.z42:16`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/BenchStats.z42#L16)

中位数耗时（纳秒）。

<a id="Std.Harness.BenchStats.MaxNs"></a>
### long MaxNs

[`src/libraries/z42.harness/src/BenchStats.z42:18`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/BenchStats.z42#L18)

最大耗时（纳秒）。

<a id="Std.Harness.BenchStats.MeanNs"></a>
### long MeanNs

[`src/libraries/z42.harness/src/BenchStats.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/BenchStats.z42#L20)

平均耗时（纳秒）；摘要行缺 `mean=` 时为 -1。

<a id="Std.Harness.BenchStats.StdDevNs"></a>
### long StdDevNs

[`src/libraries/z42.harness/src/BenchStats.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/BenchStats.z42#L22)

标准差（纳秒）；摘要行缺 `stddev=` 时为 -1。

<a id="Std.Harness.BenchStats.Samples"></a>
### long Samples

[`src/libraries/z42.harness/src/BenchStats.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/BenchStats.z42#L24)

采样次数。

<a id="Std.Harness.BenchStats.ctor(string,long,long,long,long,long,long)"></a>
### BenchStats(string label, long minNs, long medianNs, long maxNs, long meanNs, long stddevNs, long samples)

[`src/libraries/z42.harness/src/BenchStats.z42:27`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/BenchStats.z42#L27)

按七个统计量直接装配。

<a id="Std.Harness.BenchStats.parse(string)"></a>
### static BenchStats? parse(string text)

[`src/libraries/z42.harness/src/BenchStats.z42:44`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/BenchStats.z42#L44)

从 `text` 里解析**最后**一条合法的 `bench[...]` 行。

`text` 是一个基准的捕获 stdout，可能多行。一条也解析不出来返回 null
（调用方按「这个基准没有统计」处理，而不是猜字段）。

| 参数 | 说明 |
|---|---|
| `text` | 基准的完整捕获输出；杂散输出在摘要行之前时取最后一条最稳 |

## Bencher

[`src/libraries/z42.harness/src/Bencher.z42:18`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/Bencher.z42#L18)

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

成员：[`WarmupIters`](#Std.Harness.Bencher.WarmupIters) · [`Samples`](#Std.Harness.Bencher.Samples) · [`MinNs`](#Std.Harness.Bencher.MinNs) · [`MaxNs`](#Std.Harness.Bencher.MaxNs) · [`MedianNs`](#Std.Harness.Bencher.MedianNs) · [`MeanNs`](#Std.Harness.Bencher.MeanNs) · [`StdDevNs`](#Std.Harness.Bencher.StdDevNs) · [`TotalNs`](#Std.Harness.Bencher.TotalNs) · [`Bencher`](#Std.Harness.Bencher.ctor()) · [`Bencher`](#Std.Harness.Bencher.ctor(int,int)) · [`iter`](#Std.Harness.Bencher.iter(Action)) · [`printSummary`](#Std.Harness.Bencher.printSummary(string))

<a id="Std.Harness.Bencher.WarmupIters"></a>
### int WarmupIters { get; private set; }

[`src/libraries/z42.harness/src/Bencher.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/Bencher.z42#L20)

预热次数（`iter` 开头不计时的先跑次数，构造时定死）。

<a id="Std.Harness.Bencher.Samples"></a>
### int Samples { get; private set; }

[`src/libraries/z42.harness/src/Bencher.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/Bencher.z42#L22)

实际采样数：自适应形态由 `iter` 选定后写入，固定形态即构造实参。

<a id="Std.Harness.Bencher.MinNs"></a>
### long MinNs { get; private set; }

[`src/libraries/z42.harness/src/Bencher.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/Bencher.z42#L24)

最快一次采样的耗时（ns）。

<a id="Std.Harness.Bencher.MaxNs"></a>
### long MaxNs { get; private set; }

[`src/libraries/z42.harness/src/Bencher.z42:26`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/Bencher.z42#L26)

最慢一次采样的耗时（ns）。

<a id="Std.Harness.Bencher.MedianNs"></a>
### long MedianNs { get; private set; }

[`src/libraries/z42.harness/src/Bencher.z42:28`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/Bencher.z42#L28)

采样耗时的中位数（ns；偶数个样本取上半中位）。

<a id="Std.Harness.Bencher.MeanNs"></a>
### long MeanNs { get; private set; }

[`src/libraries/z42.harness/src/Bencher.z42:30`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/Bencher.z42#L30)

采样耗时的算术平均（ns，整数除法向零截断）。

<a id="Std.Harness.Bencher.StdDevNs"></a>
### long StdDevNs { get; private set; }

[`src/libraries/z42.harness/src/Bencher.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/Bencher.z42#L32)

采样总体的标准差（ns，÷n 口径，四舍五入到整 ns）。

<a id="Std.Harness.Bencher.TotalNs"></a>
### long TotalNs { get; private set; }

[`src/libraries/z42.harness/src/Bencher.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/Bencher.z42#L34)

全部采样耗时之和（ns）。

<a id="Std.Harness.Bencher.ctor()"></a>
### Bencher()

[`src/libraries/z42.harness/src/Bencher.z42:42`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/Bencher.z42#L42)

自适应采样：预热 10 次，按约 50ms 的测量预算在 [20, 2000] 内选采样数。

先用一小段 pilot 估出单次耗时，再按预算定 n：快的基准采得多（置信区间紧），
慢的采得少（墙钟时间有上界）。`iter` 之后 `Samples` 是实际用的 n。

<a id="Std.Harness.Bencher.ctor(int,int)"></a>
### Bencher(int warmupIters, int sampleIters)

[`src/libraries/z42.harness/src/Bencher.z42:50`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/Bencher.z42#L50)

固定采样：预热 `warmupIters` 次、计时 `sampleIters` 次，不做自适应。

<a id="Std.Harness.Bencher.iter(Action)"></a>
### void iter(Action body)

[`src/libraries/z42.harness/src/Bencher.z42:68`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/Bencher.z42#L68)

预热后计时采样 n 次（n 固定或自适应），刷新全部统计属性。

返回后 `MinNs` / `MedianNs` / `MaxNs` / `MeanNs` / `StdDevNs` / `TotalNs` / `Samples`
反映这次采样。

| 参数 | 说明 |
|---|---|
| `body` | 被测代码；每次采样独立计时 |

<a id="Std.Harness.Bencher.printSummary(string)"></a>
### void printSummary(string label)

[`src/libraries/z42.harness/src/Bencher.z42:152`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/Bencher.z42#L152)

打一行人类可读的摘要；这行格式是 runner 契约的一部分。

json 模式下 runner 捕获这行并经 `BenchStats.parse` 解析成 `TestResult.bench_stats`：

    bench[<label>] min=<n>ns median=<n>ns max=<n>ns mean=<n>ns stddev=<n>ns samples=<n>

| 参数 | 说明 |
|---|---|
| `label` | 摘要行里的名字；`[Benchmark] void f(Bencher b)` 形态由 runner 按函数名传入 |

## BenchHelpers

[`src/libraries/z42.harness/src/Bencher.z42:179`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/Bencher.z42#L179)

基准相关的静态帮助函数集。

```z42
public static class BenchHelpers {
    static extern object blackBox(object value)
}
```

成员：[`blackBox`](#Std.Harness.BenchHelpers.blackBox(object))

<a id="Std.Harness.BenchHelpers.blackBox(object)"></a>
### static extern object blackBox(object value)

[`src/libraries/z42.harness/src/Bencher.z42:188`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/Bencher.z42#L188)

「别把这次计算优化掉」的恒等包装（对应 criterion 的 black_box）。

目前解释器不做死代码消除，恒等返回；留着这个形口，用户代码在 JIT 学会消除
纯表达式后无需改动。可无条件使用。

> ⚠️ 形参是 `object` 而非 `<T>`：跨 zpkg 的方法级泛型实参推断尚未就绪， 装箱开销由调用方自负。

## BundleCase

[`src/libraries/z42.harness/src/BundleRunner.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/BundleRunner.z42#L10)

测试束里的一条。

`Entry` 为 null 标记单元测试（`[Test]` 模块）；非 null（golden 程序的入口符号）
标记整程序 golden。`Zbc` / `Expected` 是相对束目录的文件名；`Requires` 是该条
声明的能力（源码 `// requires-caps:` 经 manifest 带来），空 = 无。

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

成员：[`Name`](#Std.Harness.BundleCase.Name) · [`Zbc`](#Std.Harness.BundleCase.Zbc) · [`Entry`](#Std.Harness.BundleCase.Entry) · [`Expected`](#Std.Harness.BundleCase.Expected) · [`Requires`](#Std.Harness.BundleCase.Requires) · [`BundleCase`](#Std.Harness.BundleCase.ctor(string,string,string,string)) · [`IsGolden`](#Std.Harness.BundleCase.IsGolden())

<a id="Std.Harness.BundleCase.Name"></a>
### string Name

[`src/libraries/z42.harness/src/BundleRunner.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/BundleRunner.z42#L12)

条目名；golden 与被能力门跳过的条目以它作 TestResult.Name。

<a id="Std.Harness.BundleCase.Zbc"></a>
### string Zbc

[`src/libraries/z42.harness/src/BundleRunner.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/BundleRunner.z42#L14)

产物文件名（`.zbc`），相对束目录。

<a id="Std.Harness.BundleCase.Entry"></a>
### string Entry

[`src/libraries/z42.harness/src/BundleRunner.z42:16`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/BundleRunner.z42#L16)

null → 单元测试；非 null → golden 的入口符号。

<a id="Std.Harness.BundleCase.Expected"></a>
### string Expected

[`src/libraries/z42.harness/src/BundleRunner.z42:18`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/BundleRunner.z42#L18)

golden 的期望输出文件名（可为 null / ""）。

<a id="Std.Harness.BundleCase.Requires"></a>
### string[] Requires

[`src/libraries/z42.harness/src/BundleRunner.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/BundleRunner.z42#L20)

该条声明的能力（空 = 无要求）；本 VM 缺任一项则整条跳过、不加载。

<a id="Std.Harness.BundleCase.ctor(string,string,string,string)"></a>
### BundleCase(string name, string zbc, string entry, string expected)

[`src/libraries/z42.harness/src/BundleRunner.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/BundleRunner.z42#L23)

按 name / zbc / entry / expected 建一条；`Requires` 初始化为空数组。

<a id="Std.Harness.BundleCase.IsGolden()"></a>
### bool IsGolden()

[`src/libraries/z42.harness/src/BundleRunner.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/BundleRunner.z42#L32)

是否整程序 golden 条目（`Entry` 非 null）。

## BundleRunner

[`src/libraries/z42.harness/src/BundleRunner.z42:43`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/BundleRunner.z42#L43)

测试束执行器：golden + 单元 + 汇总。

golden（整程序，隔离 VM 里跑、比对 stdout）+ 单元（共享 VM 的 `[Test]` 模块，
经 [`Runner.RunModuleResults`]）+ 汇总。设备端 test-agent 与宿主
`z42b test --rid host` 共用这套算法。

manifest 本身是 JSON，但本包零依赖：调用方用自己的 z42.json 把 manifest.json
解析成 `BundleCase[]` 再传进来。

```z42
public static class BundleRunner {
    static TestResult[] RunBundle(BundleCase[] allCases, string bundleDir, string libsDir)
    static int ExitCode(TestResult[] results)
}
```

成员：[`RunBundle`](#Std.Harness.BundleRunner.RunBundle(BundleCase[],string,string)) · [`ExitCode`](#Std.Harness.BundleRunner.ExitCode(TestResult[]))

<a id="Std.Harness.BundleRunner.RunBundle(BundleCase[],string,string)"></a>
### static TestResult[] RunBundle(BundleCase[] allCases, string bundleDir, string libsDir)

[`src/libraries/z42.harness/src/BundleRunner.z42:54`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/BundleRunner.z42#L54)

跑 `cases` 里的每一条并返回汇总结果（golden 在前、单元在后，缺能力被跳过的 条目排在末尾）。

能力门在加载之前：声明的能力本 VM 缺失的条目直接记 skipped、不加载（加载
本身就可能 trap——没有 native interop 的 VM 上 `[Native]` 门面在解析器里 panic）。

| 参数 | 说明 |
|---|---|
| `allCases` | 束内全部条目 |
| `bundleDir` | 存放 zbc / 期望输出文件的目录 |
| `libsDir` | 隔离 golden VM 的 stdlib 目录（"" → `Z42_LIBS`，否则 `<bundleDir>/../libs`） |

<a id="Std.Harness.BundleRunner.ExitCode(TestResult[])"></a>
### static int ExitCode(TestResult[] results)

[`src/libraries/z42.harness/src/BundleRunner.z42:150`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/BundleRunner.z42#L150)

束结果的退出码：任一条 failed → 1，否则 0。

## LintSupport

[`src/libraries/z42.harness/src/LintSupport.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/LintSupport.z42#L8)

plugin-internal-access 的判别力成员（类 public、成员 internal——internal 可见性的既有机制形态，测试单元同款）。

```z42
public static class LintSupport {
}
```

## InvokeOutcome

[`src/libraries/z42.harness/src/Runner.z42:35`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/Runner.z42#L35)

一次调用的结果（z42 无 out 参数，用字段带出）。

`Captured` 只在 json 模式下捕获被测函数的 stdout（供基准摘要行解析），
其余情况为 `""`。

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

成员：[`Threw`](#Std.Harness.InvokeOutcome.Threw) · [`Thrown`](#Std.Harness.InvokeOutcome.Thrown) · [`ErrType`](#Std.Harness.InvokeOutcome.ErrType) · [`ErrMsg`](#Std.Harness.InvokeOutcome.ErrMsg) · [`Captured`](#Std.Harness.InvokeOutcome.Captured) · [`InvokeOutcome`](#Std.Harness.InvokeOutcome.ctor())

<a id="Std.Harness.InvokeOutcome.Threw"></a>
### bool Threw

[`src/libraries/z42.harness/src/Runner.z42:37`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/Runner.z42#L37)

被调函数是否抛了异常。

<a id="Std.Harness.InvokeOutcome.Thrown"></a>
### Type? Thrown

[`src/libraries/z42.harness/src/Runner.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/Runner.z42#L39)

抛出的异常的运行期类型（`[ShouldThrow<E>]` 判定用）；未抛时为 null。

<a id="Std.Harness.InvokeOutcome.ErrType"></a>
### string ErrType

[`src/libraries/z42.harness/src/Runner.z42:41`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/Runner.z42#L41)

抛出异常的运行期类型全限定名（FullName）；未抛时为 `""`。

<a id="Std.Harness.InvokeOutcome.ErrMsg"></a>
### string ErrMsg

[`src/libraries/z42.harness/src/Runner.z42:43`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/Runner.z42#L43)

抛出异常的 Message；未抛时为 `""`。

<a id="Std.Harness.InvokeOutcome.Captured"></a>
### string Captured

[`src/libraries/z42.harness/src/Runner.z42:45`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/Runner.z42#L45)

json 模式下捕获的被测函数 stdout（供基准摘要行解析）；其余模式为 `""`。

<a id="Std.Harness.InvokeOutcome.ctor()"></a>
### InvokeOutcome()

[`src/libraries/z42.harness/src/Runner.z42:47`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/Runner.z42#L47)

全字段初始化：Threw=false、Thrown=null、字符串字段空串。

## Runner

[`src/libraries/z42.harness/src/Runner.z42:56`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/Runner.z42#L56)

反射式测试 runner：发现并执行测试 / bench，归类 pass / fail / skip。

并入一个编好的测试 / bench 产物，经 [`TestDiscovery`] 找出 `[Test]` / `[Benchmark]` /
`[Setup]` / `[Teardown]`，逐条执行并归类 pass / fail / skip。

```z42
public static class Runner {
    static int RunModule(string path, string format)
    static int ExitCodeFor(int runnable, int failed)
    static TestResult[] RunModuleResults(string path)
}
```

成员：[`RunModule`](#Std.Harness.Runner.RunModule(string,string)) · [`ExitCodeFor`](#Std.Harness.Runner.ExitCodeFor(int,int)) · [`RunModuleResults`](#Std.Harness.Runner.RunModuleResults(string))

<a id="Std.Harness.Runner.RunModule(string,string)"></a>
### static int RunModule(string path, string format)

[`src/libraries/z42.harness/src/Runner.z42:61`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/Runner.z42#L61)

并入 `path` 处的产物并跑其中每条 `[Test]` / `[Benchmark]`，按 `format` 输出。

`format` 取 `"pretty"` | `"json"`。返回进程退出码：0 = 全过；1 = 有失败，或一条
可跑的测试都没发现（拼错 attribute、glob 漏文件都算事故，不与「全过」等价）。

<a id="Std.Harness.Runner.ExitCodeFor(int,int)"></a>
### static int ExitCodeFor(int runnable, int failed)

[`src/libraries/z42.harness/src/Runner.z42:115`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/Runner.z42#L115)

退出码决策（纯函数，可直接单测）：一条可跑的测试都没有 → 1（「测试没跑」 不得与「全过」等价）；有失败 → 1；否则 0。

<a id="Std.Harness.Runner.RunModuleResults(string)"></a>
### static TestResult[] RunModuleResults(string path)

[`src/libraries/z42.harness/src/Runner.z42:124`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/Runner.z42#L124)

跑 `path` 处产物里的每条 `[Test]` / `[Benchmark]`，**只返回结果、不打印**。

供跨模块聚合：调用方收集多个模块的结果后统一渲染一份合并报告。

## TestAttribute

[`src/libraries/z42.harness/src/TestAttributes.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestAttributes.z42#L15)

一条测试：零参、返回 void 的自由函数或静态方法。

```z42
public sealed class TestAttribute : Attribute {
    TestAttribute()
}
```

成员：[`TestAttribute`](#Std.Harness.TestAttribute.ctor())

<a id="Std.Harness.TestAttribute.ctor()"></a>
### TestAttribute()

[`src/libraries/z42.harness/src/TestAttributes.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestAttributes.z42#L17)

无实参（签名约束由编译期校验）。

## BenchmarkAttribute

[`src/libraries/z42.harness/src/TestAttributes.z42:21`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestAttributes.z42#L21)

一条基准：零参，或唯一参数为 [`Bencher`]（`void f(Bencher b)`——runner 传入并自动打摘要行）。

```z42
public sealed class BenchmarkAttribute : Attribute {
    BenchmarkAttribute()
}
```

成员：[`BenchmarkAttribute`](#Std.Harness.BenchmarkAttribute.ctor())

<a id="Std.Harness.BenchmarkAttribute.ctor()"></a>
### BenchmarkAttribute()

[`src/libraries/z42.harness/src/TestAttributes.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestAttributes.z42#L23)

无实参（签名约束由编译期校验）。

## SetupAttribute

[`src/libraries/z42.harness/src/TestAttributes.z42:27`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestAttributes.z42#L27)

同命名空间（或同一类型）里每条测试之前执行。

```z42
public sealed class SetupAttribute : Attribute {
    SetupAttribute()
}
```

成员：[`SetupAttribute`](#Std.Harness.SetupAttribute.ctor())

<a id="Std.Harness.SetupAttribute.ctor()"></a>
### SetupAttribute()

[`src/libraries/z42.harness/src/TestAttributes.z42:29`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestAttributes.z42#L29)

无实参（签名约束由编译期校验）。

## TeardownAttribute

[`src/libraries/z42.harness/src/TestAttributes.z42:33`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestAttributes.z42#L33)

同命名空间（或同一类型）里每条测试之后执行（测试失败也执行）。

```z42
public sealed class TeardownAttribute : Attribute {
    TeardownAttribute()
}
```

成员：[`TeardownAttribute`](#Std.Harness.TeardownAttribute.ctor())

<a id="Std.Harness.TeardownAttribute.ctor()"></a>
### TeardownAttribute()

[`src/libraries/z42.harness/src/TestAttributes.z42:35`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestAttributes.z42#L35)

无实参（签名约束由编译期校验）。

## SkipAttribute

[`src/libraries/z42.harness/src/TestAttributes.z42:42`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestAttributes.z42#L42)

跳过本条。`reason` 必填（E0914）；`platform` / `feature` 是可选门。

给 `platform` ⇒ 仅在该平台跳过；给 `feature` ⇒ 运行时缺这项能力才跳过
（能力集见 `Std.Platform.Capabilities()`，未知能力名按缺失处理）。

```z42
public sealed class SkipAttribute : Attribute {
    string Reason
    string Platform
    string Feature
    SkipAttribute(string reason = "", string platform = "", string feature = "")
}
```

成员：[`Reason`](#Std.Harness.SkipAttribute.Reason) · [`Platform`](#Std.Harness.SkipAttribute.Platform) · [`Feature`](#Std.Harness.SkipAttribute.Feature) · [`SkipAttribute`](#Std.Harness.SkipAttribute.ctor(string,string,string))

<a id="Std.Harness.SkipAttribute.Reason"></a>
### string Reason

[`src/libraries/z42.harness/src/TestAttributes.z42:44`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestAttributes.z42#L44)

必填：跳过的原因（报告里随 SKIP 行输出）。

<a id="Std.Harness.SkipAttribute.Platform"></a>
### string Platform

[`src/libraries/z42.harness/src/TestAttributes.z42:46`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestAttributes.z42#L46)

平台门：等于 `Std.Platform.OS()` 的返回值时跳过。

<a id="Std.Harness.SkipAttribute.Feature"></a>
### string Feature

[`src/libraries/z42.harness/src/TestAttributes.z42:48`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestAttributes.z42#L48)

能力门：运行中的 VM 不报告这项能力时跳过。

<a id="Std.Harness.SkipAttribute.ctor(string,string,string)"></a>
### SkipAttribute(string reason = "", string platform = "", string feature = "")

[`src/libraries/z42.harness/src/TestAttributes.z42:50`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestAttributes.z42#L50)

三门都可省（缺省空串）；空串在发现层归一成 null（见 [`TestDiscovery`]）。

## IgnoreAttribute

[`src/libraries/z42.harness/src/TestAttributes.z42:58`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestAttributes.z42#L58)

无条件跳过（不报理由）。

```z42
public sealed class IgnoreAttribute : Attribute {
    IgnoreAttribute()
}
```

成员：[`IgnoreAttribute`](#Std.Harness.IgnoreAttribute.ctor())

<a id="Std.Harness.IgnoreAttribute.ctor()"></a>
### IgnoreAttribute()

[`src/libraries/z42.harness/src/TestAttributes.z42:60`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestAttributes.z42#L60)

无实参。

## TimeoutAttribute

[`src/libraries/z42.harness/src/TestAttributes.z42:64`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestAttributes.z42#L64)

超时（毫秒，必须为正：E0917）。**目前只有编译期校验，runner 运行期不强制。**

```z42
public sealed class TimeoutAttribute : Attribute {
    int Milliseconds
    TimeoutAttribute(int milliseconds = 0)
}
```

成员：[`Milliseconds`](#Std.Harness.TimeoutAttribute.Milliseconds) · [`TimeoutAttribute`](#Std.Harness.TimeoutAttribute.ctor(int))

<a id="Std.Harness.TimeoutAttribute.Milliseconds"></a>
### int Milliseconds

[`src/libraries/z42.harness/src/TestAttributes.z42:66`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestAttributes.z42#L66)

超时毫秒数（必须为正）。

<a id="Std.Harness.TimeoutAttribute.ctor(int)"></a>
### TimeoutAttribute(int milliseconds = 0)

[`src/libraries/z42.harness/src/TestAttributes.z42:68`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestAttributes.z42#L68)

记录 `milliseconds`（正性由编译期 E0917 查，运行期不强制）。

## ShouldThrowAttribute

[`src/libraries/z42.harness/src/TestAttributes.z42:72`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestAttributes.z42#L72)

期望抛出 `E`（或其子类型）。runner 经 [`IExpectsException`] 取到 `E`，不必知道类型实参。

```z42
public sealed class ShouldThrowAttribute<E> : Attribute, IExpectsException {
    ShouldThrowAttribute()
    Type? ExceptionType()
}
```

成员：[`ShouldThrowAttribute`](#Std.Harness.ShouldThrowAttribute%3CE%3E.ctor()) · [`ExceptionType`](#Std.Harness.ShouldThrowAttribute%3CE%3E.ExceptionType())

<a id="Std.Harness.ShouldThrowAttribute<E>.ctor()"></a>
### ShouldThrowAttribute()

[`src/libraries/z42.harness/src/TestAttributes.z42:74`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestAttributes.z42#L74)

无实参。

<a id="Std.Harness.ShouldThrowAttribute<E>.ExceptionType()"></a>
### Type? ExceptionType()

[`src/libraries/z42.harness/src/TestAttributes.z42:76`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestAttributes.z42#L76)

期望的异常类型：typeof(E)。

## ShouldThrowAttribute

[`src/libraries/z42.harness/src/TestAttributes.z42:80`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestAttributes.z42#L80)

漏写类型实参的 `[ShouldThrow]` 落到这里（E0913 照报）；它不期望任何具体类型。

```z42
public sealed class ShouldThrowAttribute : Attribute, IExpectsException {
    ShouldThrowAttribute()
    Type? ExceptionType()
}
```

成员：[`ShouldThrowAttribute`](#Std.Harness.ShouldThrowAttribute.ctor()) · [`ExceptionType`](#Std.Harness.ShouldThrowAttribute.ExceptionType())

<a id="Std.Harness.ShouldThrowAttribute.ctor()"></a>
### ShouldThrowAttribute()

[`src/libraries/z42.harness/src/TestAttributes.z42:82`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestAttributes.z42#L82)

无实参。

<a id="Std.Harness.ShouldThrowAttribute.ExceptionType()"></a>
### Type? ExceptionType()

[`src/libraries/z42.harness/src/TestAttributes.z42:84`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestAttributes.z42#L84)

恒返 null——不期望任何具体异常类型。

## IExpectsException

[`src/libraries/z42.harness/src/TestAttributes.z42:88`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestAttributes.z42#L88)

「期望抛出某类型」的 attribute 统一口径（runner 只认这个接口）。

```z42
public interface IExpectsException {
    Type? ExceptionType()
}
```

成员：[`ExceptionType`](#Std.Harness.IExpectsException.ExceptionType())

<a id="Std.Harness.IExpectsException.ExceptionType()"></a>
### Type? ExceptionType()

[`src/libraries/z42.harness/src/TestAttributes.z42:90`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestAttributes.z42#L90)

期望抛出的异常类型；null 表示没有期望。

## TestCase

[`src/libraries/z42.harness/src/TestDiscovery.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestDiscovery.z42#L6)

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

成员：[`Qualified`](#Std.Harness.TestCase.Qualified) · [`Kind`](#Std.Harness.TestCase.Kind) · [`Method`](#Std.Harness.TestCase.Method) · [`Skipped`](#Std.Harness.TestCase.Skipped) · [`Ignored`](#Std.Harness.TestCase.Ignored) · [`SkipReason`](#Std.Harness.TestCase.SkipReason) · [`SkipPlatform`](#Std.Harness.TestCase.SkipPlatform) · [`SkipFeature`](#Std.Harness.TestCase.SkipFeature) · [`ShouldThrow`](#Std.Harness.TestCase.ShouldThrow) · [`Expected`](#Std.Harness.TestCase.Expected) · [`TestCase`](#Std.Harness.TestCase.ctor(string,int,MethodInfo)) · [`IsTest`](#Std.Harness.TestCase.IsTest()) · [`IsBenchmark`](#Std.Harness.TestCase.IsBenchmark()) · [`IsSetup`](#Std.Harness.TestCase.IsSetup()) · [`IsTeardown`](#Std.Harness.TestCase.IsTeardown()) · [`IsSkipped`](#Std.Harness.TestCase.IsSkipped()) · [`IsIgnored`](#Std.Harness.TestCase.IsIgnored()) · [`HasShouldThrow`](#Std.Harness.TestCase.HasShouldThrow()) · [`DeclaringType`](#Std.Harness.TestCase.DeclaringType()) · [`Invoke`](#Std.Harness.TestCase.Invoke()) · [`TakesBencher`](#Std.Harness.TestCase.TakesBencher())

<a id="Std.Harness.TestCase.Qualified"></a>
### string Qualified

[`src/libraries/z42.harness/src/TestDiscovery.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestDiscovery.z42#L8)

报告用的名字：`<命名空间或类型>.<函数名>`。

<a id="Std.Harness.TestCase.Kind"></a>
### int Kind

[`src/libraries/z42.harness/src/TestDiscovery.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestDiscovery.z42#L10)

1=Test, 2=Benchmark, 3=Setup, 4=Teardown.

<a id="Std.Harness.TestCase.Method"></a>
### MethodInfo Method

[`src/libraries/z42.harness/src/TestDiscovery.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestDiscovery.z42#L12)

被标注的函数本体，[`Invoke`] 的反射调用目标。

<a id="Std.Harness.TestCase.Skipped"></a>
### bool Skipped

[`src/libraries/z42.harness/src/TestDiscovery.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestDiscovery.z42#L14)

贴了 `[Skip]` 为 true（三个门控实参见下面的 `Skip*` 字段）。

<a id="Std.Harness.TestCase.Ignored"></a>
### bool Ignored

[`src/libraries/z42.harness/src/TestDiscovery.z42:16`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestDiscovery.z42#L16)

贴了 `[Ignore]` 为 true（无条件跳过，不报理由）。

<a id="Std.Harness.TestCase.SkipReason"></a>
### string? SkipReason

[`src/libraries/z42.harness/src/TestDiscovery.z42:18`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestDiscovery.z42#L18)

`[Skip]` 的三个实参；没给的为 null。

<a id="Std.Harness.TestCase.SkipPlatform"></a>
### string? SkipPlatform

[`src/libraries/z42.harness/src/TestDiscovery.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestDiscovery.z42#L20)

`[Skip]` 的 platform 实参；没给为 null。

<a id="Std.Harness.TestCase.SkipFeature"></a>
### string? SkipFeature

[`src/libraries/z42.harness/src/TestDiscovery.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestDiscovery.z42#L22)

`[Skip]` 的 feature 实参；没给为 null。

<a id="Std.Harness.TestCase.ShouldThrow"></a>
### bool ShouldThrow

[`src/libraries/z42.harness/src/TestDiscovery.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestDiscovery.z42#L24)

`[ShouldThrow<E>]`：贴了即 true；`Expected` 为 E（漏写类型实参时为 null）。

<a id="Std.Harness.TestCase.Expected"></a>
### Type? Expected

[`src/libraries/z42.harness/src/TestDiscovery.z42:26`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestDiscovery.z42#L26)

期望抛出的异常类型（`[ShouldThrow<E>]` 的 E）；漏写类型实参为 null。

<a id="Std.Harness.TestCase.ctor(string,int,MethodInfo)"></a>
### TestCase(string qualified, int kind, MethodInfo method)

[`src/libraries/z42.harness/src/TestDiscovery.z42:30`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestDiscovery.z42#L30)

装配一条用例：名字 / kind / 函数取实参，skip 与期望异常等一律先置默认， 由发现层（`_fromMethod`）读 attribute 后补写。

<a id="Std.Harness.TestCase.IsTest()"></a>
### bool IsTest()

[`src/libraries/z42.harness/src/TestDiscovery.z42:38`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestDiscovery.z42#L38)

是否 `[Test]`（Kind == 1）。

<a id="Std.Harness.TestCase.IsBenchmark()"></a>
### bool IsBenchmark()

[`src/libraries/z42.harness/src/TestDiscovery.z42:40`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestDiscovery.z42#L40)

是否 `[Benchmark]`（Kind == 2）。

<a id="Std.Harness.TestCase.IsSetup()"></a>
### bool IsSetup()

[`src/libraries/z42.harness/src/TestDiscovery.z42:42`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestDiscovery.z42#L42)

是否 `[Setup]`（Kind == 3）。

<a id="Std.Harness.TestCase.IsTeardown()"></a>
### bool IsTeardown()

[`src/libraries/z42.harness/src/TestDiscovery.z42:44`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestDiscovery.z42#L44)

是否 `[Teardown]`（Kind == 4）。

<a id="Std.Harness.TestCase.IsSkipped()"></a>
### bool IsSkipped()

[`src/libraries/z42.harness/src/TestDiscovery.z42:46`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestDiscovery.z42#L46)

是否贴了 `[Skip]`。

<a id="Std.Harness.TestCase.IsIgnored()"></a>
### bool IsIgnored()

[`src/libraries/z42.harness/src/TestDiscovery.z42:48`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestDiscovery.z42#L48)

是否贴了 `[Ignore]`。

<a id="Std.Harness.TestCase.HasShouldThrow()"></a>
### bool HasShouldThrow()

[`src/libraries/z42.harness/src/TestDiscovery.z42:50`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestDiscovery.z42#L50)

是否贴了 `[ShouldThrow<E>]`。

<a id="Std.Harness.TestCase.DeclaringType()"></a>
### string DeclaringType()

[`src/libraries/z42.harness/src/TestDiscovery.z42:53`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestDiscovery.z42#L53)

所属命名空间（自由函数）或类型（静态方法）的全限定名——`[Setup]` / `[Teardown]` 按它成组。

<a id="Std.Harness.TestCase.Invoke()"></a>
### void Invoke()

[`src/libraries/z42.harness/src/TestDiscovery.z42:64`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestDiscovery.z42#L64)

调用（无接收者）。被调函数里抛出的异常按原类型传出。

零参形态直接调（`[Test]` / `[Setup]` / `[Teardown]` 及自建 [`Bencher`] 的
`[Benchmark]`）；`[Benchmark] void f(Bencher b)` 形态由这里建 [`Bencher`] 传入，
跑完按函数名打摘要行（与自建形态手写 `b.printSummary("f")` 输出同一行）。

<a id="Std.Harness.TestCase.TakesBencher()"></a>
### bool TakesBencher()

[`src/libraries/z42.harness/src/TestDiscovery.z42:77`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestDiscovery.z42#L77)

恰好一个 `Std.Harness.Bencher` 形参（编译期 E0912 已保证 `[Benchmark]` 只有零参与这一种形状）。

## TestDiscovery

[`src/libraries/z42.harness/src/TestDiscovery.z42:95`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestDiscovery.z42#L95)

运行期反射发现测试：并入编好的产物，把贴了测试 attribute 的函数读成 [`TestCase`]。

把编好的测试 / bench 产物并入当前 VM，经反射找出贴了 `[Test]` / `[Benchmark]` /
`[Setup]` / `[Teardown]` 的函数，连同 `[Skip]` / `[Ignore]` / `[ShouldThrow<E>]`
一起读成 [`TestCase`]。

发现面 = 产物的自由函数（`Assembly.GetFunctions()`）+ 各类型的静态方法（`Type.GetMethods()`）；
只读 attribute 活实例，不依赖编译器写进产物的测试专用元数据，也不依赖测试专用的 VM 能力。
结果按全限定名排序：同输入同输出，便于比对报告。

```z42
public static class TestDiscovery {
    static TestCase[] Discover(string path)
}
```

成员：[`Discover`](#Std.Harness.TestDiscovery.Discover(string))

<a id="Std.Harness.TestDiscovery.Discover(string)"></a>
### static TestCase[] Discover(string path)

[`src/libraries/z42.harness/src/TestDiscovery.z42:97`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestDiscovery.z42#L97)

并入 `path`（.zpkg / .zbc）并发现其中的测试。加载失败抛异常。

## CaptureResult

[`src/libraries/z42.harness/src/TestIO.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestIO.z42#L4)

一次双通道捕获的结果。

```z42
public class CaptureResult {
    string Stdout { get; private set; }
    string Stderr { get; private set; }
    CaptureResult(string stdout, string stderr)
}
```

成员：[`Stdout`](#Std.Harness.CaptureResult.Stdout) · [`Stderr`](#Std.Harness.CaptureResult.Stderr) · [`CaptureResult`](#Std.Harness.CaptureResult.ctor(string,string))

<a id="Std.Harness.CaptureResult.Stdout"></a>
### string Stdout { get; private set; }

[`src/libraries/z42.harness/src/TestIO.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestIO.z42#L6)

捕获窗口内写到的 stdout 文本。

<a id="Std.Harness.CaptureResult.Stderr"></a>
### string Stderr { get; private set; }

[`src/libraries/z42.harness/src/TestIO.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestIO.z42#L9)

捕获窗口内写到的 stderr 文本。

<a id="Std.Harness.CaptureResult.ctor(string,string)"></a>
### CaptureResult(string stdout, string stderr)

[`src/libraries/z42.harness/src/TestIO.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestIO.z42#L12)

以两路捕获文本组装结果；参数顺序即字段顺序。

## TestIO

[`src/libraries/z42.harness/src/TestIO.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestIO.z42#L25)

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

成员：[`captureStdout`](#Std.Harness.TestIO.captureStdout(Action)) · [`captureStderr`](#Std.Harness.TestIO.captureStderr(Action)) · [`captureBoth`](#Std.Harness.TestIO.captureBoth(Action))

<a id="Std.Harness.TestIO.captureStdout(Action)"></a>
### static string captureStdout(Action body)

[`src/libraries/z42.harness/src/TestIO.z42:27`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestIO.z42#L27)

捕获 `body` 执行期间写到的 stdout，返回捕获文本。

<a id="Std.Harness.TestIO.captureStderr(Action)"></a>
### static string captureStderr(Action body)

[`src/libraries/z42.harness/src/TestIO.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestIO.z42#L39)

只捕获 stderr；stdout 照常穿透到进程输出。

<a id="Std.Harness.TestIO.captureBoth(Action)"></a>
### static CaptureResult captureBoth(Action body)

[`src/libraries/z42.harness/src/TestIO.z42:51`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestIO.z42#L51)

双通道各自独立捕获。

## TestResult

[`src/libraries/z42.harness/src/TestReport.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestReport.z42#L6)

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

成员：[`Name`](#Std.Harness.TestResult.Name) · [`Status`](#Std.Harness.TestResult.Status) · [`IsBenchmark`](#Std.Harness.TestResult.IsBenchmark) · [`Reason`](#Std.Harness.TestResult.Reason) · [`Stats`](#Std.Harness.TestResult.Stats) · [`TestResult`](#Std.Harness.TestResult.ctor(string,string,bool,string,BenchStats))

<a id="Std.Harness.TestResult.Name"></a>
### string Name

[`src/libraries/z42.harness/src/TestReport.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestReport.z42#L8)

全限定名：`<命名空间或类型>.<函数名>`。

<a id="Std.Harness.TestResult.Status"></a>
### string Status

[`src/libraries/z42.harness/src/TestReport.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestReport.z42#L10)

`"passed"` | `"failed"` | `"skipped"`。

<a id="Std.Harness.TestResult.IsBenchmark"></a>
### bool IsBenchmark

[`src/libraries/z42.harness/src/TestReport.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestReport.z42#L12)

该条是否为 `[Benchmark]`（json 报告的 `is_benchmark` 字段）。

<a id="Std.Harness.TestResult.Reason"></a>
### string Reason

[`src/libraries/z42.harness/src/TestReport.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestReport.z42#L14)

仅失败 / 跳过时非 null。

<a id="Std.Harness.TestResult.Stats"></a>
### BenchStats Stats

[`src/libraries/z42.harness/src/TestReport.z42:16`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestReport.z42#L16)

仅基准且摘要行解析成功时非 null。

<a id="Std.Harness.TestResult.ctor(string,string,bool,string,BenchStats)"></a>
### TestResult(string name, string status, bool isBenchmark, string reason, BenchStats stats)

[`src/libraries/z42.harness/src/TestReport.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestReport.z42#L19)

全字段直赋：`reason` 仅失败 / 跳过时非 null，`stats` 仅基准解析成功时非 null。

## TestReport

[`src/libraries/z42.harness/src/TestReport.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestReport.z42#L34)

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

成员：[`toJson`](#Std.Harness.TestReport.toJson(string,TestResult[])) · [`resultJson`](#Std.Harness.TestReport.resultJson(TestResult)) · [`esc`](#Std.Harness.TestReport.esc(string))

<a id="Std.Harness.TestReport.toJson(string,TestResult[])"></a>
### static string toJson(string artifact, TestResult[] results)

[`src/libraries/z42.harness/src/TestReport.z42:36`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestReport.z42#L36)

渲染一份 JSON 报告。形参名是 `artifact` 而非 `module`——`module` 是 z42 保留字。

<a id="Std.Harness.TestReport.resultJson(TestResult)"></a>
### static string resultJson(TestResult r)

[`src/libraries/z42.harness/src/TestReport.z42:68`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestReport.z42#L68)

渲染单条结果的 JSON 对象（不带尾逗号；由 toJson 以 "," 接合）。

<a id="Std.Harness.TestReport.esc(string)"></a>
### static string esc(string str)

[`src/libraries/z42.harness/src/TestReport.z42:100`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestReport.z42#L100)

最小 JSON 字符串转义：引号、反斜杠及异常消息里常见的控制字符。

对本报告携带的字符串（名字、状态、原因文本）已足够。

## TestRunner

[`src/libraries/z42.harness/src/TestRunner.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestRunner.z42#L15)

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

成员：[`TestRunner`](#Std.Harness.TestRunner.ctor(string)) · [`Begin`](#Std.Harness.TestRunner.Begin(string)) · [`Fail`](#Std.Harness.TestRunner.Fail(Exception)) · [`Summary`](#Std.Harness.TestRunner.Summary())

<a id="Std.Harness.TestRunner.ctor(string)"></a>
### TestRunner(string contextName)

[`src/libraries/z42.harness/src/TestRunner.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestRunner.z42#L24)

记上下文名并打印测试头横幅；计数清零，尚无进行中的条目。

<a id="Std.Harness.TestRunner.Begin(string)"></a>
### void Begin(string name)

[`src/libraries/z42.harness/src/TestRunner.z42:37`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestRunner.z42#L37)

开始一条测试；有进行中的条目则先结算它。

<a id="Std.Harness.TestRunner.Fail(Exception)"></a>
### void Fail(Exception e)

[`src/libraries/z42.harness/src/TestRunner.z42:51`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestRunner.z42#L51)

标记当前条目失败（在 catch 处调用），打印失败原因。

打印用 `ToString()` 而非 `Message`：`Assert` 抛的 `TestFailure` 里
`Message` 只是概括语（"values not equal"），actual / expected 要经
`ToString()` 才带得出来；裸 `Exception` 的 `ToString()` 同样不丢信息。

<a id="Std.Harness.TestRunner.Summary()"></a>
### int Summary()

[`src/libraries/z42.harness/src/TestRunner.z42:58`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.harness/src/TestRunner.z42#L58)

结算最后一条（若有）、打印汇总。返回 failed 计数，适合直接做进程退出码。

