# 性能基准与回归门禁（benchmark / bench gate）

> **页型**: 机制页 ｜ **状态**: ✅ 已实现 ｜ **代码**: `scripts/xtask_bench.z42` · `bench/` · `.github/workflows/bench-pr.yml` · `bench-update.yml`
> **相关**: [xtask](xtask.md) · [测试门禁](test-gate.md) ｜ **对齐**: 2026-08-24

## 概述

benchmark 基础设施回答一个问题：**这次改动让 z42 变慢了吗？** 它由三部分组成：

1. **度量**——`xtask bench` 跑一组端到端场景（`bench/scenarios/*.z42`），每条产出带
   置信区间的结构化结果（schema v2）；`xtask bench stdlib <lib>` 跑库内 `[Benchmark]` 微基准。
2. **持久化**——每次 push 到 main，`bench-update.yml` 把全量 e2e 结果提交到孤儿分支
   `bench-baselines`，作为"main 当前性能"的活基线。
3. **门禁**——PR 触碰性能敏感路径时，`bench-pr.yml` 跑同一组 e2e、拉取 `bench-baselines`
   的基线、用 `xtask bench --diff` 做**区间感知**回归判定，判红即 fail workflow。

> **本页是 bench 判红语义的权威（SoT）。** `bench/README.md` 是操作速查（怎么跑命令），
> 判红规则、数据流、为什么这样设计以此页为准。改门禁语义 → 改这里。

## 设计目标与约束

- **门禁必须可信**：共享 CI runner 的 wall-clock 噪声可达 ±60%（见
  [测试门禁](test-gate.md) 与 `docs/design/testing`）。一个只看均值比值的门禁在这种噪声下
  会持续假红——开发者学会无视它，门禁形同虚设。**核心约束：宁可漏报也不能假报**，让红=真回归。
- **不采集额外数据**：判定复用结果 JSON 里 hyperfine / Bencher 已产出的 `ci_lower`/`ci_upper`，
  不为门禁跑第二遍。区间越宽（噪声越大）门禁越保守——正合共享 runner。
- **画像隔离**：interp 与 jit、不同 os/arch 的数字**从不互比**。每条结果带一个
  `profile`，diff 按 `(name, metric, mode_label@os/arch)` 精确匹配。
- **micro 与 e2e 分工**：ns 级微基准对噪声过敏，**不进 CI 门禁**，只做本地/nightly 的稳定硬件对比；
  CI 门禁只守粗粒度 e2e（ms 级）。

## 方案与决策

| 决策 | 选择 | 理由 |
|------|------|------|
| 判红准则 | **区间分离 AND 均值超阈值**（双条件）| 单看均值比值在共享 runner 噪声下假红率极高；要求当前 CI 与基线 CI 不重叠，把"统计上不可区分"的抖动挡在门外 |
| 区间来源 | 复用结果 JSON 已有的 `ci_lower`/`ci_upper` | e2e 由 hyperfine 的 min/max 产、micro 用采样 min/max 充当；不采集新数据，零额外成本 |
| 缺区间回落 | 任一侧缺 `ci_lower`/`ci_upper` → 裸均值比值门禁，标 `(no-ci)` | 老格式 / 外部结果无 CI 时仍可判，只是回到宽松语义，显式标注让读者知道判据降级了 |
| 阈值 | 时间 5%（`--threshold-time`；CI 用 0.10）/ 内存 10% | 时间默认严、CI 放宽到 10% 匹配 0.2.3 退出标准；内存指标暂为 informational |
| 画像键 | `(name, metric, mode_label@os/arch)` | interp/jit、跨平台结果隔离，杜绝 interp 的数字被拿去比 jit 基线 |
| micro 进 CI | 否 | ns 级测量对共享 runner 噪声过敏，假阳性淹没信号；留给本地/nightly 稳定硬件 |
| 基线存放 | 孤儿分支 `bench-baselines`，非主仓库树 | 结果 JSON 随 push 每日覆盖，进主分支历史会污染 diff；孤儿分支隔离、PR 侧按需 fetch |

