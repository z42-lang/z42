# Tasks: bench 加速比 + 场景能力门控

> 状态：🟢 已完成 | 创建：2026-07-31 | 完成：2026-07-31（PR #87 合并 b3b23b64，已归档）
> 进度：阶段1-4 全落地。#1 加速比**端到端验证**（造 interp=100/jit=40 → `2.5x` 正确）；#2 门控
> 编译+逻辑验证（全 bench 路径受冷环境 z42c 种子漂移阻断，CI bench 腿覆盖）。
> 变更说明：补齐 exec-profile-matrix 漏做的 ① jit/interp 加速比派生指标（Decision 6）② 场景能力门控。
> 原因：双模式测量的 payoff（加速比）缺失；线程场景在无 threads VM 上会崩。
> 文档影响：bench/README + design/testing/exec-profile-matrix.md。
> 占用：`toolchain`。独立分支 `add-bench-speedup-cap-gating`（off origin/main）。

## 阶段 1: 助手
- [ ] 1.1 `xtask_exec_profile.z42`：`_epScenarioRequiredCaps(src)` 解析 `// requires-caps: a,b`；
      `_epCapsMissing(required[], have[])` 返回缺失项（空=齐）
- [ ] 1.2 单元验证：解析 + 子集判定（本地 smoke）

## 阶段 2: 加速比
- [ ] 2.1 `_benchDiff`：收集 current 里同 (name,metric,platform) 的 interp/jit 值，diff 尾部派生
      `interp/jit 加速比: N.Nx`（interp/jit）
- [ ] 2.2 验证：手造含 interp+jit 两条的 v2 JSON → `bench --diff` 打印加速比行

## 阶段 3: 门控
- [ ] 3.1 `_bench`：每场景读 requires-caps，缺 cap → 打印 `⊘ skip <name>: missing cap <x>` + continue
- [ ] 3.2 `06_thread_scaling.z42` 顶加 `// requires-caps: threads`
- [ ] 3.3 验证：threads 在 caps 内正常跑；模拟缺失（造一个假 requires-caps: nosuchcap）→ 跳过

## 阶段 4: 文档 + 验证归档
- [ ] 4.1 bench/README + exec-profile-matrix.md 记加速比 + requires-caps
- [ ] 4.2 xtask.zpkg 全量编译通过
- [ ] 4.3 归档 + push（GREEN 以 CI 为权威）

## 备注
- 纯 toolchain + bench + docs，不改 schema、不碰 VM。
