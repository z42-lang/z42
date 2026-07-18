# Proposal: 重建 bench 结构化输出（z42b 原生 runner）

> 状态：DRAFT（2026-07-19）｜类型：feat（新输出接口契约 → spec-first 完整流程）
> 子系统：`stdlib`（z42.test）+ `toolchain`（z42b）——**短占 stdlib 锁**（converge-z42c-onto-z42-project 持有，User 授权预抢，隔离 worktree，归档即归还）

## Why

`retire-test-runner`（2026-06-30）删除 Rust `z42-test-runner` 时，连同它的多格式输出层
（JSON/TAP/pretty）一起退役。接管的 z42b `Std.Test.Runner`（`Runner.z42`）是薄 verb，
**只有 pretty console 输出**——benchmark 的 `bench[...]` 行既不被解析成结构化数据，也没有任何
机器可读输出模式。

后果有二：

1. **文档在撒谎**：`Bencher.z42` 的 `printSummary` doc 注释（第 100–113 行）与 `README.md`
   仍宣称该行会被 `test-runner/src/exec.rs::extract_bench_stats_from_stdout` 解析成
   `TestResult.bench_stats` JSON——但那个文件已随 test-runner 删除。这是 stale 契约。
2. **micro 基准无机器可读输出**：roadmap 0.4.x「B 流」要求「每包 bench baseline」。没有结构化
   输出，baseline 采集/回归 diff 无从建立——micro 基准只能人眼看 stdout。

本变更在 z42b 原生 runner 内**重建结构化输出契约**：捕获 benchmark 的 `bench[...]` 行 → 解析为
`BenchStats` → 汇总进 `TestResult` → 以 JSON 输出（`--format json`）。这是把旧 Rust runner 的
`extract_bench_stats_from_stdout` 契约在 z42 里以纯脚本重实现（dogfood），并顺带修正 stale 文档。

## What Changes

- **z42.test 新增 `BenchStats` 类型 + `parse(line)` 静态方法**：把 `bench[<label>] min=<n>ns
  median=<n>ns max=<n>ns samples=<n>` 行解析为结构化字段；malformed 返回 null（不降级 sentinel）。
- **z42.test 新增 `TestResult` + `TestReport`**：per-entry 结果（name/status/is_benchmark/reason?/
  bench_stats?）+ 报告级 JSON 序列化（手写，z42.test 无依赖不能引 z42.json）。
- **`Runner.RunModule` 增 `format` 参数**：`pretty`（默认，保持现有行为不变）/ `json`。
  json 模式下逐 entry 捕获 stdout（复用 `TestIO.captureStdout`）、benchmark 行解析进
  `bench_stats`、末尾输出单个 JSON 报告对象。
- **z42b `test`/`bench` 增 `--format {pretty|json}` flag**：透传到 `RunModule`。
- **修正 stale 文档**：`Bencher.z42` doc 注释指向新 z42 契约（删 exec.rs 引用）；`README.md`
  Runner [Benchmark] 行更新；book 机制页记录 capture→parse→emit 流程。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42.test/src/BenchStats.z42` | NEW | `BenchStats` 类型 + `parse(string)→BenchStats?` |
| `src/libraries/z42.test/src/TestReport.z42` | NEW | `TestResult` + `TestReport`（汇总 + 手写 JSON 序列化 + 字符串转义） |
| `src/libraries/z42.test/src/Runner.z42` | MODIFY | `RunModule(path, format)`；json 模式 capture/parse/collect/emit；pretty 路径不变 |
| `src/libraries/z42.test/src/Bencher.z42` | MODIFY | 修正 `printSummary` stale doc 注释（删 exec.rs 引用，描述 z42 新契约） |
| `src/libraries/z42.test/tests/bench_stats.z42` | NEW | `BenchStats.parse` + JSON 序列化 shape 的 `[Test]` 单测 |
| `src/libraries/z42.test/README.md` | MODIFY | Runner [Benchmark] 行更新为 z42b 结构化 JSON |
| `src/toolchain/builder/core/builder_cli.z42` | MODIFY | `test`/`bench` 注册 `--format` flag |
| `src/toolchain/builder/core/builder_test.z42` | MODIFY | 读 `--format` 透传 `RunModule` |
| `docs/book/src/dev/test-gate.md` | MODIFY | 记录 bench 结构化输出机制（capture→parse→emit）与 `--format json` 用法 |
| `docs/spec/changes/ACTIVE.md` | MODIFY | 登记 stdlib 短占锁 |
| `docs/spec/changes/rebuild-bench-structured-output/` | NEW | 本 change 容器（proposal/spec/design/tasks） |

**只读引用**：

- `src/libraries/z42.test/src/ModuleLoader.z42` — `TestEntry` 结构（Kind/Flags/Qualified）
- `src/libraries/z42.test/src/TestIO.z42` — `captureStdout` 复用
- `docs/spec/archive/2026-06-30-retire-test-runner/` — 旧契约来源
- 旧 `test-runner/src/{result.rs,exec.rs,format/json.rs}`（git 历史 `80b7947a^`）— 旧 schema 参照

## Out of Scope

- **不重建 TAP/JUnit 格式**：只做 pretty + json（TAP/JUnit 无消费方，YAGNI）。
- **不改 `[Benchmark]` desugar / 不让 benchmark 函数返回 Bencher**：那需动 z42c 编译器（`compiler`
  锁被 split-irgen-class 占用）→ 越界。本变更走「捕获 stdout + 解析」路径（旧 runner 同款契约）。
- **不引入独立 `z42.bench` 包**：roadmap 0.4.x B1 独立事项。
- **不接 baseline diff / CI 门禁**：本变更只产出结构化数据，消费侧（每包 baseline）留后续。
- **不改 `printSummary` 输出格式**：保持 `min/median/max/samples`，避免破坏现有 bench 输出断言。
- **不改 duration_ms / stack_trace 富信息**：z42 in-process runner 无逐测计时；JSON 省略这些字段。

## Open Questions

- [ ] JSON 报告 schema 是否需与旧 Rust schema 完全一致，还是允许精简（省 duration_ms/stack_trace）？
      → design.md Decision 3 拟精简，待 User 确认。