## 三层度量 tier

| tier | 工具 | 位置 | 粒度 | 进 CI 门禁 |
|------|------|------|------|-----------|
| **z42 e2e** | hyperfine + 自建 harness | `bench/scenarios/` + `xtask bench` | 整程序 wall-clock（VM 启动 + stdlib 加载 + 执行），ms 级 | ✅ `bench-pr.yml` 硬门禁 |
| **z42 micro** | `[Benchmark]` + `Std.Test.Bencher`（z42b 派发）| 各 lib `bench/*_bench.z42` | 单操作（`String.Replace` / `SortedSet.Add` …），ns 级 | ❌ 本地/nightly 对比 |
| **Rust micro** | criterion | `src/runtime/benches/` | VM 内部热路径（GC cycle / smoke），未接入 xtask | ❌ `cargo bench` 直跑 |

**e2e** 捕获全管线回归（启动开销 / dispatch / 整体吞吐），是门禁守护面；**micro** 把回归
定位到具体函数、守 stdlib 热路径，但 ns 级数字只在稳定硬件上有意义，故不进门禁。

## 执行画像（profile，schema v2）

每条结果带一个 profile，标明它在什么执行组合下测出，避免误比：

- **mode**：`{tiers, aot_pkgs}`。`tiers` = 活跃后端（`interp` 恒在，可 `+jit`）；`aot_pkgs`
  = 预编 AOT 的 zpkg 子集（今天恒空，待 roadmap M9）。派生 `mode_label`：`interp` / `jit`。
- **platform**：`{os, arch}`（arch 归一化 `x64`/`arm64`/`wasm`）。
- **caps**：由 `Std.Platform.Capabilities()` 在**被测 VM 二进制**下探测
  （`bench/probe/capabilities.z42`）——`jit` / `native-interop` / `threads` 等真实能力。

`xtask bench --mode interp|jit|both`：`both` 每场景各测 interp 与 jit，产两条 profile 结果。
`--diff` 按 `mode_label@os/arch` 匹配，两个模式从不交叉比较。

## 机制

### 端到端数据流

```mermaid
flowchart TD
  subgraph push[push → main]
    U[bench-update.yml] -->|xtask bench --mode both| UR[e2e.json schema v2]
    UR -->|commit| BB[(bench-baselines 分支<br/>baselines/e2e-ubuntu-latest.json)]
  end
  subgraph pr[PR 触碰性能路径]
    P[bench-pr.yml] -->|xtask bench --mode both| PR[bench/results/e2e.json]
    BB -.->|git fetch| BL[/tmp/baseline.json]
    PR --> D{xtask bench --diff<br/>区间感知}
    BL --> D
    D -->|回归| F[❌ fail workflow]
    D -->|无回归/重叠/改进| K[✅ pass]
  end
```

基线从不进主仓库树：`bench/results/*` 与 `bench/baselines/*` 均 gitignored，真基线只活在
`bench-baselines` 孤儿分支，PR 侧 `git fetch origin bench-baselines` 按需取，不碰 worktree。

### 区间感知判红（核心）

对每条 current 结果，按画像键在 baseline 里找对应项（`_findBenchObj`），然后：

```
delta = (cur.value - base.value) / base.value       # 相对均值变化
thr   = 内存指标 ? threshold-memory : threshold-time  # 默认 5% / CI 10%

if 双方都有置信区间 (_hasCi):                          # 主路径
    if delta > thr   AND cur.ci_lower > base.ci_upper:  → ↑ 回归   (regressions++)
    elif delta < -thr AND cur.ci_upper < base.ci_lower:  → ↓ 改进
    elif |delta| > thr:                                  → ≈ (overlap)  # 均值超阈值但区间重叠 = 噪声，不判红
    else:                                                → ≈           # 均值也没超阈值
else:                                                # 回落：任一侧缺 CI
    标 (no-ci)
    if delta > thr:  → ↑ 回归   (regressions++)        # 裸均值比值（老语义）
    elif delta < -thr: → ↓ 改进

exit = regressions > 0 ? 1 : 0
```

