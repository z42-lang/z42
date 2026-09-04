# 性能基准与回归门禁（benchmark / bench gate）

> **页型**: 机制页 ｜ **状态**: ✅ 已实现 ｜ **代码**: `scripts/xtask_bench.z42` · `bench/` · `.github/workflows/bench-pr.yml` · `bench-update.yml`
> **相关**: [xtask](xtask.md) · [测试门禁](test-gate.md) ｜ **对齐**: 2026-08-24

## 概述

benchmark 基础设施回答一个问题：**这次改动让 z42 变慢了吗？** 它由三部分组成：

1. **度量**——`xtask bench` 跑一组端到端场景（`bench/scenarios/*.z42`），每条产出带
   置信区间的结构化结果（schema v2）；`xtask bench stdlib <lib>` 跑库内 `[Benchmark]` 微基准。
2. **持久化（历史 dashboard）**——每次 push 到 main，`bench-update.yml` 把全量 e2e 结果提交到孤儿分支
   `bench-baselines`，作为"main 历史性能"趋势记录。**它不再喂门禁**（见下）。
3. **门禁（同-runner A/B）**——PR 触碰性能敏感路径时，`bench-pr.yml` 在**同一台 runner** 上建 PR
   与 base（merge-base）两套工具链，`xtask bench --ab` 让每个场景在两套下**同机相邻**编译+测量，按
   比值 95% 下界判红。这取代了旧的「拉另一台 runner 的 baseline 快照做跨-runner diff」——那种比法
   被 ±26–60% 的 between-run 系统偏移主导，区间几乎总重叠、门禁近乎失明。

> **本页是 bench 判红语义的权威（SoT）。** `bench/README.md` 是操作速查（怎么跑命令），
> 判红规则、数据流、为什么这样设计以此页为准。改门禁语义 → 改这里。

## 设计目标与约束

- **门禁必须可信且能抓到真回归**：共享 CI runner 的 wall-clock 噪声可达 ±60%（见
  [测试门禁](test-gate.md) 与 `docs/design/testing`）。**但这噪声主要是 between-run（跨-runner /
  跨-job）系统偏移**——同一 runner 内 within-run 抖动小得多。旧门禁拿「另一台 runner 的 baseline
  快照」对比，被 between-run 偏移主导 → 区间总重叠 → 判红几乎不触发（既不假报、也**抓不到真回归**）。
  **核心约束：门禁既不能假报、又必须能抓真回归**——解法是把对比搬进**同一台 runner**（见下 A/B）。
- **同-runner 抵消（A/B 门禁的地基）**：base 与 pr 在同机、同 job、相邻数秒内测量 → 该机器的整体快慢
  因子 `k` 同时乘进两侧，`ratio = (k·t_pr)/(k·t_base) = t_pr/t_base`，`k` 约掉。剩下只有 within-run
  抖动，可由 SEM 量化 → 比值置信区间**才第一次统计有效**（跨-runner 比法下 SEM 不成立）。
- **不采集额外数据**：判定复用结果 JSON 里 hyperfine / Bencher 已产出的 `ci_lower`/`ci_upper`，
  不为门禁跑第二遍。区间越宽（噪声越大）门禁越保守——正合共享 runner。
- **画像隔离**：interp 与 jit、不同 os/arch 的数字**从不互比**。每条结果带一个
  `profile`，diff 按 `(name, metric, mode_label@os/arch)` 精确匹配。
- **micro 与 e2e 分工**：e2e（ms 级）守粗粒度全管线；micro（ns 级）守 stdlib 函数级与 VM 内部热路径。
  ns 级**单快照跨-runner** 比过敏，故 micro 也走**同-runner A/B**（base 树 vs pr 树同机各测一遍）才进门禁。

## 方案与决策

