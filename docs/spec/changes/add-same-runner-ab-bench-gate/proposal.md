# Proposal: 同-runner A/B 对照门禁（让 bench 门禁真能抓回归）—— Stage 1: e2e

## Why

当前 bench 门禁**近乎失明**（false green）。根因链：

1. **门禁比的是「跨-runner」两台机器**：[bench-pr.yml](../../../../.github/workflows/bench-pr.yml) 在 PR runner 上跑 e2e，
   却拿 `bench-baselines` 分支里**另一台 runner** 昨天的快照做 baseline diff。两台机器整体速度差
   ±26–60%（between-run 系统性噪声），远大于 10% 阈值。
2. **P0 为压掉这种跨-runner 假红，把区间设成 min/max（极宽）** → 区间几乎总重叠 → 判红几乎永不触发。
   换来「不假红」，代价是「基本抓不到真回归」。
3. 任何 within-run 统计（min/max、mean±stddev、SEM、百分位）都**无法**凭单次运行区分「真回归」与
   「跨-runner 偏移」——病根是 between-run，不在算法。

**根治 = 同-runner A/B 对照**：在**同一个 PR job、同一台 runner** 上，同时基于 **base（PR 的 merge base）**
和 **PR** 各建一套工具链，对每个场景用 hyperfine **交错**跑两条命令、比比值。两次测量共享同一台机器 →
跨-runner 系统性偏移**精确抵消**。confound 消除后，`mean ± SEM` 置信区间**才第一次统计有效** → 门禁得以
**收紧**：真 10% 回归 → 比值 CI 下界 > 阈值 → 判红；within-run 抖动 → 比值 CI 跨过 1 → 放过。

> 这解决缺陷清单 #1（门禁失明）与 #3（无 between-run 数据）的 e2e 部分。micro tier（#2）、criterion（#6）
> 作为后续 PR（Stage 2/3）。

## What Changes

- **xtask `bench --ab`（新子模式）**：给定 base 与 PR 两套 `(vm, libs, driver)`，对每个场景各用对应 z42c
  编成 zbc，再用 `hyperfine 'base_vm base.zbc …' 'pr_vm pr.zbc …'`（同一 invocation，交错采样）得到两条
  mean/stddev + 比值；按 A/B 判红逻辑输出每场景 verdict + 退出码（0 无回归 / 1 回归 / 2 工具错）。
- **A/B 判红（新，取代跨-runner min/max diff）**：回归 ⟺ `ratio = pr.mean/base.mean > 1+thr` **AND**
  比值显著（比值 95% 下界 > 1+thr，下界由两侧 SEM 传播）。区间跨 1+thr → 非回归（overlap）。
- **bench-pr.yml 重构**：在同 job checkout+bootstrap base ref → 建 base 工具链；再跑 `bench --ab`
  对照 PR（当前已建）。**不再** fetch bench-baselines 分支做门禁。
- **bench-baselines 分支保留**（bench-update.yml 不动）：继续每日发布单快照，供**历史 dashboard**（信息性），
  但**不再是门禁 source**。
- 文档：book 机制页记录 A/B 门禁原理（为何同-runner 抵消 + SEM 何时有效）+ 判红伪代码；roadmap 更新；
  bench/README.md「CI 集成」节改为 A/B 流程。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `scripts/xtask_bench.z42` | MODIFY | 新增 `bench --ab` 编排 + A/B 判红（`_benchAb`/`_abVerdict`）；复用 `_benchObj` 采集 mean/stddev |
| `scripts/xtask_cli.z42` | MODIFY | 注册 `--ab` 及其选项（`--base-vm`/`--base-libs`/`--base-driver`/`--threshold-time`）路由到 `_benchAb` |
| `.github/workflows/bench-pr.yml` | MODIFY | 同 job 建 base 工具链 + 跑 `bench --ab`；删 fetch-baseline/diff 两步 |
| `bench/README.md` | MODIFY | 「CI 集成」节改写为同-runner A/B 门禁流程 |
| `docs/book/src/dev/benchmarking.md` | MODIFY | A/B 门禁机制页：同-runner 抵消原理 + SEM 有效性 + 判红伪代码 + 数据流 mermaid |
| `docs/roadmap.md` | MODIFY | Deferred Backlog Index：micro A/B（Stage 2）/ criterion 接线（Stage 3）登记 |
| `scripts/test/xtask_test_lib.z42` | MODIFY | 若 `bench --ab` 需单测入口（A/B 判红纯函数 verdict 测例） |

**只读引用**：

- `.github/actions/ci-bootstrap/*` — 复用其建 base 工具链（不改，仅在 workflow 里二次调用）
- `bench-update.yml` — 确认保留、不改
- `src/libraries/z42.core/src/Math.z42` — `Sqrt`（SEM 传播）
- `docs/spec/archive/2026-08-24-add-interval-aware-bench-gate/` — P0 判红语义参照

## Out of Scope

- **micro/stdlib tier 的 A/B**（缺陷 #2）→ Stage 2 独立 PR（需 Bencher 加 mean/stddev，即原方向 A 工作，
  在 A/B confound 消除后才有意义）。
- **criterion（Rust）接线**（#6）→ Stage 3。
- **e2e 内存指标 / blackBox / 离群百分位**（#4/#7/#8）→ 不在本 change。
- 不改 bench-update.yml / bench-baselines 分支产出（保留作历史 dashboard）。

## Open Questions

- [ ] A/B 交错实现：hyperfine 双命令单 invocation（交错，最佳）vs 顺序两次 + `--diff`（简单）—— design 推荐前者。
- [ ] CI 建 base 成本：`src/runtime` 未变时是否复用同一 z42vm（省一次 Rust release 建）—— design 推荐「未变即复用」。
- [ ] base ref 取 `pull_request.base.sha`（PR 分叉点）确认。
- [ ] 判红阈值/显著性：`ratio 下界 > 1+thr`，thr 默认沿用 0.10，置信度 95%（z=1.96）确认。