判红的**充要条件是双条件同时成立**：`delta > thr`（均值确实超阈值）**且**
`cur.ci_lower > base.ci_upper`（当前置信区间整体高于基线置信区间，即两次测量统计上可区分）。

**区间重叠 ⇒ 一律不判红**，无论 delta 看起来多大。这是压掉共享 runner 假红的关键：一次
+11% 的均值抬升，若两个区间仍重叠，就是同一分布的两次抽样，判为 `(overlap)` 噪声、放行。

改进（`↓`）对称要求区间反向分离，但**从不 fail**，只作展示。

### 判定结果的四种标注

| 符号 / 标注 | 含义 | 是否 fail |
|-------------|------|-----------|
| `↑` | 真回归：均值超阈值 **且** 区间分离 | ✅ exit 1 |
| `↓` | 真改进：均值降超阈值 **且** 区间反向分离 | ✗ |
| `≈ (overlap)` | 均值超阈值但区间重叠 → 噪声 | ✗ |
| `≈` | 均值未超阈值 | ✗ |
| `(no-ci)` | 某侧缺置信区间 → 回落裸均值比值判定 | 视 delta |
| `(new)` / `(removed)` | 仅一侧存在该项 | ✗（informational）|

输出示例（`↑` 是真回归；+11% 那条均值超阈值但区间重叠 → 判噪声不 fail）：

```
  01_fibonacci [time jit@linux/x64]  90 ms → 130 ms  ↑ 44.4%
  02_math_loop [time jit@linux/x64]  50 ms → 100 ms  ≈ 11.1% (overlap)

❌ 1 regression(s) — delta over threshold AND CI-separated (out of 2 benchmarks)
```

### CI 门禁接线（bench-pr.yml）

触发路径刻意收窄（`src/runtime` / `src/libraries` / `src/compiler` / `bench` /
`scripts/**/*.z42` / 本 workflow），**纯文档 PR 跳过**。步骤：

1. bootstrap 工具链（nightly z42c 种子 → 当前源码 warm 自建，同 build-and-test 路径）
2. `xtask bench --mode both` 跑全量 e2e（interp + jit 各一条 profile 结果）
3. `git fetch origin bench-baselines` 取 `baselines/e2e-ubuntu-latest.json`；**分支不存在则跳过 diff 并 warning**（首次 bootstrap）
4. `xtask bench --diff --threshold-time 0.10` 判定，回归 → exit 1 → fail workflow
5. **allocations 探针（informational，永不 fail）**：alloc 计数是确定性的（不像 wall-time），
   打印每场景 × GC-mode 的分配数，为将来的确定性 alloc 回归门禁积累观测（决策 D4）

## 已知局限与后续

- **micro 未进 CI**：stdlib micro baseline 进 nightly（宽阈值门禁）是 Deferred
  `stdlib-bench-baseline-future-ci-nightly`；criterion tier 未接入 xtask（`cargo bench` 直跑）。
- **内存指标**：schema 有 `metric:"memory"` 位，但 e2e harness 暂不采集 RSS；`--threshold-memory`
  保留默认、内存 diff 为 informational。
- **allocations 门禁**：探针已打印、待观测 3–5 轮跨 GC-mode 稳定性后再转硬门禁。

## 参考

- 操作速查（跑哪条命令、加 scenario）：`bench/README.md`
- schema 定义：`bench/baseline-schema.json`（JSON Schema Draft 2020-12）
- 区间感知门禁提案 / 设计：`docs/spec/archive/2026-08-24-add-interval-aware-bench-gate/`
