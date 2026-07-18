# z42.test

z42 标准测试库 —— 给 stdlib 自身和用户脚本提供 attribute 注解（[Test] / [Skip] / [ShouldThrow<E>] 等）+ Assert + TestIO + Bencher，配合 [z42-test-runner](../../toolchain/test-runner/) 运行。

## 现状（v0.5, 2026-05-05）

R 系列基础设施已落（R1 / R2 minimal / R2 完整版 / R3 minimal+R3a+R3c / R4.A+R4.B / R5）。当前能力：

| 能力 | 状态 | API |
|---|---|---|
| Attribute 注解 | ✅ R1.C / R4.A / R4.B / add-test-timeout-attribute / add-test-skip-platform-feature-eval | `[Test]` / `[Skip(reason:, platform?:, feature?:)]` (平台/特性条件实际生效) / `[Ignore]` / `[Setup]` / `[Teardown]` / `[Benchmark]` / `[ShouldThrow<E>]` / `[Timeout(milliseconds: N)]` |
| 失败位置展示 | ✅ surface-test-failure-source-location | runner pretty/TAP/JSON 均自动展示 `failure_location` + 完整 `stack_trace`；reason 字段保持向前兼容（in-process; subprocess + JIT 待跟进 spec） |
| Assert 数值比较 | ✅ extend-assert-numeric-and-collection-helpers | `Greater` / `Less` / `GreaterOrEqual` / `LessOrEqual` / `InRange` × `{long, double}`；浮点 NaN guard |
| Assert 数组集合助手 | ✅ 同上 | `ArrayContains` / `ArrayDoesNotContain` / `ArrayIsEmpty` / `ArrayIsNotEmpty` (`object[]`)；前缀 `Array` 避开 z42.core 跨包 overload-resolution 限制 |
| Assert 基础（9 方法） | ✅ R2 minimal | Equal / NotEqual / True / False / Null / NotNull / Contains / Fail / Skip |
| Assert 扩展（lambda） | ✅ R2 完整版 | Throws / DoesNotThrow / EqualApprox |
| TestIO（捕获 console） | ✅ R2 完整版 | captureStdout / captureStderr / captureBoth |
| Bencher（基准测量） | ✅ R2 完整版 | Bencher.iter(Action) / printSummary / Min·Max·Median·Total·Samples + BenchHelpers.blackBox |
| Imperative TestRunner（旧） | ✅ v0 保留 | Begin / Fail / Summary（lambda 前的兼容路径）|
| Runner [Benchmark] 调度 | ✅ add-benchmark-runner-dispatch + rebuild-bench-structured-output | `[Benchmark] void f()` **或** `void f(Bencher b)`（后者编译期 desugar 成前者）；与 `[Test]` 同执行路径。默认 pretty（`PASS`/`FAIL`/`SKIP` + `Result:` 汇总）；`z42b {test,bench} --format json` 产结构化报告 `TestReport`：per-entry `is_benchmark` + benchmark 的 `bench_stats`（`Runner` json 模式捕获 benchmark stdout → `BenchStats.parse`）|

## 推荐用法（lambda 时代）

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
    var b = new Bencher();
    var c = new Counter();
    b.iter(() => { c.n = c.n + 1; });
    Assert.Equal(110, c.n);     // 默认 warmup=10 + samples=100
    b.printSummary("counter");
}

class Counter { public int n; public Counter() { this.n = 0; } }
```

跑测试：`just test-stdlib mylib`（默认串行，in-process VM 保留 [Setup]/[Teardown]）。

**并行执行**（add-test-runner-parallel 2026-05-27）：`z42 xtask.zpkg test lib --jobs N mylib`
或 `--jobs 0` 自动用 `available_parallelism()`。N > 1 强制 subprocess 模式 —
速度上 4–8× 但 [Setup]/[Teardown] 不会运行（VmContext 是 `!Send`，无法跨线程
共享）。z42.crypto 7 文件实测：serial 18s → `--jobs 8` 5.7s。

## 已知限制（待 z42 反射能力增强）

- `Assert.Throws(Action)` 不带类型断言（任意 throw 都算命中）；类型敏感的"应抛特定类型"用 `[ShouldThrow<E>]` 测试级注解
- z42 lambda 对值类型采用快照捕获语义，要把 capture 结果传出 lambda body 必须用引用类型（class wrapper / array），不能直接对外部 int / string 局部变量赋值
- BenchHelpers.blackBox 接 `object` 而非 generic `<T>`（z42 parser 在表达式上下文不识别方法级显式 generic call）

## 旧 v0 imperative TestRunner（保留）

## 使用

```z42
using Std.Test;

void Main() {
    var t = new TestRunner("MyTests");

    t.Begin("Addition");
    try { Assert.Equal(4, 2 + 2); } catch (Exception e) { t.Fail(e); }

    t.Begin("Concatenation");
    try { Assert.Equal("ab", "a" + "b"); } catch (Exception e) { t.Fail(e); }

    return t.Summary();   // exit code = failed 计数
}
```

输出（全过场景）：

```
══════════════════════════════════════
 MyTests
══════════════════════════════════════
  ✓ Addition
  ✓ Concatenation
──────────────────────────────────────
 Result: 2 passed, 0 failed
══════════════════════════════════════
```

## API

| 方法 | 说明 |
|---|---|
| `new TestRunner(string contextName)` | 打印 header（用 contextName） |
| `void Begin(string name)` | 开始一个 case；隐式 finalize 上一个（未 fail 视为 pass）|
| `void Fail(Exception e)` | 标记当前 case 失败 + 打印失败原因 |
| `int Summary()` | finalize 最后一个 case + 打印汇总 + 返回 failed 计数 |

## 路线图

### 已交付（2026-04-29 ~ 2026-05-05）

[Test] attribute 注解发现 + z42-test-runner subprocess 调度（R3 minimal）→ 与 v2 路线图对齐：

```z42
public class MyTests {
    [Test] public void Addition() { Assert.Equal(4, 2 + 2); }
    [Test] public void Concat()   { Assert.Equal("ab", "a" + "b"); }
}
```

由 `just test-stdlib` 自动发现 + 调度。imperative TestRunner v0 仍可用（向后兼容）。

### 后续

- Runner [Benchmark] 调度 + criterion-style baseline diff（独立 spec）
- 类型敏感的 `Assert.Throws<E>(Action)` —— 等 z42 反射能力增强（is X cross-module / generic-E IsInstance / Object.GetType() vtable inheritance 任一修好）

## 设计选择

- **不依赖 native 库** — 纯脚本，可被 stdlib 自身用（一旦 z42 编译速度允许 stdlib 互测）
- **不引入新 builtin** — 复用 `Std.Assert.*` + `Console.WriteLine` + `Exception.Message`
- **Summary 返回 int** — 适合做 `Main()` 的返回值传给 process exit code
- **Begin 是显式动词** —— 故意与 xUnit `[Fact]`/JUnit 隐式风格不同；v0 用户必须手写每个 case 的开始

## 不做（明确否决）

- ❌ 自定义 Assert 类 —— 复用 `Std.Assert`，不重复发明
- ❌ 异步测试支持 —— 等 L3 async/await
- ❌ 参数化测试 —— 等 lambda + collection literals
- ❌ 测试发现 / 自动注册 —— 等 reflection
