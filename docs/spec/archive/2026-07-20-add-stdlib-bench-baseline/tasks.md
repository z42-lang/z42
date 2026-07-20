# Tasks: stdlib bench baseline 捕获 + 本地 diff

> 状态：🟢 已完成 | 创建：2026-07-20 | 完成：2026-07-20
> 子系统锁：`toolchain`（短占，空闲即取）

## 进度概览
- [x] 阶段 1: MicroBenchAgg collector（解析 + schema-v1 序列化）
- [x] 阶段 2: 贯穿 capture 路径（cli 选项 → _testLib → _runUnitsBatched）
- [x] 阶段 3: schema tier + 文档
- [x] 阶段 4: 验证

## 阶段 1: collector
- [x] 1.1 `scripts/xtask_bench.z42`：新增 `MicroBenchAgg` 类——`addModuleJson(json, lib)`
      （results[] → is_benchmark && ContainsKey bench_stats → 累加 schema-v1 项，name=lib.label，
      value=median_ns，ci=min/max，unit=ns，tier=z42-micro）+ `toSchemaV1(commit,branch,os,ts)`
      （复用 `_gitOut`/`_osTag`/`_utcNow` 外壳）+ `count()`

## 阶段 2: capture 路径
- [x] 2.1 `scripts/xtask_cli.z42`：`bench stdlib` 注册 `--json <path>` 选项
- [x] 2.2 `scripts/test/xtask_test_lib_units.z42`：`_runUnitsBatched` 加可选 collector 参数——
      capture 时给 z42b 追加 `--format json` 并 `agg.addModuleJson(rr.Stdout, lib)`，否则维持
      pretty 透传（test 路径传 null → 零变化）
- [x] 2.3 `scripts/test/xtask_test_lib.z42`：`--json` 时建 agg、贯穿、末尾 `File.WriteAllText`
      写 schema-v1 + 打印 `wrote baseline -> <out> (<n> benchmarks)`

## 阶段 3: schema + 文档
- [x] 3.1 `bench/baseline-schema.json`：`tier` enum 追加 `"z42-micro"`
- [x] 3.2 `bench/README.md`：stdlib baseline「捕获 → 优化 → diff」工作流一节
- [x] 3.3 `docs/spec/changes/ACTIVE.md`：登记 toolchain 短占锁

## 阶段 4: 验证
- [x] 4.1 worktree 种子 + 建 xtask
- [x] 4.2 `bench stdlib z42.core --json /tmp/c.json` → 合法 v1 + z42.core.* + value=median_ns
- [x] 4.3 `bench stdlib --json /tmp/all.json` → 全库聚合、name 无冲突
- [x] 4.4 `bench stdlib z42.core`（无 --json）→ pretty 不变（回归保护）
- [x] 4.5 `bench --diff --current /tmp/all.json --baseline /tmp/all.json --threshold-time 0.25`
      → exit 0；人造抬高一条 → exit 1
- [x] 4.6 spec scenarios 逐条覆盖 + 文档同步核对
- [x] 4.7 完整 gate 以 CI 为权威（纯 toolchain，冷环境本地不全验）

## 备注
- 复用现成 `_benchDiff`（零 diff 代码）+ schema-v1；micro 主指标取 median_ns。
- CI 硬门禁**明确不做**（micro 噪声，README 既有结论）→ Deferred `stdlib-bench-baseline-future-ci-nightly`。

## 实施记录
- 验证（nightly 0.33 种子，cold worktree）：单库/全库捕获 → schema-v1 合法（全库 **46 benchmark / 13 库**，
  含 partial-failure 模块的通过项如 z42.math.sqrt/pow）；`bench --diff` 自比 exit 0 / 人造回归 exit 1；
  无 `--json` 时 pretty 逐字节不变、不写文件。
- **捕获设计改进**：一个模块内部分 benchmark 失败时，仍捕获其通过项（z42b 即使有失败也打完整
  TestReport JSON；`addModuleJson` 只收 is_benchmark+bench_stats）——避免因单个失败丢整模块数据。
- **发现 pre-existing 编译器 bug（不在本变更范围）**：`[Benchmark] void f(Bencher b)`（form-2 arg 形）
  的 z42c trampoline desugar 失效 → 运行期 `MethodInfo.Invoke: expects 1 argument, got 0`，3 个 pre-existing
  form-2 基准失败（z42.math bench_abs_loop、z42.test bench_demo/bench_examples 各一）。属 `compiler` 子系统、
  与本 toolchain 变更正交 → 记此、未修；新 bench 一律 form-1。**建议独立 change 修 trampoline**。