| 决策 | 选择 | 理由 |
|------|------|------|
| **PR 门禁比法** | **同-runner A/B**（base 与 pr 同机相邻测量，比比值）| 跨-runner baseline 快照被 ±26–60% between-run 偏移主导，门禁失明；同机测量让 `k` 约掉，才既不假报又能抓真回归 |
| **A/B 判红准则** | **比值 95% 下界 R_lower > 1+thr**（SEM 误差传播）| 同机抵消后 within-run SEM 才有效；对商做误差传播得比值置信区间，下界超阈值 = 有把握判回归 |
| **A/B 交错** | hyperfine 双命令单 invocation（`env Z42_LIBS=… vm …` 前缀）| 两侧相邻数秒、各带自己的 libs；逐次交错属过度工程 |
| 区间感知 diff（历史） | `bench --diff`：**区间分离 AND 均值超阈值** | 旧 PR 门禁准则（P0）；现降级为 `bench --diff` 命令 + 历史 dashboard 对比，不再是 PR 门禁 |
| 区间来源 | 复用结果 JSON 已有的 `ci_lower`/`ci_upper` | e2e 由 hyperfine 的 min/max 产、micro 用采样 min/max 充当；不采集新数据，零额外成本 |
| 缺区间回落 | 任一侧缺 `ci_lower`/`ci_upper` → 裸均值比值门禁，标 `(no-ci)` | 老格式 / 外部结果无 CI 时仍可判，只是回到宽松语义，显式标注让读者知道判据降级了 |
| 阈值 | 时间 5%（`--threshold-time`；CI 用 0.10）/ 内存 10% | 时间默认严、CI 放宽到 10% 匹配 0.2.3 退出标准；内存指标暂为 informational |
| 画像键 | `(name, metric, mode_label@os/arch)` | interp/jit、跨平台结果隔离，杜绝 interp 的数字被拿去比 jit 基线 |
| micro 进 CI | **是（同-runner A/B）** | 单快照跨-runner 比确实噪声过敏；但**同-runner** base-batch vs pr-batch 把机器因子约掉，配 Bencher mean/stddev 的 SEM → 门禁有效（`bench --micro-diff`） |
| 基线存放 | 孤儿分支 `bench-baselines`，非主仓库树 | 结果 JSON 随 push 每日覆盖，进主分支历史会污染 diff；孤儿分支隔离、PR 侧按需 fetch |

## 三层度量 tier

| tier | 工具 | 位置 | 粒度 | 进 CI 门禁 |
|------|------|------|------|-----------|
| **z42 e2e** | hyperfine + 自建 harness | `bench/scenarios/` + `xtask bench` | 整程序 wall-clock（VM 启动 + stdlib 加载 + 执行），ms 级 | ✅ `bench-pr.yml` 硬门禁 |
| **z42 micro** | `[Benchmark]` + `Std.Test.Bencher`（z42b 派发）| 各 lib `bench/*_bench.z42` | 单操作（`String.Replace` / `SortedSet.Add` …），ns 级 | ✅ 同-runner A/B（`bench --micro-diff`） |
| **Rust micro** | criterion | `src/runtime/benches/` | VM 内部热路径（GC cycle / smoke）| ✅ criterion 原生 A/B（仅 src/runtime 改动时） |

**e2e** 捕获全管线回归（启动开销 / dispatch / 整体吞吐），是门禁守护面；**micro** 把回归
定位到具体函数、守 stdlib 热路径——ns 级单快照跨-runner 比确实过敏，但**同-runner A/B**
（base 树 + pr 树同机各测一遍、比比值）把机器因子约掉，配 mean/stddev 的 SEM 后同样能进门禁。

### micro 同-runner A/B（`bench --micro-diff`，Part B）

micro `[Benchmark]` 在 z42b 里**进程内**跑，base 与 pr 的同名 stdlib zpkg 无法共载（不像 e2e 每场景一个
独立 z42vm 子进程）。故同-runner A/B 用**两个隔离的 `bench stdlib --json` 基线**实现：

1. **PR 树**跑 `bench stdlib --json micro-pr.json`（PR 工具链编+跑全 stdlib `[Benchmark]`）。
2. **base(merge-base) 树**跑同样命令 → `micro-base.json`（base 工具链，复用 CI「Build base toolchain」步已建
   的 base 编译器+stdlib，仅新建 base z42b）。
3. `bench --micro-diff --current micro-pr.json --baseline micro-base.json` 逐基准（按 `name` + 画像键配对）
   `_abVerdict`——与 e2e 共用同一判红纯函数。回归 ⟺ R_lower > 1+thr（CI 用 0.10）。

同机顺序测量 → 机器因子在比值抵消；采样越多（自适应）CI 越紧、门禁越敏。PR 新增/改名的基准在 base 无
对应 → 信息性 skip，不判红。

### Bencher 统计与自适应采样（`Std.Test.Bencher`）

micro tier 的每个 `[Benchmark]` 用 `Bencher` 采样，`printSummary` 产出一行契约：

