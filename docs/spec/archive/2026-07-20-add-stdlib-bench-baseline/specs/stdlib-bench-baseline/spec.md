# Spec: stdlib bench baseline 捕获

## ADDED Requirements

### Requirement: `bench stdlib --json <out>` 聚合捕获

`xtask bench stdlib [lib] --json <out>` 运行选定库的 `[Benchmark]`,把每条成功解析的
`bench_stats` 聚合成一个 schema-v1 baseline 文档写入 `<out>`;不传 `--json` 时行为不变(pretty)。

#### Scenario: 单库捕获产出 schema-v1 文件
- **WHEN** `xtask bench stdlib z42.core --json bench/results/stdlib.json`
- **THEN** 生成的文件是合法 schema-v1(`schema_version:1` + `commit`/`branch`/`os`/`timestamp` +
  `benchmarks[]`),且含 z42.core 每个 benchmark 一条 `{name:"z42.core.<label>", tier:"z42-micro",
  metric:"time", value:<median_ns>, unit:"ns", ci_lower:<min_ns>, ci_upper:<max_ns>, samples:<n>}`

#### Scenario: 全库捕获聚合到单文件
- **WHEN** `xtask bench stdlib --json <out>`（无 lib → 全部）
- **THEN** `<out>.benchmarks[]` 含所有库所有 benchmark,`name` 以 `<lib>.` 前缀区分,无重名冲突

#### Scenario: 不传 --json 保持 pretty（回归保护）
- **WHEN** `xtask bench stdlib z42.core`（无 `--json`）
- **THEN** 输出与本变更前一致(pretty `bench[...]` 行 + PASS),不产文件

#### Scenario: 非 benchmark 条目不入 baseline
- **WHEN** 某 bench 文件含 smoke `[Test]`（无 bench_stats）
- **THEN** 该 `[Test]` 不出现在 `benchmarks[]`（仅 `is_benchmark==true` 且带 `bench_stats` 者入）

### Requirement: 复用 `bench --diff` 对比 stdlib baseline

`xtask bench --diff --current <stdlib-json> --baseline <stdlib-baseline>` 用既有 `_benchDiff`
逻辑对比 micro baseline,无需 micro 专用 diff。

#### Scenario: 回归超阈值 → exit 1
- **WHEN** current 某 `z42-micro` 项 median_ns 比 baseline 高出 `--threshold-time`（如 0.25）
- **THEN** 报 `↑ <pct>%`,退出码 1

#### Scenario: 改进 / 持平 / 新增 / 移除
- **WHEN** 低于负阈值 → `↓`(不 fail);阈值内 → `≈`;current 独有 → `(new)`;baseline 独有 → `(removed)`
- **THEN** 均不计回归,退出码 0（无其它回归时）

### Requirement: schema 承认 z42-micro tier

#### Scenario: z42-micro 通过 schema 校验
- **WHEN** baseline 含 `tier:"z42-micro"` 的项
- **THEN** 符合 `bench/baseline-schema.json`(tier enum 含 `z42-micro`);既有 `z42-e2e` 等不受影响

## Pipeline Steps

纯 toolchain(z42 写的 xtask)+ 数据 schema,无编译器/VM/语言改动。
- [x] toolchain（xtask bench stdlib 捕获路径 + schema）
- [ ] 编译器 / VM / stdlib 运行时 —— **不涉及**
