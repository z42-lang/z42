# Tasks: 重建 bench 结构化输出

> 状态：🟡 进行中 | 创建：2026-07-19
> 子系统锁：`stdlib`（短占，converge-z42c-onto-z42-project 授权预抢）+ `toolchain`（空闲）

## 进度概览
- [ ] 阶段 1: BenchStats 解析
- [ ] 阶段 2: TestReport（TestResult + JSON）
- [ ] 阶段 3: Runner json 模式
- [ ] 阶段 4: z42b --format flag
- [ ] 阶段 5: 单元测试
- [ ] 阶段 6: 文档修正
- [ ] 阶段 7: GREEN 验证

## 阶段 1: BenchStats 解析
- [ ] 1.1 NEW `src/libraries/z42.test/src/BenchStats.z42`：`BenchStats` 类型（label/min_ns/median_ns/max_ns/samples）+ `parse(string line)→BenchStats?`（malformed→null，多行取末行）

## 阶段 2: TestReport
- [ ] 2.1 NEW `src/libraries/z42.test/src/TestReport.z42`：`TestResult`（name/status/is_benchmark/reason?/bench_stats?）+ `TestReport.toJson(module, results)`（手写序列化 + `_jsonEscape`）

## 阶段 3: Runner json 模式
- [ ] 3.1 MODIFY `Runner.z42`：`RunModule(path, format)` 签名；抽 `_runOne` 返回 `TestResult`
- [ ] 3.2 json 模式：`TestIO.captureStdout` 包裹 invoke；benchmark 捕获文本跑 `BenchStats.parse`；收集 results；末尾 `TestReport.toJson` 输出；抑制 pretty 行
- [ ] 3.3 pretty 模式：保持逐字节不变（回归保护）

## 阶段 4: z42b --format flag
- [ ] 4.1 MODIFY `builder_cli.z42`：`test`/`bench` 各 `AddOption("format", "", "output format: pretty|json", "pretty")`
- [ ] 4.2 MODIFY `builder_test.z42`：`_runModule` 读 `r.GetOption("format")` 透传 `RunModule`

## 阶段 5: 单元测试
- [ ] 5.1 NEW `src/libraries/z42.test/tests/bench_stats.z42`：`[Test]` × canonical / no-bench-line→null / malformed→null / 多行取末行 / `TestReport.toJson` shape + 转义

## 阶段 6: 文档修正
- [ ] 6.1 MODIFY `Bencher.z42`：printSummary doc 注释删 exec.rs 引用，改指 `BenchStats.parse` + `tests/bench_stats.z42`
- [ ] 6.2 MODIFY `README.md`：Runner [Benchmark] 行更新为 z42b 结构化 JSON（`--format json`）
- [ ] 6.3 MODIFY `docs/book/src/dev/test-gate.md`：记录 capture→parse→emit 机制 + `--format json` 用法
- [ ] 6.4 MODIFY `docs/spec/changes/ACTIVE.md`：登记 stdlib 短占锁

## 阶段 7: GREEN 验证
- [ ] 7.1 worktree 种子供给（Z42_HOME/镜像主树工具链）+ `cargo build --release`（z42vm）
- [ ] 7.2 `xtask test stdlib z42.test`（含新单测 + 现有 dogfood/golden 回归）
- [ ] 7.3 集成冒烟：`z42b bench --format json <bench_examples.zpkg>` 肉眼验 JSON
- [ ] 7.4 完整 `xtask test`（e2e + cross-zpkg + stdlib + compiler + vscode-syntax）
- [ ] 7.5 spec scenarios 逐条覆盖确认
- [ ] 7.6 文档同步核对（阶段 9 触发矩阵）

## 备注
- Option B（benchmark 返回 Bencher）受 compiler 锁阻断 → Deferred `bench-structured-future-return-bencher`（design.md）。
- z42.core bench 套件是**独立后续 commit**（test 类型），不在本 change。
- **实施期发现 z42c 前端 bug**（Decision 5 + Deferred `bench-structured-future-array-param`）：数组
  类型参数 `T[]`（用户类元素）在特定上下文被误解析为 `new T[...]`（`E0401: unknown type in `new`: ]`）。
  二分探针 A–S 定位；属被占的 compiler 子系统 → 本变更**规避不修**：`TestReport` 改 `report + resultJson`
  无数组参数设计。Runner 遍历局部数组拼 body（局部 `new T[]` 不触发）。
- **GREEN 环境**：本地 fead63ff（stale 本地 main，落后 origin/main 8 提交、缺 rel 热修）无 warm 工具链；
  用 Jul-15 nightly SDK（format-32、与 fead63ff 同为 pre-property 世界）作种子冷建验证 z42.test。
  origin/main 已加 `__property_get_value`（property 世界），与本地 nightly z42vm 不兼容 → 完整
  origin/main GREEN 以 CI 为权威（冷环境本地不可验自举链，沿用 stdlib 短占先例）。归档前 rebase 到 origin/main。
