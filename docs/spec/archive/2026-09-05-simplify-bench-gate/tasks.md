# Tasks: 简化 bench 门禁 —— 阈值抬到噪声底之上 + 删掉不判红的开销

> 状态：🟢 已完成 | 创建：2026-09-05 | 完成：2026-09-05

**变更说明：** bench-pr 门禁的判红阈值（10%）画在实测噪声底（±13~16%）之下，每个 PR 期望假红约 2 条；
同时单次 job 执行 17~18 min，其中 26% 花在一个「informational、从不 fail」的 allocations 探针上。
本次：阈值抬到 0.25、micro 层降级为只打印不判红、删 allocations 探针、e2e 场景分层（`--tier gate`）+
`--mode` 按改动面收窄；并删掉早已不喂门禁的 `bench-update.yml` + `bench-baselines` 孤儿分支机制。

**原因：** 门禁「太久 + 数据不稳定 ⇒ 没被当回事」。数据不稳定不是运气问题，是判红规则本身站不住
（阈值低于噪声底 / 区间被低估三倍 / 87 次比较不做多重比较校正），实测最近 4 次失败逐条核对全是假红。

**文档影响：** `docs/book/src/dev/benchmarking.md`（判红语义 SoT，新增「噪声底与阈值」节）、
`bench/README.md`、`.github/workflows/README.md`、`docs/workflow/ci.md`、`docs/roadmap.md`
（`retire-baseline-branch` 标完成 + 新登记 `ab-resample-on-suspicion`）、`src/runtime/benches/README.md`。

## 实测依据（改动的全部依据，勿凭感觉重调）

逐步耗时：run 33930770961 / 33923733669。失败判红输出：33926859270 / 33882916075 / 33928998840 / 33884191241。

- 噪声底（perf-neutral PR 上比值对数标准差 ×1.96）：e2e **±13%**、micro **±16%**。旧阈值 10% 在其下。
- micro 声称的区间宽度中位 4.8%、最窄 0.25% ⇒ 相对真实噪声**低估约三倍**（`Bencher` stddev 是进程内
  样本方差，而 base/pr 是两个独立进程）。
- 耗时：e2e A/B 385s（36%）/ **allocations 探针 268~284s（26%，从不 fail）** / bootstrap 210~288s /
  micro 捕获 92s / **建 base 工具链仅 54~61s（5%，不是耗时主体）**。

## 阶段 1: 门禁可信（止血）
- [x] 1.1 `bench-pr.yml`：micro A/B 步骤改名 `Micro A/B (informational — never fails)`，命令尾 `|| true`
- [x] 1.2 `bench-pr.yml`：e2e 与 micro 的 `--threshold-time` 0.10 → 0.25
- [x] 1.3 `bench-pr.yml`：头部注释重写为三缺陷 → 三对策的因果说明，指向 book 的「噪声底与阈值」

## 阶段 2: 提速
- [x] 2.1 `bench-pr.yml`：删掉 allocations 探针整步（−284s）
- [x] 2.2 `xtask_bench.z42`：新增 `_benchScenarioTier` / `_benchTierWanted` / `_benchTierValid`；
      `_bench` 与 `_benchAb` 两处循环接入 tier 过滤（跳过时打印，不静默）
- [x] 2.3 `xtask_cli.z42`：`bench` e2e parser 加 `--tier gate|full|all`（默认 all，本地行为不变）
- [x] 2.4 `bench/scenarios/*.z42` 全 11 个加 `// tier:` 声明（gate 6 / full 5，各带一句理由）
- [x] 2.5 `bench-pr.yml`：e2e 传 `--tier gate`；`--mode` 按 `git diff -- src/runtime` 收窄（jit / both）

## 阶段 3: 清理数据机制
- [x] 3.1 删 `.github/workflows/bench-update.yml`
- [x] 3.2 删 `bench/baselines/.gitkeep` 目录 + `.gitignore` 的 `bench/baselines/*.json` 条目
- [x] 3.3 `xtask_bench.z42`：`--diff` 去掉「自动挑 `bench/baselines/main-<os>.json`」默认值，
      改为必须显式 `--baseline <path>`；删随之无用的 `_osTag()`
- [x] 3.4 留档 `bench-baselines` 最后一份 JSON（`main@eaeed8d9` / 2026-09-04T23:53:58Z / 22 条结果）
- [ ] 3.5 **本 PR 合并后**再删远程 `bench-baselines` 分支（1249 条机器提交）——顺序不能反：
      合并前 main 上的 `bench-update.yml` 仍在，任何一次 push 都会把分支重建出来

## 阶段 4: 文档同步
- [x] 4.1 `docs/book/src/dev/benchmarking.md`：新增「噪声底与阈值」节（三缺陷 + 两张现场表 + 阈值依据）；
      决策表 3 行改写；三层 tier 表 micro 改「只打印不判红」；CI 接线节重写 + 耗时构成表；
      已知局限登记 `ab-resample-on-suspicion` 与「`full` 层当前只在本地跑」的取舍
- [x] 4.2 `bench/README.md`：职责表 / micro-vs-e2e 表 / 目录树 / 使用示例 / CI 集成节 / baseline 对比节
- [x] 4.3 `.github/workflows/README.md` 删 bench-update 行、改 bench-pr 描述
- [x] 4.4 `docs/workflow/ci.md` 的 paths-ignore 表去掉 bench-update
- [x] 4.5 `docs/roadmap.md`：`retire-baseline-branch` 标 ✅、新登记 `ab-resample-on-suspicion`
- [x] 4.6 `src/runtime/benches/README.md` 去掉 `bench/baselines/` 引用

## 阶段 5: 验证
- [x] 5.1 `bench --tier gate --mode jit` 实跑：恰好 6 个 gate 场景测量、5 个 full 打印跳过
- [x] 5.2 `bench --tier bogus` 报错退出 2；`bench -h` 显示 `--tier`
- [x] 5.3 `bench --ab-selftest` 通过（判红纯函数未改，回归保护）
- [x] 5.4 完整 `xtask test` 全绿
- [x] 5.5 `grep -rn "bench-baselines\|bench-update\|bench/baselines"` 在活文档中清零
      （`docs/spec/archive/` 与 roadmap 已完成里程碑行属历史记录，保留不改）

## 备注

- **本次未做（下一步）**：「可疑即复测」（`ab-resample-on-suspicion`）。它是把阈值从 0.25 收回 0.15、
  以及让 micro tier 重新硬门禁的**唯一前提**，已登记进 roadmap Deferred Backlog Index。
- **已知取舍**：`bench-update.yml` 删除后，`full` 层的 5 个场景暂无 CI 落点，只在本地
  `xtask bench`（默认 `--tier all`）跑。恢复定期全量应落在独立数据仓 / 定时 workflow，
  而不是回到「每次 push main 烧一个 job」。
- **纠正了一处早期误判**：曾以为「建 base 工具链」是耗时主体、准备按改动面三分。逐步耗时实测显示它
  只有 54~61s（5%），不值得先做——真正的大头是 e2e A/B（36%）与 allocations 探针（26%）。
