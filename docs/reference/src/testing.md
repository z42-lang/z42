# 测试

> 对齐：2026-09-17（change `restructure-docs-three-books`）｜ 代码：`src/libraries/z42.test/src/`、
> `src/libraries/z42.core/src/Assert.z42`、`src/compiler/z42c.semantics/src/DeclEnforcer.z42`、
> `src/toolchain/builder/core/builder_test.z42`
>
> 测试文件放在哪、`[tests]` / `[[test]]` / `[benches]` 怎么配、`harness = false` 是什么，
> 见[工程清单 z42.toml](toolchain/z42-toml.md) 的「测试 / Bench / Example 目标配置」一节。

z42 的测试单元是**贴了 `[Test]` 的零参 `void` 自由函数**；`z42 test` 编译工程后把它们逐个跑掉，
按退出码和一份报告给出结果。本页是这套东西的契约面：attribute 全表与判定规则、`Std.Assert` 全表、
`Bencher` / `TestIO` 的公开成员、`z42 test` / `z42 bench` 的旗标与退出码，
以及一组**写得出来但当前不生效**的东西——最后一节必读。

## 最小形态

```
demo/
├── z42.toml
├── src/
│   └── Math.z42
└── tests/
    └── math_tests.z42      ← 一个文件 = 一个测试目标，目标名 = 文件 stem
```

```z42
// tests/math_tests.z42
namespace Demo.Tests;

using Demo;                  // 被测代码的命名空间

[Test]
void test_add_ok() {
    Assert.Equal(5, MathX.Add(2, 3));
}

[Test]
void test_add_wrong() {
    Assert.Equal(6, MathX.Add(2, 3));
}

[Test]
[Skip(reason: "not implemented yet")]
void test_not_ready() {
    Assert.Fail("should never run");
}
```

`z42 test` 即可。**不需要**在清单里写 `[tests.dependencies] "z42.test"`——stdlib 自动可用。
**也不需要** `using Std;`：`Assert` / `TestFailure` / `SkipSignal` 都在 prelude 包 `z42.core` 里，
裸写即可；`Bencher` / `TestIO` 在 `z42.test`，要 `using Std.Test;`。

> 工程里**没有** `tests/` 目录时，`z42 test` 退回编译工程自身、跑其中的 `[Test]`——
> 「整个包就是一组测试」的独立测试工程是这个形态。

## 测试 attribute

### 类别

一个声明上贴一个。多贴不报错，但**最后一个生效**（`[Test] [Benchmark] void f()` 被当作
benchmark），别这么写。

