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

## Part B — micro A/B 门禁（Stage 2）

- [ ] `_microRunOneToolchain(root,lib,vm,driver,libs,mode,builderZpkg) → label→BenchStats`：
      参数化工具链跑一个 lib 的 bench 模块、采集 stats（优先小重构 `_runLibKind` 注入工具链+captureOnly）。
- [ ] `_benchAbMicro`：对每个 lib，base 工具链 + pr 工具链各 `_microRunOneToolchain`，按 label 配对
      `_abVerdict`，产 `tier=z42-micro` 的 ab.json 片段；base 编译失败/仅 PR 有 → 信息性 skip（不红）。
- [ ] `_abResultJson` 加 `tier` 参数（e2e→"z42-e2e" / micro→"z42-micro"）。
- [ ] `_benchAb`：e2e 后跑 micro（除非 `--ab-e2e-only`），合并 `regr`/`toolErr`/`nb` 计数。
- [ ] CLI：注册 `--ab-e2e-only` / `--ab-micro-only`（默认两跑）。
- [ ] `--ab-selftest`：加一个 micro 配对样例（纯函数层已被 `_abVerdict` 覆盖，主要验配对+skip 逻辑）。
- [ ] bench-pr.yml：确认 base 工具链够 micro 用（已有 `.ab-base-alllibs`）；timeout 视增量 45→60。
- [ ] GREEN：本机快验 `bench --ab --quick --ab-micro-only --base-* …`（origin/main 作 base）+ 交 CI bench-pr。

## Part C — criterion A/B 门禁（Stage 3）

- [ ] bench-pr.yml：加 `if src/runtime changed` 的 criterion A/B step——
      base-src `cargo bench --bench gc_cycle_bench -- --save-baseline ab-base`，
      pr `cargo bench --bench gc_cycle_bench -- --baseline ab-base`，解析 change/estimates.json，
      任一 bench 回归（>10% 且 criterion 判 Regressed）→ 红。
- [ ] smoke_bench 保留但不门禁（注释说明其 sanity 角色）。
- [ ] 文档：criterion 门禁只在 runtime 变时跑；gc_cycle_bench 纳入、smoke 不纳入。
- [ ] GREEN：交 CI（criterion 编译重，本机可选跑 `cargo bench --bench gc_cycle_bench` 冒烟）。

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
