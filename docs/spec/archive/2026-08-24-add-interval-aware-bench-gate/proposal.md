# Proposal: 区间感知的 bench 回归门禁

## Why

`bench --diff`（`scripts/xtask_bench.z42` 的 `_benchDiff`）用**裸均值比值** `(cur - base) / base`
判回归，完全无视它自己已写进结果 JSON 的 `ci_lower` / `ci_upper`。baseline 与 PR 分别跑在两台
不同的 `ubuntu-latest` 物理机上，共享 runner 噪声可达 ±26–60%（见 memory
`bench-regression-04-cross-runner-noise`）。结果：大量假红 → 团队学会忽略门禁 → 门禁形同虚设。

不修：性能门禁不可信，真回归被淹没在假红里，等于没有门禁。

## What Changes

让回归判定变为**噪声感知**：一个 benchmark 只有在「均值超阈值」**且**「置信区间与 baseline
分离（不重叠）」时才判回归。区间重叠一律判「无变化」，无论均值差多少——因为重叠意味着两次测量
在统计上无法区分，差异是噪声。用**已存的数据**，不采集任何新东西。

- 回归（↑，fail）⟺ `delta > thr` 且 `cur.ci_lower > base.ci_upper`
- 改进（↓，不 fail）⟺ `delta < -thr` 且 `cur.ci_upper < base.ci_lower`
- 否则 → ≈（区间重叠 = 噪声，标 `(overlap)`）
- 任一侧缺 `ci_lower`/`ci_upper`（null/absent）→ 回落现行裸均值判定，标 `(no-ci)`

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `scripts/xtask_bench.z42` | MODIFY | `_benchDiff` 判定改区间感知；新增 `_findBenchObj` / `_hasCi` 辅助 |
| `bench/README.md` | MODIFY | diff/门禁节写清新判定语义，顺带修该节 stale 描述 |
| `bench/testdata/baseline.json` | NEW | 判定回归测试的 baseline fixture |
| `bench/testdata/current-overlap.json` | NEW | 区间重叠（噪声）场景 fixture |
| `bench/testdata/current-regress.json` | NEW | 区间分离向上（真回归）场景 fixture |
| `bench/testdata/current-improve.json` | NEW | 区间分离向下（真改进）场景 fixture |
| `docs/spec/changes/add-interval-aware-bench-gate/*` | NEW | 本变更规范 |

**只读引用**：

- `bench/baseline-schema.json` — 确认 fixtures 符合 schema v2
- `src/libraries/z42.json/src/JsonValue.z42` — 确认 null/缺键读取 API（`ContainsKey`/`IsNull`/`AsDouble`）

## Out of Scope

- 完整 `docs/book/` benchmark 机制页（属 P1 文档止血 change）
- 内存指标接线（e2e 只产 time；P2）
- micro 统计学增强（均值/方差/自适应采样；P2）
- 把 fixtures 验证接入 GREEN gate（scripts 层无单元测试腿；本 change 只本地+CI 验证）

## Open Questions

- 无（决策 A 测试策略、决策 B 文档落点已在阶段 6.5 由 User 裁决）