| attribute | 作用 |
|---|---|
| `[Test]` | 一个测试单元 |
| `[Benchmark]` | 一个基准单元，写法与结果判定同 `[Test]`，另见[下文](#benchmark-与-bencher) |
| `[Setup]` | 每个同命名空间 `[Test]`/`[Benchmark]` 运行**前**调用一次 |
| `[Teardown]` | 每个同命名空间 `[Test]`/`[Benchmark]` 运行**后**调用一次（测试抛异常也照跑） |

`[Setup]` / `[Teardown]` 按**命名空间**配对，不按文件、也不按类——同一个 namespace 下写几个就都跑几个。

### 五条签名规则（编译期强制）

四个类别 attribute 共用同一组要求，违反即报错、不进产物：

1. 必须是**自由函数**或 `static` 方法（实例方法 / 构造器不行）
2. 返回类型必须是 `void`
3. **零参数**（唯一例外：`[Benchmark] void f(Bencher b)`，见下）
4. 不能是泛型方法
5. 必须有方法体

诊断码：`[Test]` → `E0911`，`[Benchmark]` → `E0912`，`[Setup]`/`[Teardown]` → `E0915`。
完整条目见[错误码全量表](appendix/error-codes.md)。

```
./tests/bad_tests.z42(4,6): E0911: `[Test]` must be applied to a free function or a `static` method
    (got: instance method `Holder.test_instance`); add `static`, or move it to a top-level function
./tests/bad_tests.z42(8,2): E0911: `[Test]` must return `void` (got: `int`)
./tests/bad_tests.z42(11,2): E0911: `[Test]` must take no parameters (got: 1)
```

贴在 `static class` 的 `static` 方法上是合法的，但报告里的名字会带
`$<参数个数>` 后缀（`Demo.S.MathTests.test_in_static_class$0`）；自由函数没有这个后缀。

### 修饰 attribute

修饰 attribute **必须与 `[Test]` 或 `[Benchmark]` 贴在同一个声明上**，否则报 `E0914`。

| attribute | 形态 | 语义 |
|---|---|---|
| `[Skip(reason: "…")]` | `reason` **必填且非空**（缺 → `E0914`） | 无条件跳过 |
| `[Skip(reason: "…", platform: "…")]` | 见下 | 宿主 OS **等于** `platform` 时跳过；否则照跑 |
| `[Skip(reason: "…", feature: "…")]` | 见下 | 宿主 VM **缺少**该能力时跳过；有则照跑 |
| `[Ignore]` | 无参 | 无条件跳过，报告里理由固定为 `ignored` |
| `[ShouldThrow<E>]` | 类型实参必填 | 单元抛出 `E` **或 `E` 的任意子类**才算通过；不抛或类型不符判失败 |

`[ShouldThrow<E>]` 的 `E` 必须（传递地）派生自 `Exception`，否则 `E0913`。

### `[Skip(platform:)]` 的取值

比对的是 [`Std.Platform.OS()`](stdlib/platform.md) 的返回值（字符串**全等**，大小写敏感）：

| 值 | 宿主 |
|---|---|
| `"linux"` | Linux |
| `"macos"` | macOS |
| `"windows"` | Windows |
| `"android"` | Android |
| `"ios"` | iOS |
| `"wasm"` | WebAssembly |
| `"freebsd"` | FreeBSD |

写一个不存在的平台名（`"atari"`）不报错，只是**永远不匹配**⇒ 该测试在所有宿主上都照跑。

### `[Skip(feature:)]` 的取值

比对的是 [`Std.Platform.Capabilities()`](stdlib/platform.md) 报告的能力集。**全部合法值只有五个**：

| 能力名 | 含义 |
|---|---|
| `"jit"` | 该 VM 二进制编入了 JIT 后端 |
| `"native-interop"` | 编入了 native interop |
| `"bundled-compression"` | 编入了内置压缩 |
| `"threads"` | 有真实 OS 线程（wasm 之外都有） |
| `"socket"` | 有真实 OS 网络（TCP / UDP / HTTP / WS；wasm 之外都有） |

> 🔴 **deny-by-default：能力名拼错不报错，判为「缺失」⇒ 测试被静默跳过。**
> `[Skip(feature: "thread")]`（少个 s）、`[Skip(feature: "multithreading")]`、
> `[Skip(feature: "filesystem")]`——这些都不是合法能力名，贴上去的测试**在任何宿主上都永远不跑**，
> 而报告里只是一行普通的 `SKIP`，没有任何警告。加 `feature:` 之后务必确认该用例仍出现在
> 「运行」一侧，别只看 `Result:` 那行的总数。

### `platform:` 与 `feature:` 同时写

**`platform:` 单独决定结果，`feature:` 被完全忽略**——不是「或」，也不是「与」：

```z42
// 宿主 = macos，"nonexistent" 必然不在能力集里
[Test]
[Skip(platform: "linux", feature: "nonexistent", reason: "…")]
void t() { }          // → 照跑（platform 不匹配就结束判定，feature 根本没看）
```

要同时受两个条件约束，当前只能拆成两个测试，或者把其中一条改写成测试体内的运行期判断
（`if (!Platform.HasThreads()) { return; }`）。

## `Std.Assert`

住在 `z42.core`（prelude，无需 `using`）。失败一律抛 `Std.TestFailure`。
**全部方法如下，此表即全集**：

### 相等 / 布尔 / null

| 签名 | 通过条件 |
|---|---|
| `Equal(object expected, object actual)` | `expected.Equals(actual)` |
| `NotEqual(object expected, object actual)` | 上式取反 |
| `True(bool condition)` | `condition` |
| `False(bool condition)` | `!condition` |
| `Null(object? value)` | `value == null` |
| `NotNull(object? value)` | `value != null` |
| `Contains(string expected, string actual)` | `actual.Contains(expected)`——**第一个参数是子串** |

### 主动控制

| 签名 | 行为 |
|---|---|
| `Fail(string message)` | 直接抛 `TestFailure(message)` |
| `Skip(string reason)` | 抛 `SkipSignal(reason)`。⚠️ 见[不支持](#不支持--当前不生效) |

### 异常

| 签名 | 通过条件 |
|---|---|
| `Throws(string expectedTypeName, Action body)` | `body` 抛出的异常**短类名全等** `expectedTypeName` |
| `ThrowsAny(Action body)` | `body` 抛出任意异常 |
| `DoesNotThrow(Action body)` | `body` 不抛 |

`Throws` 收的是**短名**、且**不认继承**——这两点与 `[ShouldThrow<E>]` 都不同：

```z42
Assert.Throws("TestFailure", () => Assert.Fail("x"));       // ✅
Assert.Throws("Std.TestFailure", () => Assert.Fail("x"));   // ❌ 全限定名不匹配
Assert.Throws("Exception", () => { throw new MyError(); }); // ❌ 子类不算命中
```

### 数值排序与范围

前五个各有 `long` 与 `double` 两个重载（同名同形，只是参数类型不同）。
**参数顺序是 `(actual, expected)`**，与 `Equal(expected, actual)` **故意相反**——
顺序本身就是被断言的不等式，`Greater(port, 0)` 读作「port > 0」。

| 签名 | 通过条件 |
|---|---|
| `Greater(actual, expected)` | `actual > expected` |
| `Less(actual, expected)` | `actual < expected` |
| `GreaterOrEqual(actual, expected)` | `actual >= expected` |
| `LessOrEqual(actual, expected)` | `actual <= expected` |
| `InRange(actual, min, max)` | `min <= actual <= max`（**闭区间**；开区间自己用 `Greater` + `Less` 拼） |
| `EqualApprox(double actual, double expected, double eps)` | `abs(actual - expected) <= eps`（**只有 `double` 一个重载**） |

`double` 重载对 `NaN` 显式判失败：任一操作数是 `NaN` 即抛，不会静默通过
（朴素的 `<=` 检查会让 `Greater(NaN, x)` 混过去）。`EqualApprox` 用绝对容差，`NaN` 永不命中。

### 数组

只有 `object[]` 重载（`int[]` / `string[]` 等经装箱传入）。比对用 `==`：对象比引用、装箱基元比值。

| 签名 | 通过条件 |
|---|---|
| `ArrayContains(object needle, object[] haystack)` | `haystack` 里有元素 `== needle` |
| `ArrayDoesNotContain(object needle, object[] haystack)` | 上式取反 |
| `ArrayIsEmpty(object[] coll)` | `coll.Length == 0` |
| `ArrayIsNotEmpty(object[] coll)` | `coll.Length != 0` |

`List` / `Dictionary` / `HashSet` 没有对应助手，自己取 `.Count` / `.Contains` 再 `Assert.True`。

## `Std.Test.TestIO`

捕获被测代码写到 stdout / stderr 的内容。`using Std.Test;`。

| 签名 | 返回 |
|---|---|
| `static string captureStdout(Action body)` | `body` 期间写到 stdout 的全部文本 |
| `static string captureStderr(Action body)` | 同上，stderr（stdout 照常透传到进程） |
| `static CaptureResult captureBoth(Action body)` | 两路各自独立捕获 |

`CaptureResult` 公开两个只读属性：`string Stdout` / `string Stderr`。

```z42
var s = TestIO.captureStdout(() => {
    Console.WriteLine("hello");
    Console.WriteLine("world");
});
Assert.Equal("hello\nworld\n", s);
```

- 三个方法都可**嵌套**：内层只看见内层的输出，外层只看见内层捕获窗口之外发生的输出。
- `body` 抛异常时异常**原样透传**（捕获的缓冲被丢弃），所以在 lambda 里写 `Assert.*` 是安全的。

> ⚠️ **lambda 按值捕获——在 lambda 里给外层局部变量赋值，出了 lambda 就看不见了。**
> 这不是 `TestIO` 的特殊规则，是 z42 的[闭包捕获语义](language/closures.md)；但因为
> `capture*` 的返回值只有「捕获到的文本」这一条，需要把别的东西带出来时最容易撞上：
>
> ```z42
> string outer = "before";
> TestIO.captureStdout(() => { outer = "inside"; Console.WriteLine("x"); });
> // outer 仍然是 "before"
> ```
>
> 要带出值就捕获一个**对象**（引用类型按身份共享，写得进去）：
>
> ```z42
> class Cell { public string v; public Cell() { this.v = ""; } }
>
> var c = new Cell();
> TestIO.captureStdout(() => { c.v = "inside"; Console.WriteLine("x"); });
> // c.v == "inside"
> ```

## `[Benchmark]` 与 `Bencher`

基准文件默认放 `bench/`，用 `z42 bench` 跑（`bench/*.z42`，一文件一目标，与 `tests/` 同构）。
`[Benchmark]` 写在 `tests/` 里也会被 `z42 test` 一并跑掉。

### 两种签名

```z42
using Std.Test;

// 形态 1：零参，自己建 Bencher、自己报 label
[Benchmark]
void bench_addition() {
    var b = new Bencher();
    b.iter(() => BenchHelpers.blackBox(1 + 2 + 3));
    b.printSummary("addition");
}

// 形态 2：收一个 Bencher 形参，Bencher 由框架用默认构造器建好传入，
// 结束时以**函数名**为 label 自动汇报
[Benchmark]
void bench_with_arg(Bencher b) {
    b.iter(() => BenchHelpers.blackBox(2 * 21));
}
```

形态 2 是**唯一**允许 `[Benchmark]` 带参数的情形；参数类型必须正是 `Bencher`，
且只能有这一个参数，否则 `E0912`。

### `Bencher` 公开成员

| 成员 | 说明 |
|---|---|
| `Bencher()` | **自适应采样**：warmup 10 次，随后一段试跑估算单次耗时，取 n 填满约 50 ms 的测量预算，n 夹在 `[20, 2000]` |
| `Bencher(int warmupIters, int sampleIters)` | **固定**：warmup `warmupIters` 次，采样恰好 `sampleIters` 次，不自适应 |
| `void iter(Action body)` | 先跑 warmup（不计时），再逐次计时采样；返回后下面的统计量全部就位 |
| `void printSummary(string label)` | 打印一行汇总（格式见下） |
| `int WarmupIters` `{ get; }` | warmup 次数 |
| `int Samples` `{ get; }` | `iter()` **实际**用的采样数（自适应时是算出来的那个 n；`iter()` 之前为 0） |
| `long MinNs` `MaxNs` `MedianNs` `MeanNs` `TotalNs` `{ get; }` | 采样的最小 / 最大 / 中位 / 均值 / 总和，纳秒 |
| `long StdDevNs` `{ get; }` | 总体标准差（÷n），四舍五入到纳秒；亚纳秒离散度舍成 0 |

`BenchHelpers.blackBox(object value)` 是恒等包装，标记「别把这段优化掉」。当前是空操作，
无条件使用是安全的。

`printSummary` 的输出格式是稳定契约：

```
bench[<label>] min=<n>ns median=<n>ns max=<n>ns mean=<n>ns stddev=<n>ns samples=<n>
```

## `z42 test` / `z42 bench`

```
z42 test  [OPTIONS] [target]
z42 bench [OPTIONS] [target]
```

`[target]` 省略 = 从当前目录向上找最近的 `z42.toml`；也可直接给一个已编好的 `.zpkg` / `.zbc`
（那时不重新编译，直接跑）。

### 选目标

| 旗标 | 作用 |
|---|---|
| `--list` | 只把目标名一行一个打出来，不编不跑，退 0 |
| `--name <name>` | 只建 + 跑这**一个**目标。名字不存在 → 报错并列出可用目标名，退 2 |
| `--filter <substr>` | 只跑**目标名**含该子串的那些。零命中 = 正常结果（打一行说明，退 0） |
| `--format <pretty\|json>` | 输出格式，默认 `pretty` |
| `--release` | 用 release profile 编译（默认 debug） |

旗标全表（含把测试组装到设备 / 模拟器上跑的那一组）见
[`z42` 命令面](toolchain/cli-z42.md)。

> `--filter` 与 `--name` 筛的都是**目标名（默认 = 文件 stem）**，不是测试函数名。
> **按测试函数名筛当前无法做到**——帮助里那条标着 `(reserved)` 的同名 `--filter` 没有独立效果，
> 拿它去匹配方法名只会得到「一个目标都没匹配上」。

### 退出码

| 码 | 含义 |
|---|---|
| `0` | 至少发现 1 个单元，且没有失败（全 skipped 也算 0）；或工程压根没有测试目标、`--filter` 筛空 |
| `1` | 有失败；**或某个目标里发现 0 个单元**；或编译失败 |
| `2` | 用法 / 环境错：`--name` 点名的目标不存在、找不到 `z42.toml` |

「**没有测试目标**」（工程里根本没有 `tests/`）与「**目标里没有单元**」（有目标、编出来一个
`[Test]` 都没有）是两回事：前者退 0，后者退 1。

**「发现 0 个单元」判红**是刻意的——「我的测试没跑」不得与「我的测试全过」在退出码上等价。
报告照常打印，另有一段说明走 stderr：

```
  Result: 0 passed, 0 failed, 0 skipped
error: no [Test] or [Benchmark] found in `…/demo.test.empty_tests.zpkg`
  a test artifact with zero discovered tests is treated as a failure —
  …
```

同理，`tests/` 下有 `.z42` 源却一个目标都没发现，也直接判红并回显当前生效的发现 glob。

### `pretty` 输出

```
── test target: math_tests ──
compiled: ./artifacts/demo/debug/build/app.zpkg
compiled: ././artifacts/test-targets/math_tests/build/app.zpkg
  PASS Demo.Tests.test_add_ok
  FAIL Demo.Tests.test_add_wrong: Std.TestFailure: values not equal
  SKIP Demo.Tests.test_not_ready (not implemented yet)

  Result: 1 passed, 1 failed, 1 skipped
```

每行的形状：

| 行 | 形状 |
|---|---|
| 通过 | `  PASS <完全限定函数名>` |
| 失败（抛异常） | `  FAIL <名>: <异常 FullName>: <Message>` |
| 失败（`[ShouldThrow]` 不符） | `  FAIL <名> (expected throw <E>, got <实际类型 \| no throw>)` |
| 跳过（`[Skip]`） | `  SKIP <名> (<reason>)` |
| 跳过（`[Ignore]`） | `  SKIP <名> (ignored)` |

> 失败行只给**异常的类型名和 message**。`Assert.Equal` 失败时 message 固定是 `values not equal`，
> **不含具体的期望值 / 实际值**，也没有 file:line。要在报告里看见值，就自己把值写进
> message：`if (a != b) { Assert.Fail($"add: expected {b}, got {a}"); }`。

基准的汇总行直接写在它自己的 `PASS` 行之前：

```
bench[addition] min=375ns median=417ns max=3583ns mean=432ns stddev=109ns samples=2000
  PASS Demo.Bench.bench_zero_arg
```

### `json` 输出

`--format json` 下 **stdout 只有一个 JSON 对象**，编译进度那些行改走 stderr——
`z42 test --format json > report.json` 拿到的就是干净的报告。

```json
{
  "tool": "z42b",
  "module": "././artifacts/test-targets/math_tests/dist/demo.test.math_tests.zpkg",
  "summary": { "total": 3, "passed": 1, "failed": 1, "skipped": 1 },
  "results": [
    { "name": "Demo.Tests.test_add_ok", "status": "passed", "is_benchmark": false },
    { "name": "Demo.Tests.test_add_wrong", "status": "failed", "is_benchmark": false,
      "reason": "Std.TestFailure: values not equal" },
    { "name": "Demo.Tests.test_not_ready", "status": "skipped", "is_benchmark": false,
      "reason": "not implemented yet" }
  ]
}
```

字段：

| 字段 | 说明 |
|---|---|
| `tool` | 恒为 `"z42b"` |
| `module` | 实际跑的产物路径 |
| `summary` | `total` / `passed` / `failed` / `skipped` 四个整数 |
| `results[].name` | 完全限定函数名 |
| `results[].status` | `"passed"` \| `"failed"` \| `"skipped"` |
| `results[].is_benchmark` | 布尔 |
| `results[].reason` | **仅** failed / skipped 时出现 |
| `results[].bench_stats` | **仅** 基准且汇总行解析成功时出现 |

`bench_stats` 的字段：`label`、`min_ns`、`median_ns`、`max_ns`、`mean_ns`、`stddev_ns`、`samples`
（全是整数，`label` 是字符串）。`mean_ns` / `stddev_ns` 为 `-1` 表示汇总行里没有这两项。

## 不支持 / 当前不生效

这一节列的是**写得出来、编译也过，但运行期没有效果**的东西。写进测试前请先读完。

| 写法 | 实际行为 |
|---|---|
| `[Timeout(milliseconds: N)]` | **完全无效**。编译期会检查 `milliseconds` 存在且为正整数字面量（否则 `E0917`），但运行期没有任何超时保护：贴 `[Timeout(milliseconds: 50)]` 的测试跑满 2 秒照样判 PASS，**死循环的测试会永远挂住整个 `z42 test`**。别指望它兜底。 |
| `Assert.Skip(reason)` | **判失败，不是跳过**。报告里出现 `FAIL …: Std.SkipSignal: <reason>`，退出码 1。运行期才能决定的跳过，当前只能在测试体里 `return`（那会记为通过）。 |
| `[Skip(feature: "拼错的名字")]` | **静默跳过**，不报错。见[上文](#skipfeature-的取值)。 |
| `[Skip(platform: …, feature: …)]` 同贴 | `feature:` 被忽略，见[上文](#platform-与-feature-同时写)。 |
| `[Setup]` 里抛异常 | **被静默吞掉**，测试照常运行、照常可能判通过。fixture 出错不会让任何东西变红——`[Setup]` 里想断言就得改成在测试体里断言。`[Teardown]` 同理。 |
| `[TestCase(args)]` | **不存在**，写了直接 `E0443: undefined type: TestCaseAttribute`。参数化测试当前只能在测试体里自己写循环。 |
| `--format tap` / `--format junit` | **不存在**，只有 `pretty` 和 `json`。 |
| 按测试函数名过滤 | **做不到**。`--filter` 筛的是目标名。 |
| 失败位置 | 报告里**没有** file:line，也没有栈。`TestFailure` 的位置字段当前恒为空。 |
| 每个用例的耗时 | `[Test]` 的报告里**没有**耗时字段（`pretty` 和 `json` 都没有）。要计时用 `[Benchmark]`。 |
| 并行执行 | 一个目标内的单元**串行**执行，顺序即声明顺序。 |

## 相关

- [`z42` 命令面](toolchain/cli-z42.md)——`z42 test` / `z42 bench` 的旗标全表、产物位置、通用退出码
- [工程清单 z42.toml](toolchain/z42-toml.md)——`[tests]` / `[[test]]` / `[benches]` 字段、
  `harness = false` 的退出码目标、三层依赖合并
- [平台与宿主信息](stdlib/platform.md)——`Platform.OS()` / `Capabilities()` 的完整取值与谓词
- [闭包与捕获](language/closures.md)——lambda 按值快照捕获的完整规则
- [异常](language/exceptions.md)——`Exception` 层次与 `catch` 语义
- [自定义 Attribute 与反射](language/attributes.md)——自己写 attribute（本页这些是编译器内建的）
- [错误码全量表](appendix/error-codes.md)——`E0911`–`E0917` 的完整条目
