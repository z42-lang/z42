# Spec: 区间感知的 bench 回归门禁

## MODIFIED Requirements

### Requirement: bench --diff 回归判定

**Before:** 回归 ⟺ `(cur - base) / base > threshold`（裸均值比值，无视置信区间）。

**After:** 回归 ⟺ 均值超阈值 **且** 当前置信区间与 baseline 置信区间分离；区间重叠一律非回归。
任一侧缺置信区间时回落 Before 的裸均值判定。

#### Scenario: 区间分离向上 + 均值超阈值 → 回归
- **WHEN** `delta > threshold-time` 且 `cur.ci_lower > base.ci_upper`
- **THEN** 标 `↑`，计入 regressions，`bench --diff` exit 1

#### Scenario: 区间重叠（噪声）即使均值超阈值 → 非回归
- **WHEN** `delta > threshold-time` 但 `cur.ci_lower ≤ base.ci_upper`（区间重叠）
- **THEN** 标 `≈` 并附 `(overlap)`，**不**计入 regressions；若无其它回归则 exit 0

#### Scenario: 区间分离向下 + 均值低于负阈值 → 改进
- **WHEN** `delta < -threshold-time` 且 `cur.ci_upper < base.ci_lower`
- **THEN** 标 `↓`（改进），不 fail

#### Scenario: 缺置信区间 → 回落裸均值
- **WHEN** 当前或 baseline 任一条缺 `ci_lower`/`ci_upper`（键缺失或为 null）
- **THEN** 按裸均值 `delta > threshold` 判回归，输出附 `(no-ci)` 标注

#### Scenario: 多 benchmark 含 1 条真回归
- **WHEN** 一组 benchmark 中恰有 1 条满足「分离向上 + 超阈值」
- **THEN** 该条标 `↑`，`bench --diff` exit 1，其余照常打印且不误判

#### Scenario: 内存指标同样走区间判定
- **WHEN** metric = memory（或 unit ∈ {bytes,KB,MB}）且带置信区间
- **THEN** 用 `threshold-memory` + 同一区间分离/重叠规则判定

## Pipeline Steps

不涉及编译器 pipeline（纯 xtask 脚本行为变更）。受影响组件：
- [x] xtask `bench --diff`（`scripts/xtask_bench.z42`）