```
bench[<label>] min=<n>ns median=<n>ns max=<n>ns mean=<n>ns stddev=<n>ns samples=<n>
```

- **mean / stddev**（extend-ab-bench-micro-criterion）：`mean=`/`stddev=` 是 SEM 传播的输入，供
  micro tier 的同-runner A/B 门禁（与 e2e 同套 `_abVerdict`；见上「micro 同-runner A/B」）。是**追加
  字段**——旧 summary line（无 mean/stddev）仍能被 `BenchStats.parse` 解析，缺失记 -1，下游按「无 CI」降级。
- **自适应采样**：默认构造 `new Bencher()` 现按 **~50ms 测量预算**自动选采样数 n（先跑 15 次 pilot
  估单次成本，再 clamp 到 [20, 2000]）——快基准多采样（CI 更紧）、慢基准少采样（墙钟有界）。
  显式 `new Bencher(warmup, samples)` 保持**固定** samples（无自适应），既有 tuned 基准行为不变。
- stddev 是**总体**方差（÷n）、四舍五入到 ns；亚-ns 离散 round 到 0 → 下游判「无 CI」→ 裸比值，
  安全不假红。

### criterion(Rust) 同-runner A/B（Part C）

Rust VM 内部热路径（`src/runtime/benches/gc_cycle_bench.rs`：cycle_heavy / shallow_tree / large_array）
用 **criterion 原生的同-runner baseline 对照**，不复用 z42 侧 `_abVerdict`——criterion 自带 outlier
检测 + bootstrap CI + 变化判定，比手搓 SEM 更权威：

1. **base(merge-base) 树**：`cargo bench --bench gc_cycle_bench -- --save-baseline ab-base`。
2. **PR 树**：`cargo bench --bench gc_cycle_bench -- --baseline ab-base`（criterion 报每个基准的均值变化 %）。
3. 共享 `CRITERION_HOME` 让两树的结果相遇；门禁读 `<bench>/change/estimates.json` 的 `mean.point_estimate`
   与 `confidence_interval.lower_bound`——**变化 > +10% 且 CI 下界 > 0**（criterion 确信真慢）→ 判红。

**`concurrent_*`（多线程）基准仅信息性、不判红**：criterion 的 `--baseline` 比的是**两次独立跑**（base 先存、
pr 再比），线程 GC 基准在共享 runner 上的**跑间线程调度噪声**很大（实测：一个 perf-中立的纯注释 PR，
concurrent 基准摆动 +6~33%，单线程基准却居 0 附近）。criterion 紧的**跑内** CI 抓不到这种跑间漂移，硬判会
假红。故 concurrent 基准打印但不 fail job；单线程 GC 基准照常硬门禁。

**仅当 PR 触碰 `src/runtime` 时跑**（`git diff --quiet base..HEAD -- src/runtime` 为真则跳过——VM 字节
相同、结果必不动）。`smoke_bench.rs` 是纯 Rust sanity（不碰 VM），保留作「criterion 装置能跑」自检、
**不纳入门禁**。

## 执行画像（profile，schema v2）

每条结果带一个 profile，标明它在什么执行组合下测出，避免误比：

- **mode**：`{tiers, aot_pkgs}`。`tiers` = 活跃后端（`interp` 恒在，可 `+jit`）；`aot_pkgs`
  = 预编 AOT 的 zpkg 子集（今天恒空，待 roadmap M9）。派生 `mode_label`：`interp` / `jit`。
- **platform**：`{os, arch}`（arch 归一化 `x64`/`arm64`/`wasm`）。
- **caps**：由 `Std.Platform.Capabilities()` 在**被测 VM 二进制**下探测
  （`bench/probe/capabilities.z42`）——`jit` / `native-interop` / `threads` 等真实能力。

`xtask bench --mode interp|jit|both`（含 `--ab`）：`both` 每场景各测 interp 与 jit，产两条结果。
A/B 门禁对**同一 scenario×mode** 在 base/pr 两套工具链间比较，interp 与 jit 各自比、从不交叉；
`--diff` 按 `mode_label@os/arch` 匹配基线，同样两模式隔离。

## 机制

### 同-runner A/B 判红（PR 主门禁，核心）

PR 门禁在**一台 runner** 上建两套工具链，同机测量、比比值：

