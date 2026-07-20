# Design: stdlib bench baseline 捕获 + 本地 diff

## Architecture

```
xtask bench stdlib [lib] --json <out>
  └─ _testLib(r,"bench")                      [xtask_test_lib.z42]
       json = r.GetOption("json")
       MicroBenchAgg agg = (json!="") ? new MicroBenchAgg() : null
       └─ _runUnitsBatched(..., agg)          [xtask_test_lib_units.z42]
            per module m:
              agg==null → z42b bench m         → Console.Write(stdout)   (pretty, 不变)
              agg!=null → z42b bench m --format json → agg.addModuleJson(stdout, lib)
       末尾：json!="" → File.Write(out, agg.toSchemaV1(commit,branch,os,ts))

xtask bench --diff --current <out> --baseline <base> --threshold-time 0.25
  └─ _benchDiff（现成，零改动；schema-v1 通用，按 name+metric 匹配）
```

## Decisions

### Decision 1: 复用 schema-v1 + `_benchDiff`，不发明 micro 专用格式/diff

**问题：** micro 的 `bench_stats`(min/median/max/samples,单位 ns)与 e2e schema-v1
(`value/unit/metric/tier/ci_lower/ci_upper/samples`)不同构。要不要 micro 专用 baseline + diff?

**决定：** **映射进 schema-v1**——`value=median_ns, unit="ns", metric="time",
tier="z42-micro", ci_lower=min_ns, ci_upper=max_ns`。好处:①**`_benchDiff` 零改动**直接可用
(name+metric 匹配、阈值、`↑↓≈`、exit 0/1/2);②与 e2e baseline 同一工具面,一致的心智;
③schema 只需 enum 加一项。代价:median 为主指标(min/max 进 ci 字段,diff 只比 median)——对
"优化前后量化"足够。

### Decision 2: 主指标取 median_ns

**决定：** baseline 的 `value` 用 `median_ns`(非 min/mean)。median 对 GC 抖动/调度尖峰更稳,
是 micro-bench 的常规主指标。min/max 存 ci_lower/ci_upper 供人参考。

### Decision 3: capture 贯穿共享的 `_runUnitsBatched`，用 null-collector 守护 test 路径

**问题：** `_runUnitsBatched` 同时服务 `test stdlib` 与 `bench stdlib`。加 capture 不能碰坏 test。

**决定：** 加一个**可选 collector 参数**(test 路径传 null)。`agg==null` 完全走原路径
(pretty 透传,逐字节不变);`agg!=null` 才给 z42b 追加 `--format json` 并解析 stdout。test
路径 collector 恒 null → 零行为变化。z42b 的 pretty/json 两模式我已在 rebuild 那轮验证。

### Decision 4: 不进 CI 硬门禁（事实校正）

`bench/README` 明确 micro 不进 CI(ns 级共享-runner 噪声)。本变更只给**本地/nightly**工具:
`--json` 捕获 + `bench --diff` 对比。CI 持久化/nightly job 若做,另立 change 且宽阈值。roadmap
B3(e2e 硬门禁)与此正交。

## Implementation Notes

- **`MicroBenchAgg`**(新类,置 `xtask_bench.z42`,同 `Z42Xtask` 命名空间,跨文件可用):
  - `addModuleJson(string moduleJson, string lib)`:`JsonValue.Parse` → `results[]`,对每个
    `res.Get("is_benchmark").AsBool() && res.ContainsKey("bench_stats")` 者,取 `bench_stats`
    的 label/median_ns/min_ns/max_ns/samples,拼一条 schema-v1 benchmark 对象累加(逗号分隔串)。
    `name = lib + "." + label`。
  - `toSchemaV1(commit,branch,os,ts)`:套 schema-v1 外壳(复用 `_bench` 同款手拼 + `_gitOut`/
    `_osTag`/`_utcNow`),`benchmarks:[ <累加项> ]`。
  - `count()`:入账条数(空则写空 `benchmarks:[]`,仍合法 v1)。
- **JSON 解析**：z42b 输出是紧凑单行 JSON;`JsonValue.Parse` 直接吃。`ContainsKey` 判 bench_stats
  存在(z42.json 有该 API;`Get` 缺键会抛)。
- **z42b `--format json` 传参**：`_runUnitsBatched` capture 分支在 `.Arg(verb).Arg(arts[b])`
  后追加 `.Arg("--format").Arg("json")`。
- **lib 名来源**：`_runUnitsBatched` 已知当前 lib(或从 unit 名/artifact 路径取);传给 `addModuleJson`。
- **写盘**：`_testLib` 末尾 `File.WriteAllText(_absUnder(root, json), agg.toSchemaV1(...))`,打印
  `wrote baseline -> <out> (<n> benchmarks)`。
- **文件行数**：`MicroBenchAgg` 预期 ~40 行,`xtask_bench.z42` 增量后核对不越 500 硬限。

## Testing Strategy

- **功能验证**(本地实跑,cold worktree + nightly 种子):
  - `xtask bench stdlib z42.core --json /tmp/c.json` → 校验文件是合法 v1、含 z42.core.* 项、
    value=median_ns。
  - `xtask bench stdlib --json /tmp/all.json` → 全库聚合,name 无冲突。
  - `xtask bench stdlib z42.core`(无 --json)→ pretty 不变(回归)。
  - `bench --diff --current /tmp/all.json --baseline /tmp/all.json --threshold-time 0.25` →
    exit 0(自比无回归);人造抬高一条 → exit 1。
- **schema 校验**:产出文件对 `bench/baseline-schema.json` 结构自查(字段齐全、tier=z42-micro)。
- **完整 gate**:纯 toolchain 变更,`test stdlib`/`test compiler` 不受影响(collector 守护);
  完整 `xtask test` 以 CI 为权威(冷环境)。

## Deferred / Future Work

### stdlib-bench-baseline-future-ci-nightly
- **来源**：本 change design Decision 4
- **触发原因**：micro ns 级在共享 CI runner 噪声大,PR 硬门禁会误报(README 既有结论)。
- **前置依赖**：稳定的专用 runner 或 nightly job + 经验校准的宽阈值 + baseline 持久化到
  `bench-baselines` 分支(`baselines/stdlib-<runner>.json`,对齐 e2e)。
- **触发条件**：0.4.x B 流推进到 CI 布线时。
- **当前 workaround**：本地/手动 `--json` 捕获 + `bench --diff`。
