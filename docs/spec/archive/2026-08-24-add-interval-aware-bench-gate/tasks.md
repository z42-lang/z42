# Tasks: 区间感知的 bench 回归门禁

> 状态：🟢 已完成 | 创建：2026-08-24 | 完成：2026-08-24

## 进度概览
- [x] 阶段 1: 判定辅助 + 主循环改造
- [x] 阶段 2: fixtures + 三态验证
- [x] 阶段 3: 文档同步 + GREEN

## 阶段 1: 代码
- [x] 1.1 `scripts/xtask_bench.z42`：新增 `_findBenchObj`（返回整个 benchmark 对象或 null）
- [x] 1.2 新增 `_hasCi(JsonValue)`（两个区间字段均存在且非 null）
- [x] 1.3 改 `_benchDiff` 主循环：用 `_findBenchObj` 取 baseObj，读区间，按 Decision 1/2 判定
- [x] 1.4 输出行追加 `(overlap)` / `(no-ci)` 标注；打印条件改 `!quiet || sym == "↑"`
- [x] 1.5 更新 `_benchDiff` 头注释 + threshold 提示行说明「区间感知」

## 阶段 2: fixtures + 验证
- [x] 2.1 `bench/testdata/baseline.json`（schema v2，带 ci_lower/ci_upper）
- [x] 2.2 `bench/testdata/current-overlap.json`（均值 +11% 但区间大幅重叠）
- [x] 2.3 `bench/testdata/current-regress.json`（区间分离向上 + 超阈值）
- [x] 2.4 `bench/testdata/current-improve.json`（区间分离向下）
- [x] 2.5 建 xtask.zpkg，三态验证 exit code：overlap→0 / regress→1 / improve→0 ✅ 全通过
- [x] 2.6 手验一次 `(no-ci)` 回落路径 ✅ 去 CI 的 +11% 数据 → 裸均值 ↑ 判红标 (no-ci)

## 阶段 3: 文档 + GREEN
- [x] 3.1 `bench/README.md` diff/门禁节写清新判定语义（顺带修 stale 描述）
- [x] 3.2 `xtask test` 全绿（本改动不触碰编译/测试腿）
- [x] 3.3 spec scenarios 逐条覆盖确认（6/6：分离↑/重叠≈/分离↓/no-ci 回落/多条含1回归/memory 同判定）

## 备注
- 决策 A（fixtures 不进 GREEN gate）/ 决策 B（P0 只改 README，book 页留 P1）已由 User 阶段 6.5 裁决。
- WARN `Build.ProjectHooks` cross-zpkg fixup 是 pre-existing VM 元数据警告，与本改动无关。
