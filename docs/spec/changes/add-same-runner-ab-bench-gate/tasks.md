# Tasks: 同-runner A/B 对照门禁（Stage 1: e2e）

> 状态：🟡 进行中 | 创建：2026-08-26

## 进度概览
- [x] 阶段 1: `bench --ab` 编排 + 判红（xtask）
- [x] 阶段 2: CLI 接线 + 单测
- [x] 阶段 3: bench-pr.yml 重构
- [x] 阶段 4: 文档
- [~] 阶段 5: 验证（本地部分绿；e2e/GREEN 交 CI bench-pr 权威）

## 阶段 1: xtask `bench --ab`
- [~] 1.1 抽公共场景枚举/编译 helper：**改为不动 `_bench`**（避免动已绿 e2e 路径），`_benchAb` 复刻主循环 + 抽 `_abEmitZbc`/`_abOneScenario` 保函数 <60 行
- [x] 1.2 `_abOneScenario`——base/pr 各编 zbc + 一次 hyperfine 双命令（`env` 前缀带各自 libs）→ 两侧 mean/stddev
- [x] 1.3 `_abVerdict(...)` 纯函数——SEM 传播算 R_lower/R_upper + verdict（regression/overlap/faster/no-ci）；参数改原始 double（可单测）
- [x] 1.4 `_benchAb(ParseResult)`——枚举场景×mode，聚合 verdict，写 `bench/results/ab.json`，退出码 0/1/2

## 阶段 2: CLI + 单测
- [x] 2.1 `xtask_cli.z42`：注册 `bench --ab` + `--base-vm`/`--base-libs`/`--base-driver`（`--threshold-time` 复用现有），路由 `_benchAb`
- [x] 2.2 `_abVerdict` 单测：`bench --ab-selftest`（真回归 / 噪声 overlap / faster / no-ci 四例），本机 4/4 绿

## 阶段 3: bench-pr.yml 重构
- [x] 3.1 加第二个 `actions/checkout`（`path: base-src`, `ref: pull_request.base.sha`）
- [x] 3.2 `git diff base..pr -- src/runtime` 探测 → base_vm 复用 pr_vm 或 cargo 建
- [~] 3.3 建 base 工具链：**修正为 PR z42c 编 base 源**（composite `cd toplevel` 无法指向 base-src；design D3 已更）→ base_vm/base_libs/base_driver
- [x] 3.4 用 PR xtask 跑 `bench --ab --base-* …`（删 fetch-baseline / diff 两步）
- [x] 3.5 `timeout-minutes` 30→45；上传 `bench/results/ab.json` 作 artifact

## 阶段 4: 文档
- [x] 4.1 `docs/book/src/dev/benchmarking.md`：A/B 门禁机制页（同-runner 抵消 + SEM 有效性 + 判红伪代码 + mermaid）
- [x] 4.2 `bench/README.md`：状态表 + 「CI 集成」节改写为 A/B 流程
- [x] 4.3 `docs/roadmap.md`：Deferred 登记 `ab-bench-micro`/`ab-bench-criterion`/`ab-interleave-per-run`/`retire-baseline-branch`

## 阶段 5: 验证
- [x] 5.1 `_abVerdict` 单测绿（`bench --ab-selftest` 4/4）
- [x] 5.2 novel 机制本地验：env 前缀 + hyperfine 双命令产 results[0]=base/[1]=pr（各 mean/stddev），rc=0
- [~] 5.3 `xtask test` GREEN：本机 z42vm 退出期挂起 → 以 CI bench-pr + build-and-test 为权威
- [x] 5.4 spec scenarios 逐条覆盖 + 文档 doc-check
- [ ] 5.5 PR 上 bench-pr（新 A/B 通路）对自身 base 绿 = 门禁自证（**待开 PR**）

## 备注
- 门禁改判红机制（跨-runner diff → 同-runner A/B），属 feat；bench-baselines/bench-update **保留不动**。
- micro tier A/B（缺陷 #2）= Stage 2 独立 PR，前置 Bencher mean/stddev；criterion（#6）= Stage 3。
- 关键待确认（6.5）：交错方式 / base 建法 + vm 复用 / 阈值+Z / timeout 提升。
