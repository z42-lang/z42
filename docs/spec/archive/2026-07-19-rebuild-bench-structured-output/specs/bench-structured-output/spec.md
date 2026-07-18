# Spec: bench 结构化输出

## ADDED Requirements

### Requirement: BenchStats 解析 `bench[...]` 行

`Std.Test.BenchStats.parse(string line)` 把 benchmark 的规范摘要行解析为结构化字段，
malformed 输入返回 `null`（不降级为 sentinel 值）。规范格式（`Bencher.printSummary` 契约）：

```
bench[<label>] min=<n>ns median=<n>ns max=<n>ns samples=<n>
```

#### Scenario: 规范格式解析成功
- **WHEN** 输入 `"bench[addition] min=3875ns median=3958ns max=9042ns samples=100"`
- **THEN** 返回 `BenchStats{ label="addition", min_ns=3875, median_ns=3958, max_ns=9042, samples=100 }`

#### Scenario: 无 bench 前缀返回 null
- **WHEN** 输入不含 `bench[` 前缀（如 `"  PASS Foo.bar"` 或空串）
- **THEN** 返回 `null`

#### Scenario: 字段缺失/格式错乱返回 null
- **WHEN** 输入 `"bench[x] min=1ns max=2ns"`（缺 median / samples）
- **THEN** 返回 `null`（不猜测缺失字段）

#### Scenario: 多行输入取最后一个 bench 行
- **WHEN** 输入含多行、其中若干 `bench[...]` 行
- **THEN** 解析**最后一个**合法 `bench[...]` 行（mirror 旧 runner `picks_last`；一个 benchmark 只应产一行，多行取末行最鲁棒）

### Requirement: Runner json 输出模式

`Std.Test.Runner.RunModule(string path, string format)` 在 `format == "json"` 时输出单个 JSON
报告对象到 stdout（抑制 pretty 逐行），`format == "pretty"`（或其它值）保持既有 pretty 行为不变。
退出码语义不变（0=全通过，1=有失败）。

#### Scenario: json 模式产出结构化报告
- **WHEN** 以 `format="json"` 运行一个含 1 个 `[Test]`（通过）+ 1 个 `[Benchmark]` 的模块
- **THEN** stdout 是单个 JSON 对象，含 `"module"`、`"summary":{total,passed,failed,skipped}`、
  `"results":[...]`；benchmark 那条 result `"is_benchmark":true` 且带 `"bench_stats":{label,min_ns,median_ns,max_ns,samples}`

#### Scenario: json 模式抑制 pretty 逐行
- **WHEN** json 模式运行
- **THEN** stdout **不含** `"  PASS ..."` / `"  Result: ..."` 等 pretty 行；只有 JSON

#### Scenario: pretty 模式行为不变（回归保护）
- **WHEN** 以 `format="pretty"`（默认）运行
- **THEN** 输出与本变更前逐字节一致（`PASS`/`FAIL`/`SKIP` 行 + `Result:` 汇总）

#### Scenario: 失败结果带 reason
- **WHEN** json 模式下某 `[Test]` 抛异常
- **THEN** 该 result `"status":"failed"` 且 `"reason"` 含异常类型与消息；JSON 字符串正确转义（引号/反斜杠/换行）

### Requirement: z42b `--format` flag

`z42b test` / `z42b bench` 接受 `--format {pretty|json}`，默认 `pretty`，透传给 `RunModule`。

#### Scenario: bench --format json
- **WHEN** `z42b bench --format json <compiled.zpkg>`
- **THEN** 输出 benchmark 的 JSON 报告，退出码 0（无失败）

#### Scenario: 缺省与非法值
- **WHEN** 不传 `--format`
- **THEN** 等价 `pretty`
- **WHEN** 传 `--format json`
- **THEN** 走 json 模式（其它值按 pretty 处理，不报错——保守）

## MODIFIED Requirements

### Requirement: printSummary doc 契约指向 z42 runner

**Before:** `Bencher.printSummary` doc 注释宣称该行被
`src/toolchain/test-runner/src/exec.rs::extract_bench_stats_from_stdout`（已删除）解析。

**After:** doc 注释描述该行被 `Std.Test.Runner`（json 模式）经 `BenchStats.parse` 解析进
`TestResult.bench_stats`；格式契约的对端改为 `BenchStats.parse` + `tests/bench_stats.z42`。

## Pipeline Steps

纯 stdlib + toolchain，无编译器/VM 改动：
- [ ] Lexer / Parser / TypeChecker / IR Codegen / VM interp — **均不涉及**
- [x] stdlib（z42.test 反射 runner）
- [x] toolchain（z42b CLI flag）