```mermaid
flowchart TD
  subgraph pr[PR 触碰性能路径 — 同一台 runner]
    C1[checkout PR] --> BT[ci-bootstrap<br/>PR 工具链]
    C2[checkout base.sha<br/>path: base-src] --> BB[PR z42c 编 base 源<br/>→ base 工具链]
    BT --> AB{{每场景×mode}}
    BB --> AB
    AB -->|base_vm base.zbc<br/>+ pr_vm pr.zbc<br/>hyperfine 双命令同机相邻| M[base.mean/stddev<br/>pr.mean/stddev]
    M --> V{_abVerdict<br/>ratio=pr/base<br/>R_lower SEM 传播}
    V -->|R_lower > 1+thr| F[❌ fail workflow]
    V -->|overlap / faster| K[✅ pass]
  end
  subgraph dash[push → main：历史 dashboard，不喂门禁]
    U[bench-update.yml] -->|xtask bench| BL[(bench-baselines 分支)]
  end
```

**base 工具链怎么来**：ci-bootstrap 的 composite 头一步 `cd $(git rev-parse --show-toplevel)` 会锁回 PR
checkout，无法指向 base-src，故**不复用它建 base**；改用**已 bootstrap 的 PR z42c 直接编 base 源**（新
编译器编旧源恒成立，staged-bootstrap 纪律）：PR z42c 编 `base-src/src/compiler` → base z42c，再由 base
z42c 编 `base-src/src/libraries` → base stdlib。**只测 scenario 运行时**，故 base driver 由 PR z42c 生成
其字节码这点对测量零影响——它跑的仍是 base 的 codegen 逻辑，产 base 风格的 scenario .zbc。z42vm 复用：
`git diff base..pr -- src/runtime` 无变更时 base_vm = pr_vm，省最重的 cargo 建。

判红纯函数 `_abVerdict`（`scripts/xtask_bench.z42`，可 `bench --ab-selftest` 单测）：

```
SEM_b = stddev_b / sqrt(n_b);   SEM_p = stddev_p / sqrt(n_p)
ratio = mean_p / mean_b
relSE = sqrt( (SEM_p/mean_p)^2 + (SEM_b/mean_b)^2 )     # 商的误差传播
R_lower = ratio * (1 - Z*relSE);  R_upper = ratio * (1 + Z*relSE);  Z=1.96 (95%)

if R_lower > 1 + thr:   → ↑ regression   (fail, exit 1)   # 95% 有把握真实比值超阈值
elif R_upper < 1 - thr: → ↓ faster        (informational)
else:                   → ≈ overlap       (noise, 放行)
缺 stddev/n（n≤1 或 stddev≤0）→ 回落裸比值 ratio>1+thr，标 (no-ci)
```

- `thr` 默认 **0.10**（CI 用 0.10，沿用 0.2.3 退出标准）；`Z=1.96`。
- 结果落 `bench/results/ab.json`（`ab-v1` schema：每场景 base/pr mean·stddev、ratio、r_lower/r_upper、
  verdict），信息性 artifact，不复用 baseline-schema。
- **同机抵消让 within-run SEM 在此统计有效**——这是 P0 跨-runner 比法下不成立、A/B 下才成立的关键。

### 区间感知 diff（`bench --diff`；历史 dashboard 对比，非 PR 门禁）

> **P0 时期的 PR 门禁准则，现已退休为 `bench --diff` 命令 + 历史对比工具**（不再由 bench-pr 调用）。
> 基线仍活在 `bench-baselines` 孤儿分支（`bench/results/*`、`bench/baselines/*` gitignored），
> `bench-update.yml` 每 push 覆盖一次，供人工趋势对比 / 本地 `--diff`。判红语义如下：

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
`scripts/**/*.z42` / 本 workflow），**纯文档 PR 跳过**。`timeout-minutes: 60`（建两套工具链 + micro/criterion A/B）。步骤：

1. checkout PR + checkout `base.sha`（`path: base-src`，两者 `fetch-depth: 0`）
2. bootstrap PR 工具链（`ci-bootstrap`：nightly z42c 种子 → 当前源码 warm 自建）
3. **建 base 工具链**（同 runner）：`git diff base..pr -- src/runtime` 无变更 → base_vm=pr_vm，否则
   cargo 建 base z42vm；PR z42c 编 `base-src/src/compiler`→base z42c，base z42c 编 `base-src/src/libraries`→base stdlib
