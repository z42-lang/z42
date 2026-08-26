# tasks — micro + criterion tier A/B

> 每个 Part 各自 GREEN 再进下一个。A 是 B 的前置。

## Part A — Bencher 富化统计（前置，独立 PR）✅ 完成（本地 GREEN）

- [x] `Bencher.z42`：加 `MeanNs`/`StdDevNs`（long），`iter` 两遍扫算 mean+population stddev（round）。
      **并做自适应采样**（User 裁决「本次一并做」）：默认构造器 pilot 估算 → 50ms 预算 clamp [20,2000]；
      显式 `Bencher(W,S)` 保持固定。
- [x] `Bencher.printSummary`：行契约追加 `mean=<n>ns stddev=<n>ns`（max 后、samples 前）。更新文件头注释契约。
- [x] `BenchStats.z42`：加字段 `MeanNs`/`StdDevNs`；`_parseLine` 读新 key，缺失置 -1（不使整行 malformed）。
- [x] `TestReport._statsJson`：JSON 加 `mean_ns`/`stddev_ns`。
- [x] `MicroBenchAgg.addModuleJson`：读 `mean_ns`/`stddev_ns` 写进 schema-v2 对象（信息性 baseline 富化）。
- [x] `tests/bench_stats.z42`：加 mean/stddev 解析 + 缺失回落 -1 断言；`tests/dogfood.z42` 默认 Bencher 改断言
      自适应契约；`README.md` 更新默认构造说明。
- [x] GREEN：`test stdlib z42.test` 全绿（26 passed）；`bench stdlib z42.test` 新格式 mean=/stddev= 正确、
      adaptive samples 5/20/1898/2000 分模式；`--json` 微基线携带 mean_ns/stddev_ns。CI 待 PR。

## Part B — micro A/B 门禁（Stage 2）✅ 完成（本地 GREEN + CI 待 PR）

> **实现精化**（见 design.md B1.1）：micro `[Benchmark]` 进程内跑 → base+pr stdlib 无法共载 → 改「两个
> 隔离 `bench stdlib --json`（base 树/pr 树同机）+ 纯函数 `bench --micro-diff` diff」。零深度重构，
> `bench stdlib --json` 原样复用（Part A 已让它产 mean_ns/stddev_ns）。

- [x] `bench --micro-diff --current <prMicro.json> --baseline <baseMicro.json> [--threshold-time T]`
      纯命令（`_benchMicroDiff`）：按 name+画像键配对 → `_abVerdict`（复用 e2e 同函数）→ R_lower>1+thr 判红；
      base 无对应基准 → skip。helper `_microMean/_microSd/_microN`（mean_ns/stddev_ns/samples，-1 回落 no-ci）。
- [x] CLI：`--micro-diff` flag 注册 + `_benchE2e` 路由（复用现有 `--current`/`--baseline`/`--threshold-time`）。
- [x] bench-pr.yml：加 3 步——PR 树 `bench stdlib --json micro-pr.json` / base 树（cwd=base-src、复用
      base-src/artifacts/build、cp base_vm）`bench stdlib --json micro-base.json` / `bench --micro-diff` 门禁；
      timeout 45→60；upload 带两份 micro baseline。
- [x] 本地 GREEN：`bench --micro-diff` 真 diff（pr vs pr）7/7 overlap exit 0；合成回归（pr vs 2×-fast base）
      7/7 regression exit 1。CI 两-树编排待 PR bench-pr 自证。
- [~] 逃逸阀 `--ab-e2e-only`/`--ab-micro-only`：**不需要**（micro 是独立 `bench stdlib`+`--micro-diff`，非折进
      `bench --ab`）——精化后 e2e 与 micro 天然解耦，各自 CI step。

## Part C — criterion A/B 门禁（Stage 3）✅ 完成（本机验 parser + criterion 机制；CI 待 runtime PR）

- [x] bench-pr.yml：加 `if src/runtime changed`（`git diff --quiet base..HEAD -- src/runtime`）的 criterion
      step——共享 `CRITERION_HOME`；base-src `--save-baseline ab-base`，pr `--baseline ab-base`；python3
      解析 `<bench>/change/estimates.json` 的 `mean.point_estimate`+`confidence_interval.lower_bound`，
      >10% 且 CI 下界>0 → exit 1。
- [x] smoke_bench 保留不门禁（step 注释说明 sanity 角色，只 gate gc_cycle_bench）。
- [x] 文档：benchmarking.md 加 criterion A/B 节 + tier 表 + CI 步骤；roadmap Deferred 划掉；README。
- [x] 本机验：`cargo bench --bench gc_cycle_bench -- --save-baseline ab-base` + `--baseline ab-base` 跑通，
      criterion 产 `change/estimates.json`（schema/路径与 parser 假设一致）；parser 对真实输出判 exit 0。
      **CI 两-树 A/B 仅 src/runtime 改动 PR 触发**——本 PR 不动 runtime → 该 step 会 skip（不自证，
      待将来 runtime PR 验；与 Stage 1 格式-bump 边角同类不可本地全验）。

## 文档（doc-check）

- [ ] `docs/book/src/dev/benchmarking.md`：新增 micro/criterion tier A/B 两节（机制 + 噪声降级语义 + 触发门控）。
- [ ] `bench/README.md`：CI 节补 micro/criterion tier。
- [ ] `docs/roadmap.md`：`ab-bench-micro`/`ab-bench-criterion` 从 Deferred 划掉，链到本 change 归档。
- [ ] Bencher.z42 / BenchStats.z42 文件头契约注释更新（行格式）。

## 验证锚点（勿重新探索）

- micro 编译/运行/解析既有实现：`scripts/test/xtask_test_lib.z42:_testLibCore/_runLibKind`、
  `xtask_test_targets.z42:385+`（stdlib per-lib driver）。
- micro 聚合：`scripts/xtask_bench.z42:598 MicroBenchAgg`。
- A/B 既有：`scripts/xtask_bench.z42:139 _abVerdict` / `:188 _abOneScenario` / `:248 _benchAb`。
- criterion benches：`src/runtime/benches/{gc_cycle_bench,smoke_bench}.rs`，`Cargo.toml:226 [[bench]]`。
- CI base build：`.github/workflows/bench-pr.yml:86 Build base toolchain`。
