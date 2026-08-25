# Spec: 同-runner A/B 对照门禁（e2e）

## ADDED Requirements

### Requirement: `bench --ab` 同-runner 对照

#### Scenario: 每场景各编两份 zbc 并同 invocation 对照
- **WHEN** `bench --ab --base-vm V --base-libs L --base-driver D` 处理一个场景
- **THEN** 用 base 工具链编出 `base.zbc`、用 PR（ambient）工具链编出 `pr.zbc`
- **AND** 用一次 `hyperfine 'base_vm base.zbc …' 'pr_vm pr.zbc …'` 采集两侧 mean/stddev

#### Scenario: caps 不足场景跳过（沿用现有）
- **WHEN** 场景声明的 `requires-caps` 在测量 VM 上缺失
- **THEN** 显式跳过该场景（不 crash），A/B 不将其计入 verdict

### Requirement: A/B 判红（比值置信下界）

#### Scenario: 真回归被判红
- **WHEN** `ratio = pr.mean/base.mean` 使 95% 下界 `R_lower > 1 + threshold`
- **THEN** 该场景标记回归，`bench --ab` 退出码为 1

#### Scenario: within-run 噪声不判红
- **WHEN** `ratio > 1 + threshold` 但 `R_lower ≤ 1 + threshold`（区间跨过阈值）
- **THEN** 标记 `(overlap)`，不计回归，退出码不因它变 1

#### Scenario: 显著提速为信息性
- **WHEN** `R_upper < 1 - threshold`
- **THEN** 标记 `(faster)`，退出码不受影响（仅信息）

#### Scenario: 缺 stddev 回落裸比值
- **WHEN** 任一侧缺 stddev / n（不应发生）
- **THEN** 回落 `ratio > 1 + threshold` 裸比值判定，标 `(no-ci)`

#### Scenario: 退出码语义
- **WHEN** 全部场景无回归
- **THEN** 退出码 0；任一场景回归 → 1；工具/编译错误 → 2

### Requirement: A/B 结果 artifact

#### Scenario: 落 ab.json
- **WHEN** `bench --ab` 完成
- **THEN** 写 `bench/results/ab.json`，每场景含 `{name, mode, base_mean, base_stddev, pr_mean, pr_stddev, ratio, r_lower, verdict}`

### Requirement: bench-pr 门禁走同-runner A/B

#### Scenario: 同 job 建 base 并对照
- **WHEN** bench-pr 触发
- **THEN** 在同一 job 内 checkout+bootstrap `pull_request.base.sha` 得 base 工具链
- **AND** 运行 `bench --ab` 对照 PR，回归即 fail job
- **AND** 不再 fetch `bench-baselines` 分支做门禁

#### Scenario: z42vm 复用优化
- **WHEN** 本 PR 未改动 `src/runtime`（`git diff --quiet base..pr -- src/runtime`）
- **THEN** base 复用 PR 的 z42vm，跳过 base 的 Rust release 构建

## MODIFIED Requirements

### Requirement: bench 门禁 source

**Before:** bench-pr fetch `bench-baselines` 分支的**另一台 runner** 快照做 diff（跨-runner，噪声 ±26–60%）。
**After:** bench-pr 在**同一台 runner** 建 base 工具链、A/B 对照（跨-runner 偏移抵消）；bench-baselines 分支
保留作历史 dashboard，不再喂门禁。

## Pipeline Steps

不涉及编译器 pipeline（工具脚本 + CI workflow + 文档）。