4. **e2e A/B**：`xtask bench --ab --mode both --threshold-time 0.10 --base-vm/-libs/-driver …`：每场景×mode
   在两套下同机编译+测量，`_abVerdict` 判红 → 回归 exit 1 → fail workflow；上传 `bench/results/ab.json`
5. **micro A/B**（Part B）：PR 树 + base 树各 `bench stdlib --json`（base 树复用 3 建的工具链、cp base_vm、
   仅新建 base z42b）→ `bench --micro-diff` 逐基准 `_abVerdict` 判红
6. **criterion A/B**（Part C，仅 `src/runtime` 改动时）：base 树 `--save-baseline ab-base`、PR 树
   `--baseline ab-base`（共享 `CRITERION_HOME`），读 `change/estimates.json` 判红（>10% 且 CI 分离）
7. **allocations 探针（informational，永不 fail）**：alloc 计数是确定性的（不像 wall-time），
   打印每场景 × GC-mode 的分配数，为将来的确定性 alloc 回归门禁积累观测（决策 D4）

> **格式-bump 边角（已知瞬态）**：PR 同时 bump zpkg 格式 **且**动 `src/runtime` 时，base driver 是 PR(新)
> 格式而 base_vm 是 base(旧)格式读不了 → 该 PR 当次 bench 可能红。与 bootstrap-seed 的格式-bump 瞬态一致，
> 随 nightly 自愈、不阻塞（格式-bump 时 perf 对照本就意义有限）。

### 启动类微小回归：先排除「布局彩票」（cache-failed-name-resolution，2026-09-04）

hello 启动只有 ~6.5 ms、以**冷代码**为主，对二进制布局极其敏感。实测过一个反直觉的对照：

> 在 `LazyLoader` 上加**一个从不读的 `usize` 字段**（其余全部保持 HEAD，`git diff` 只有 3 行），
> hello 启动的 **instructions retired 从 69.6 M 涨到 73.5 M（+5.7%）**，墙钟 +0.4 ms。

也就是说，一条**逻辑上零成本**的改动就能在启动上造出 ~6% 的「回归」。装箱、`#[inline(never)]`、
把整个 `LazyLoader` 挪到 `Box` 里（让 `VmCore` 大小不再随它变）都消不掉这个差值。

**因此判断「某改动是否真的拖慢了启动」必须做两件事**：

1. **看 `instructions retired`**（macOS: `/usr/bin/time -l`）而不是只看墙钟——它对代码布局免疫，
   跨运行的离散度只有 ~0.5%，能把「真多干活了」和「布局摆动」分开；
2. **做扰动对照组**：`HEAD + 一个死字段` 重编一份，测同一指标。对照组同样摆动 ⇒ 这个差值
   **不可归因于被测改动**。顺带对一遍 `--print-stats-on-exit` 的计数器（builtin_calls /
   jit_methods_compiled / allocations / …），逐项相同则可确认没有行为差异。

（cache-failed-name-resolution 差点因为这条被误判成 −6% 启动回归而砍掉一条 1.94× 的优化。）

## 已知局限与后续

- **micro/criterion tier 已进门禁**（Stage 2/3，extend-ab-bench-micro-criterion）：micro 用两个隔离
  `bench stdlib --json` 基线 + `bench --micro-diff`；criterion 用其原生 baseline 对照（仅 runtime 改动时）。
  `ab-bench-micro` / `ab-bench-criterion` 两个 Deferred 项**已落地**。
- **A/B 交错粒度**：hyperfine 双命令是「base 全跑→pr 全跑」于一 invocation（同机相邻，够抵消 between-run），
  非逐次交错；逐次交错抗 job 内漂移更强，Deferred `ab-interleave-per-run`。
- **内存指标**：schema 有 `metric:"memory"` 位，但 e2e harness 暂不采集 RSS；`--threshold-memory`
  保留默认、内存 diff 为 informational。
- **allocations 门禁**：探针已打印、待观测 3–5 轮跨 GC-mode 稳定性后再转硬门禁。

## 参考

- 操作速查（跑哪条命令、加 scenario）：`bench/README.md`
- schema 定义：`bench/baseline-schema.json`（JSON Schema Draft 2020-12）
- 同-runner A/B 门禁提案 / 设计：`docs/spec/changes/add-same-runner-ab-bench-gate/`
- 区间感知门禁（P0，已退休为 dashboard 对比）提案 / 设计：`docs/spec/archive/2026-08-24-add-interval-aware-bench-gate/`
