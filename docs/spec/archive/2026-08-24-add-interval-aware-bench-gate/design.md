# Design: 区间感知的 bench 回归门禁

## Architecture

```
bench --diff
  ↓
_benchDiff(current.json, baseline.json)
  per (name, metric, mode-key)：
    baseObj = _findBenchObj(baseline, name, metric, mkey)   ← 改：拿整个对象，不只 value
    delta   = (curV - baseV) / baseV                         （保留：均值方向 + 幅度）
    haveCi  = _hasCi(cur) && _hasCi(baseObj)
    ┌ haveCi:
    │   分离向上 (cur.ci_lower > base.ci_upper) 且 delta>thr → ↑ 回归 (fail)
    │   分离向下 (cur.ci_upper < base.ci_lower) 且 delta<-thr → ↓ 改进
    │   否则（含均值超阈值但区间重叠）→ ≈ (overlap)
    └ !haveCi:
        回落裸均值：delta>thr → ↑ / delta<-thr → ↓，标 (no-ci)
```

现行代码只经 `_findBenchValue` 取 `value`（`xtask_bench.z42:165, 217`），拿不到区间。改为
`_findBenchObj` 取整个 benchmark JSON 对象，再从中读 `value` + `ci_lower` + `ci_upper`。
`_findBenchValue` 保留不动（`_printSpeedups` 仍用它，`xtask_bench.z42:247`）。

## Decisions

### Decision 1：回归判定 = 均值超阈值 AND 区间分离

**问题**：如何在不改变「阈值」直觉的前提下加噪声免疫？

**选项**：
- A — 纯区间不重叠即回归（丢弃阈值）：过敏感，微小但统计显著的抖动也 fail。
- B — 均值超阈值 **且** 区间分离：保留阈值语义（回归必须「够大」），叠加噪声门（必须「统计可区分」）。
- C — 用 t 检验 / 效应量：需样本原始数据，schema 只存聚合三点，做不到。

**决定**：选 B。回归 = `delta > thr && cur.ci_lower > base.ci_upper`。两个条件都满足才 fail——
既要足够大（超阈值），又要与 baseline 在统计上可区分（区间不重叠）。这是「用已存数据能做到的、
最贴合直觉的噪声免疫」。

### Decision 2：缺 CI 时回落裸均值（向后兼容）

**问题**：老 baseline / 部分结果可能没有 `ci_lower`/`ci_upper`（schema 允许 null）。

**决定**：任一侧缺 CI → 回落现行裸均值判定，输出标 `(no-ci)`。既不破坏老数据，也让「这条没走
区间判定」在输出里可见、诚实。e2e 与 micro 的现有 baseline 都带 CI，回落只在异常/老数据时触发。

### Decision 3（测试策略，User 阶段 6.5 裁决）：fixtures + 本地/CI 验证，不进 GREEN gate

scripts 层无 `[Test]` 单元测试腿。加 `bench/testdata/` 4 个小 fixtures（1 baseline + 3 current），
阶段 8 本地跑 `xtask bench --diff` 断言三态 exit code（overlap→0 / regress→1 / improve→0）。
证据留在 tasks.md。不接 GREEN gate（gate 无 scripts 测试腿）。

### Decision 4（文档落点，User 阶段 6.5 裁决）：P0 只改 bench/README.md

对外行为变更本应落 `docs/book/` 机制页，但 book 现无 bench 页。P0 只在 `bench/README.md`
（事实权威）的 diff/门禁节写清新判定；完整 book 机制页留给 P1 文档止血 change。

## Implementation Notes

- 安全读 CI：`_hasCi(b)` = `b.ContainsKey("ci_lower") && !b.Get("ci_lower").IsNull()`（对
  `ci_upper` 同）。`Get` 键不存在会抛、`AsDouble` 对 null 会抛，故必须先守卫
  （`JsonValue.z42:166, 188`）。
- `_findBenchObj` 未命中返回 `null`（z42 引用类型可空，参 `JsonBinder.z42:12`）。
- 输出行在末尾追加 `note`（`" (overlap)"` / `" (no-ci)"` / `""`）；打印条件由
  `!quiet || delta > thr` 改为 `!quiet || sym == "↑"`（只有真回归在 quiet 下仍打印）。
- 阈值默认不变（time 0.05 / mem 0.10）；CI 传 `--threshold-time 0.10` 不变。

## Testing Strategy

- fixtures 三态：`xtask bench --diff --current testdata/current-overlap.json --baseline
  testdata/baseline.json` → exit 0（标 overlap）；`current-regress` → exit 1；
  `current-improve` → exit 0。
- 回落：临时删一条 fixture 的 CI 字段 → 标 `(no-ci)` 且走裸均值（tasks 里手验一次即可）。
- GREEN：`xtask test` 全绿（本改动不触碰编译/测试腿，纯 xtask 脚本行为）。
