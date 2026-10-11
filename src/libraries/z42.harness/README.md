# z42.harness

## 职责

测试 / bench 的**执行器**：把编好的测试产物加载进 VM，按编译期写入的 TIDX 发现 `[Test]` / `[Benchmark]` / `[Setup]` / `[Teardown]`，逐条执行并归类 pass / fail / skip，输出 pretty 或 JSON 报告。
调用方是 [z42b](../../toolchain/builder/)（`z42b test` / `z42b bench`）与设备端 [test-agent](../../toolchain/workload/test/)，两者共用同一份执行核。

**不做**：写测试用的 API（`TestIO` / `Bencher` / `TestRunner`）在 [z42.test](../z42.test/)，`Assert` 在 z42.core；写测试的工程只依赖 z42.test，不依赖本包。通用的「加载 zpkg / 按 FQN 调用」在 z42.core 的 `Std.Runtime.ModuleLoader`。

## 功能索引

命名空间 `Std.Harness`。

| 功能 | 入口 |
|------|------|
| 加载测试产物并取回 TIDX | `TestModule.Load(path) → TestEntry[]`（`src/TestModule.z42`） |
| 单模块执行 | `Runner.RunModule(path, format)` → 退出码；`Runner.RunModuleResults(path)` → `TestResult[]`（`src/Runner.z42`） |
| bundle 执行（golden 隔离 + unit 共享 VM + 能力门） | `BundleRunner.RunBundle(cases, dir, libs)` / `ExitCode`（`src/BundleRunner.z42`）；golden 走 `TestModule.RunGoldensIsolated` |
| 报告 | 默认 pretty（`PASS` / `FAIL` / `SKIP` + `Result:` 汇总）；`--format json` 产 `TestReport.toJson`（含 `failure_location` / `stack_trace`，benchmark 条目带 `is_benchmark` + `bench_stats`） |
| 零发现判红 | `Runner.ExitCodeFor(runnable, failed)`：一条可跑的测试都没有 ⇒ 退出码 1 |

## 基础用法

```z42
using Std.Harness;

int Main(string[] args) {
    return Runner.RunModule(args[0], "pretty");   // 0 = 全过；1 = 有失败或零发现
}
```

## 如何测试验证

```bash
xtask test stdlib z42.harness       # 报告 JSON 形状、退出码、bundle 能力门
xtask test toolchain builder        # z42b 端到端（编译 → 加载 → 执行 → 报告）
```

## 关联文档

- 机制（TIDX 语义 · runner 协议 · builtin）：[testing/framework.md](../../../docs/internals/src/testing/framework.md)
- 一份测试怎么被跑起来：[devinfra/test-pipeline.md](../../../docs/internals/src/devinfra/test-pipeline.md)

## 核心文件

| 文件 | 类型 | 职责 |
|------|------|------|
| `src/TestModule.z42` | `static class TestModule` + `TestEntry` | 加载测试产物、取回 TIDX 条目；隔离 VM 跑 golden |
| `src/Runner.z42` | `static class Runner` + `InvokeOutcome` | 单模块 `[Test]` / `[Benchmark]` 调度：Setup → 用例 → Teardown、Skip 门、ShouldThrow 匹配 |
| `src/BundleRunner.z42` | `static class BundleRunner` + `BundleCase` | bundle 执行（z42b `--rid host` 与设备 agent 共用） |
| `src/TestReport.z42` | `static class TestReport` + `TestResult` | pretty / JSON 报告 |

## 依赖关系

依赖 `z42.core`（`ModuleLoader.Invoke`、平台能力）+ `z42.io`（Console）+ `z42.test`（`TestIO` 捕获 bench 输出、`BenchStats` 解析摘要）。VM 侧 builtin：`__load_module`（加载 + 返回 TIDX，条目实例化为 `Std.Harness.TestEntry`）、`__run_goldens_isolated`。
