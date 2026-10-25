# 写测试

代码写完，怎么知道它对？到现在为止靠的是 `z42 run` 加肉眼。这一章把「我觉得它对」换成
**可以反复执行的断言**：测试写在工程的 `tests/` 目录里，`z42 test` 一条命令跑完，告诉你
哪些过了、哪些没过、哪些先跳过了。

> 熟悉 C# 的读者请注意：测试框架不是外装的包，`[Test]` 与 `Assert` 随标准库一起到位，
> 清单里不用加任何依赖；`z42 test` 是工具链自带的命令，没有 runner 可选。

## 测试住在 `tests/`

下面这个库工程只有一个源文件，`tests/` 里放了三个测试文件。**一个文件就是一个测试目标**，
目标名就是文件名去掉扩展名：

```console
{{#include ../../../../examples/practice/testing/math/run.console:files}}
```

被测的代码普普通通：

```z42
// examples/practice/testing/math/src/Math.z42
{{#include ../../../../examples/practice/testing/math/src/Math.z42}}
```

## 一个测试长什么样

一个 `[Test]` 贴在一个**无参、返回 `void` 的自由函数**上，函数体里用 `Assert` 说出期望。
测试文件自己的命名空间写在被测命名空间之下（`MathKit.Tests`），被测类型不用 `using` 就能看见；
`[Test]` 等 attribute 在 `Std.Harness`，这一句 `using` 要写：

```z42
// examples/practice/testing/math/tests/math_tests.z42
{{#include ../../../../examples/practice/testing/math/tests/math_tests.z42:basic}}
```

`Assert.Equal` 的参数顺序是 **(期望, 实际)**；`EqualApprox` 比较浮点数要给容差；
`InRange` 是闭区间。`Assert` 住在 prelude 里，不需要 `using`。

### 断言「它会抛异常」

两种写法。`[ShouldThrow<E>]` 贴在测试上，整个函数抛出 `E`（或 `E` 的子类）才算通过；
`Assert.Throws` 则精确到一条语句，收的是异常的**短类名**字符串，并且不认继承：

```z42
// examples/practice/testing/math/tests/math_tests.z42
{{#include ../../../../examples/practice/testing/math/tests/math_tests.z42:throws}}
```

## 跑起来

在工程目录里 `z42 test`，每个目标一段报告，每个测试一行：

```console
{{#include ../../../../examples/practice/testing/math/run.console:all}}
```

三个状态：`PASS`、`FAIL`、`SKIP`。失败行给出异常类型与 message——注意 `Assert.Equal` 的 message
固定是 `values not equal`，**不含期望值与实际值**；要在报告里看见值，就自己把值写进
`Assert.Fail($"…")` 的 message。有任何失败，退出码就是 `1`（末行的 `[exit: 1]`），
持续集成据此判红。

`wip_tests` 里故意放了一个失败和一个跳过，它们长这样：

```z42
// examples/practice/testing/math/tests/wip_tests.z42
{{#include ../../../../examples/practice/testing/math/tests/wip_tests.z42}}
```

`[Skip(reason: "…")]` 的理由**必填**，会原样印在 `SKIP` 行里——跳过是给自己留的欠条，写清为什么。

### 只跑一个目标

`--name` 点名一个目标，`--list` 只列目标名不跑：

```console
{{#include ../../../../examples/practice/testing/math/run.console:one}}
```

```console
{{#include ../../../../examples/practice/testing/math/run.console:list}}
```

> ⚠️ `--name` 与 `--filter` 筛的都是**目标名**（文件名），不是测试函数名。按函数名筛选目前做不到。

## 每个测试前后做点事：`[Setup]` / `[Teardown]`

`[Setup]` 在**同一命名空间**里的每个测试运行前调用一次，`[Teardown]` 在每个测试之后调用一次
（测试抛了异常也照跑）。配对按命名空间，不按文件也不按类。下面用一个静态计数器验证
「每个测试前都重置过」：

```z42
// examples/practice/testing/math/tests/setup_tests.z42
{{#include ../../../../examples/practice/testing/math/tests/setup_tests.z42}}
```

两个测试各自把计数器加到 1 并断言它等于 1——要是 `[Setup]` 没在第二个测试前再跑一遍，
第二个就会看到 2。它们在上面的报告里都是 `PASS`。

## 签名写错会在编译期被拦住

`[Test]` 对函数签名有硬要求：自由函数或 `static` 方法、返回 `void`、零参数。违反的不是
「跑不过」，而是**编不过**，诊断码 `E0911`：

```z42
// examples/practice/testing/badsig/tests/bad_tests.z42
{{#include ../../../../examples/practice/testing/badsig/tests/bad_tests.z42}}
```

```console
{{#include ../../../../examples/practice/testing/badsig/run.console:run}}
```

三条诊断对应三处错：实例方法、返回 `int`、带了参数。

## 小结

- 测试文件放 `tests/`，一个文件一个目标；`z42 test` 全跑，`--name` 点名，`--list` 只列。
- `[Test]` 贴在无参 `void` 自由函数上，`using Std.Harness;`；`Assert.*` 不用 `using`，`Equal` 是（期望, 实际）。
- 抛异常用 `[ShouldThrow<E>]`（认子类）或 `Assert.Throws("短类名", …)`（不认子类）。
- `[Skip(reason:)]` 跳过要写理由；`[Setup]` / `[Teardown]` 按命名空间配对、每个测试前后各跑一次。
- 任何失败退出码 `1`；签名写错编不过（`E0911`）。

`Assert` 的全部方法、`[Skip]` 的平台与能力条件、`[Benchmark]` 与 JSON 输出，见
[参考手册 · 编写与运行测试](https://z42-lang.github.io/z42/reference/testing.html)。
