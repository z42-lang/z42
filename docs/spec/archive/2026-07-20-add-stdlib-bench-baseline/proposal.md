# Proposal: stdlib bench baseline 捕获 + 本地 diff

> 状态：DRAFT（2026-07-20）｜类型：feat（新 CLI 选项 + 新 baseline 数据面）
> 子系统：`toolchain`（scripts/xtask + bench/）——短占（空闲即取）

## Why

`rebuild-bench-structured-output`(2026-07-19) 让 z42b `--format json` 能产出结构化
`bench_stats`(min/median/max/samples),`补 11 库 bench 套件`(2026-07-20) 铺满 16/24 库的
micro-benchmark。但 **xtask `bench stdlib` 至今只 pretty 透传**——不采集、不落盘、不能对比。
优化者只能肉眼比对 stdout,无法把"优化前基线"固化、无法机器化量化收益。

roadmap 0.4.x B 流 B5(perf 库 baseline 铺面)正需要这一步:**结构化 bench_stats → baseline
文件 → 回归 diff**。本变更补齐 micro-bench 的「捕获 + 本地/nightly diff」能力(不含 PR 硬门禁
——micro 的 ns 级数字在共享 runner 上噪声过大,`bench/README` 已明确 micro 不进 CI 硬门禁;
硬门禁是 e2e 的 B3,与此正交)。

## What Changes

- **`xtask bench stdlib --json <out>`**:各库 bench 走 z42b `--format json`,聚合所有
  `bench_stats` 到**一个** schema-v1 baseline 文件(默认 `bench/results/stdlib.json`)。
  每条映射为 `{name:"<lib>.<label>", tier:"z42-micro", metric:"time", value:median_ns,
  unit:"ns", ci_lower:min_ns, ci_upper:max_ns, samples}`。
- **复用现有 `xtask bench --diff`** 做回归对比(零新 diff 代码):`bench --diff
  --current bench/results/stdlib.json --baseline <基线> --threshold-time 0.25`(micro 噪声
  大 → 阈值放宽)。`_benchDiff` 按 `name+metric` 匹配、`↑↓≈`、exit 0/1/2,已通用。
- **schema 加 `z42-micro` tier**:`bench/baseline-schema.json` 的 `tier` enum 追加一项。
- **文档**:`bench/README` 增 stdlib baseline「捕获 → 优化 → diff」工作流一节。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `scripts/xtask_cli.z42` | MODIFY | `bench stdlib` 注册 `--json <path>` 选项 |
| `scripts/test/xtask_test_lib.z42` | MODIFY | `--json` 时建 collector、贯穿 run、末尾写 schema-v1 文件 |
| `scripts/test/xtask_test_lib_units.z42` | MODIFY | `_runUnitsBatched` 加可选 collector：capture 时给 z42b 传 `--format json` 并解析 stdout → 累加,否则维持 pretty 透传 |
| `scripts/xtask_bench.z42` | MODIFY | 新增 `MicroBenchAgg` collector 类（parse module JSON → schema-v1 benchmark 对象）+ 写盘 helper（复用 `_gitOut`/`_osTag`/`_utcNow`） |
| `bench/baseline-schema.json` | MODIFY | `tier` enum 追加 `"z42-micro"` |
| `bench/README.md` | MODIFY | stdlib baseline 捕获 + 本地 diff 工作流 |
| `docs/spec/changes/ACTIVE.md` | MODIFY | 登记 toolchain 短占锁 |
| `docs/spec/changes/add-stdlib-bench-baseline/` | NEW | 本 change 容器 |

**只读引用**：`src/toolchain/builder/core/builder_test.z42`（z42b `--format` 契约）、
`src/libraries/z42.test/src/TestReport.z42`（JSON schema）、`bench/baseline-schema.json`（v1 结构）。

## Out of Scope

- **不加 PR 硬门禁**：micro 噪声大,README 明确不进 CI；本变更只给本地/nightly 工具。CI 布线(若做)
  另立 change,且应为 nightly-only + 宽阈值。
- **不改 e2e bench 路径**（`_bench`/scenarios 不动）。
- **不建独立 `z42.bench` 包**（roadmap B1,独立事项）。
- **不改 z42b / z42.test**（`--format json` 已就绪,本变更纯消费端）。

## Open Questions

- [ ] baseline 存本地文件即可(优化者自管),还是也要 `bench-update.yml` 持久化到 `bench-baselines`
      分支?→ 本变更先只做本地文件;CI 持久化留后续(见 Out of Scope)。
