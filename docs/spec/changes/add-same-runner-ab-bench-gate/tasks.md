# Tasks: 同-runner A/B 对照门禁（Stage 1: e2e）

> 状态：🟡 进行中 | 创建：2026-08-26

## 进度概览
- [ ] 阶段 1: `bench --ab` 编排 + 判红（xtask）
- [ ] 阶段 2: CLI 接线 + 单测
- [ ] 阶段 3: bench-pr.yml 重构
- [ ] 阶段 4: 文档
- [ ] 阶段 5: 验证

## 阶段 1: xtask `bench --ab`
- [ ] 1.1 `xtask_bench.z42`：抽公共场景枚举/编译 helper（从 `_bench` 主循环提取，供 `_bench` 与 `_benchAb` 复用）
- [ ] 1.2 `xtask_bench.z42`：`_abOneScenario`——base/pr 各编 zbc + 一次 hyperfine 双命令 → 两侧 mean/stddev
- [ ] 1.3 `xtask_bench.z42`：`_abVerdict(baseObj, prObj, thr)` 纯函数——SEM 传播算 R_lower/R_upper + verdict（regression/overlap/faster/no-ci）
- [ ] 1.4 `xtask_bench.z42`：`_benchAb(ParseResult)`——枚举场景×mode，聚合 verdict，写 `bench/results/ab.json`，退出码 0/1/2

## 阶段 2: CLI + 单测
- [ ] 2.1 `xtask_cli.z42`：注册 `bench --ab` + 选项 `--base-vm`/`--base-libs`/`--base-driver`/`--threshold-time`，路由 `_benchAb`
- [ ] 2.2 `_abVerdict` 单测：真回归 / 噪声 overlap / faster / no-ci 四例（xtask 单测入口）

## 阶段 3: bench-pr.yml 重构
- [ ] 3.1 加第二个 `actions/checkout`（`path: base-src`, `ref: pull_request.base.sha`）
- [ ] 3.2 `src/runtime` 变更探测 → 决定是否全建 base_vm（复用优化）
- [ ] 3.3 对 base-src 跑 `ci-bootstrap` → base_vm/base_libs/base_driver
- [ ] 3.4 用 PR xtask 跑 `bench --ab --base-* …`（删 fetch-baseline / diff 两步）
- [ ] 3.5 `timeout-minutes` 30→45；上传 `bench/results/ab.json` 作 artifact

## 阶段 4: 文档
- [ ] 4.1 `docs/book/src/dev/benchmarking.md`：A/B 门禁机制页（同-runner 抵消 + SEM 有效性 + 判红伪代码 + mermaid）
- [ ] 4.2 `bench/README.md`：「CI 集成」节改写为 A/B 流程
- [ ] 4.3 `docs/roadmap.md`：Deferred 登记 Stage 2/3 + interleave/retire-baseline

## 阶段 5: 验证
- [ ] 5.1 `_abVerdict` 单测绿
- [ ] 5.2 本地 A/B 自验：origin/main 作 base、当前分支作 pr，`bench --ab --quick` 出 verdict + ab.json
- [ ] 5.3 `xtask test`（GREEN gate；bench 相关以 CI bench-pr 为最终权威）
- [ ] 5.4 spec scenarios 逐条覆盖 + 文档 doc-check
- [ ] 5.5 PR 上 bench-pr（新 A/B 通路）对自身 base 绿 = 门禁自证

## 备注
- 门禁改判红机制（跨-runner diff → 同-runner A/B），属 feat；bench-baselines/bench-update **保留不动**。
- micro tier A/B（缺陷 #2）= Stage 2 独立 PR，前置 Bencher mean/stddev；criterion（#6）= Stage 3。
- 关键待确认（6.5）：交错方式 / base 建法 + vm 复用 / 阈值+Z / timeout 提升。
