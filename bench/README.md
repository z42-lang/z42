# z42 Benchmarks

## 职责

跨编译器 / 运行时的性能基准基础设施。多层度量：

| 层 | 工具 | 位置 | 状态 |
|----|------|------|------|
| Rust 微基准 | criterion | `src/runtime/benches/` | ✅ 同-runner A/B（criterion 原生 baseline 对照；仅 src/runtime 改动时）|
| z42 端到端 | hyperfine + 自建 harness | `bench/scenarios/` + `xtask bench` | ✅ |
| **z42 进程内微基准** | **`[Benchmark]` + `Std.Test.Bencher`（z42b 派发）** | **各 lib `bench/*_bench.z42`** | **✅ 同-runner A/B（`bench --micro-diff`）** |
| **PR 回归门禁（同-runner A/B）** | **`xtask bench --ab`（e2e）+ `--micro-diff`（micro）+ criterion baseline** | **`.github/workflows/bench-pr.yml`** | **✅** |
| 基线对比（本地 / 历史 dashboard） | `xtask bench --diff` | `bench/baselines/` | ✅（不再是 PR 门禁）|
| 主分支 baseline 持久化 | `.github/workflows/bench-update.yml` | — | ✅（降级 dashboard）|

> **C# 编译器吞吐（BenchmarkDotNet / `z42.Bench`）已随 C# bootstrap 移除**（2026-06-26）——
> 该 tier 不复存在；schema 不再有 `csharp-throughput`。

### 执行画像（mode × platform × capability）——add-exec-profile-matrix

每条基准结果都带一个 **profile**（schema v2），标明它是在什么执行画像下测的，避免不同模式 /
平台 / 能力的数字互相误比：

- **mode**（执行组合，非标量）：`{tiers, aot_pkgs}`。`tiers` = 活跃后端（`interp` 恒在，可 `+jit`）；
  `aot_pkgs` = 预编 AOT 的 zpkg 子集（**今天恒空**；非空的部分/全 AOT、AOT+JIT 混合 = `skipped-not-yet`，
  待 roadmap M9）。`mode_label`：`interp` / `jit` / （未来）`jit+aot[z42.core]`。
- **platform**：`{os, arch}`（arch 归一化 `x64`/`arm64`/`wasm`）。
- **caps**：由 `Std.Platform.Capabilities()` 在**被测 VM 二进制**下探测（`bench/probe/capabilities.z42`）
  ——`jit` / `native-interop` / `threads` 等真实能力，非静态推断。

`xtask bench --mode interp|jit|both` 扫描测量模式；`both` 每场景各测 interp 与 jit（各一条
profile 结果）。非可跑格子（如 interp-only VM 上请求 jit）显式跳过并打印原因。`--diff` 按
`(name, metric, mode_label@os/arch)` 匹配。

**jit/interp 加速比**（add-bench-speedup-cap-gating）：`--diff` 对同一 `(name, metric, platform)`
下同时测了 interp 与 jit 的场景，尾部派生一节 `Speedup (interp/jit, >1 = jit faster)`，逐条打印
`<name> [time@os/arch]  N.Nx`（= interp.value / jit.value）。纯派生展示，不入 schema、不触发回归。

**场景能力门控**（add-bench-speedup-cap-gating）：场景顶部可声明 `// requires-caps: <csv>`（如
`threads`）。harness 探到被测 VM 的 caps 后，缺任一所需 cap → **显式跳过该场景**（不崩），供
wasm/mobile 等无线程 VM 安全略过 `06_thread_scaling` 之类场景。

### micro vs e2e — 何时用哪个

| | **micro (`[Benchmark]`)** | **e2e (`bench/scenarios`)** |
|--|---------------------------|------------------------------|
| 粒度 | 单个操作（`String.Replace` / `SortedSet.Add` / `JsonValue.Parse`），ns 级 | 整程序 wall-clock（VM 启动 + stdlib 加载 + 执行），ms 级 |
| 用途 | 把回归**定位到具体函数**；守护 stdlib 热路径；量化单操作优化 | 捕获**全管线**回归（启动开销 / dispatch / 整体吞吐）|
| 运行 | `xtask bench stdlib <lib>`（本地/按需）| `xtask bench`（本地 + CI）|
| CI | ✅ **同-runner A/B**（`bench --micro-diff`：base 树 vs pr 树同机各测一遍）| ✅ 同-runner A/B（`bench --ab`）|

> **micro 如何进 CI**（extend-ab-bench-micro-criterion）：ns 级**单快照跨-runner** 比确实过敏，
> 故 micro 走**同-runner A/B**——PR 树 + base 树在同一 job 各跑一遍 `bench stdlib --json`，
> `bench --micro-diff` 比比值（机器因子抵消，配 Bencher mean/stddev 的 SEM）。criterion(Rust) tier
> 同理走其原生 `--save-baseline`/`--baseline`（仅 src/runtime 改动时）。判红语义 SoT 见
> `docs/book/src/dev/benchmarking.md`。

### 运行 micro-benchmarks（本地）

各 lib 的 `bench/*_bench.z42` 里的 `[Benchmark]` 方法由 z42b（z42.builder.zpkg）派发。
单独跑某个 lib 的基准（不跑其它 [Test]）：

```bash
# 1. 编译某 lib 的 bench 测试到 .zbc（test 工具链自动做；或手动 z42c --emit zbc）
# 2. 只跑 benchmark 方法（[Benchmark] 自带 Bencher 采样 + printSummary）：
z42 xtask.zpkg bench stdlib <lib>
#   → 每个 [Benchmark] 由 z42b 经 z42vm 运行，自报 warmup/samples/min/median/max
```

`bench_stats` 来自 `Bencher.printSummary(label)`，json 模式由 `Std.Test.Runner`
经 `BenchStats.parse` 捕获（rebuild-bench-structured-output）。人类可读的
`bench[label] min=… median=… max=…` 行走 pretty 模式。

### stdlib baseline：捕获 → 优化 → diff（本地/nightly，add-stdlib-bench-baseline）

micro-bench 的「优化前基线」可固化成 schema-v2 文件（每项带 profile），优化后 diff 量化收益：

```bash
# 1. 优化前：捕获聚合 baseline（各库 [Benchmark] 走 z42b --format json，median_ns 为主指标）
xtask bench stdlib --json bench/baselines/stdlib-before.json
#    → {schema_version:2, …, benchmarks:[{name:"<lib>.<label>", tier:"z42-micro",
#       metric:"time", value:<median_ns>, unit:"ns", ci_lower:<min>, ci_upper:<max>, samples,
#       profile:{mode,mode_label,platform,caps}}]}  ← profile 由被测 VM 探针填（mode 随 --mode）
#    单库：xtask bench stdlib z42.core --json /tmp/core-before.json

# 2. 改优化…

# 3. 优化后：再捕获 + 复用 e2e 的 diff（micro 噪声大 → 阈值放宽到 0.25）
z42 xtask.zpkg bench stdlib --json /tmp/after.json
z42 xtask.zpkg bench --diff --current /tmp/after.json \
    --baseline bench/baselines/stdlib-before.json --threshold-time 0.25
#    → ↑ 回归(exit 1) / ↓ 改进 / ≈ 持平 / (new)/(removed)
```

> **`bench --diff` 是本地/历史用法**（跨快照裸比值，宽阈值）；micro 的 **CI 硬门禁**走
> `bench --micro-diff`（同-runner A/B，见上「micro vs e2e」）——两者别混：`--diff` 面向本地优化
> 前后对比、`--micro-diff` 面向 PR 门禁（同机 base vs pr、SEM 判红）。
>
> **两种 [Benchmark] 签名均可**：form-1（自建 `Bencher`）与 form-2
> （`void f(Bencher b)`，z42c AST-desugar 成零参 wrapper——fix-benchmark-bencher-arg-trampoline
> 2026-07-20 移植回 z42c）都正常捕获入 baseline。

## 目录结构

```
bench/
├── README.md                  # 本文件
├── baseline-schema.json       # JSON Schema (Draft 2020-12) for results
├── scenarios/                 # 端到端场景 (.z42 → .zbc → 测时)
│   ├── 01_fibonacci.z42       # 递归 (~ms 量级)
│   ├── 02_math_loop.z42       # 整数循环 (~ms)
│   ├── 03_startup.z42         # 最小启动 baseline
│   ├── 04_c2_p1_arith_loop.z42 # 算术循环
│   ├── 05_polymorphic_dispatch.z42 # 多态派发 (PIC)
│   ├── 06_thread_scaling.z42  # 多线程 spawn/join (caps=threads)
│   ├── 07_string_heavy.z42    # 字符串搜索/拼接 (per-char VCall 分发税)
│   ├── 08_dict_heavy.z42      # 字典 insert/lookup (string-key hash + Equals 探测)
│   ├── 09_alloc_ctorless.z42  # 无构造函数类的分配紧循环 (守 cache-failed-name-resolution)
│   ├── 10_mono_vcall.z42      # 单态虚调用紧循环 (IC 恒命中，隔离 dispatch 成本)
│   └── 11_type_test_chain.z42 # 8 路 is 链 + 接口 is + as (类型判定 IsaCache，perf-vm-isa-cache)
├── probe/                     # capabilities.z42 — 被测 VM 能力探针（profile.caps 来源）
├── baselines/                 # main 分支的历史基线（gitignored，CI 上传到 gh-pages）
│   └── .gitkeep
└── results/                   # 当前 run 输出（gitignored）
    └── .gitkeep
```

## 使用

> 注：仓库无 `justfile`；下列是实际命令（旧 `just bench-*` 别名已不存在）。

```bash
# z42 端到端（hyperfine on .zbc）——默认测 jit，加 --mode 扫描
xtask bench                       # 全场景，jit
xtask bench --mode both           # 每场景各测 interp 与 jit（各一条 profile 结果）
xtask bench stdlib <lib>          # 某 lib 的 [Benchmark] 微基准（本地/按需）

# 快速 sanity（< 60s）
xtask bench --quick               # 只跑前 2 个场景，少 iter

# Rust criterion 微基准（未接入 xtask）
cargo bench --manifest-path src/runtime/Cargo.toml
```

## CI 集成（PR）

> **机制与判红语义的权威（SoT）见 book 页 [性能基准与回归门禁](../docs/book/src/dev/benchmarking.md)。**
> 本节只是操作速查。

PR 触碰性能敏感路径（`src/runtime` / `src/libraries` / `src/compiler` / `bench` /
`scripts/**/*.z42`）时，`.github/workflows/bench-pr.yml` **自动跑同-runner A/B 回归门禁并在真回归时 fail**
（add-same-runner-ab-bench-gate，取代旧的跨-runner baseline diff）：

1. checkout PR + checkout `base.sha`（`path: base-src`）
2. bootstrap PR 工具链；再在**同一 runner** 用 PR z42c 编 base 源建 base 工具链
   （`src/runtime` 无变更时 base_vm 复用 pr_vm）
3. `xtask bench --ab --mode both --threshold-time 0.10 --base-vm/-libs/-driver …`：每场景×mode 在
   base/pr 两套工具链下**同机相邻**编译+测量（hyperfine 双命令），比值 95% 下界 `R_lower > 1+thr` →
   回归 → exit 1 → **workflow fail**；结果落 `bench/results/ab.json` 并上传 artifact
4. 另跑一个 informational allocations 探针（确定性指标，只打印不 fail）

判红准则（`R_lower > 1+thr`，SEM 误差传播）与「为什么同机比才有效」见 book 页。纯文档 PR 因路径过滤自动跳过。
本地自验判红纯函数：`z42 xtask.zpkg bench --ab-selftest`（4 例，不跑 z42vm 场景）。

**历史 dashboard（不再喂门禁）**：每次 push 到 main，`bench-update.yml` 仍自动跑全量 e2e 并把结果提交到
`bench-baselines` 分支，作趋势记录 / 本地 `--diff` 对比源：

```
bench-baselines/
├── README.md
└── baselines/
    └── e2e-ubuntu-latest.json   # auto-updated by bench-update.yml
```

手动获取最新 main baseline：

```bash
git fetch origin bench-baselines:bench-baselines
git show bench-baselines:baselines/e2e-ubuntu-latest.json > /tmp/main.json
z42 xtask.zpkg bench --diff --baseline /tmp/main.json
```

## 与 baseline 对比

```bash
# 1. 把当前结果保存为 baseline（首次或重置）
cp bench/results/e2e.json bench/baselines/main-darwin-arm64.json

# 2. 后续跑 bench 后对比
z42 xtask.zpkg bench
z42 xtask.zpkg bench --diff                              # 自动选 main-<os>.json
z42 xtask.zpkg bench --diff --baseline bench/baselines/main-x.json   # 显式 baseline
```

退化判定（**区间感知**，add-interval-aware-bench-gate 2026-08-24；完整语义与数据流见 book 页
[性能基准与回归门禁](../docs/book/src/dev/benchmarking.md)）：一次回归必须**同时**满足
「均值超阈值」**且**「置信区间与 baseline 分离」。区间重叠 = 两次测量统计上不可区分 = 噪声，
一律**不**判回归——这是压掉共享 runner 假红、让门禁重新可信的关键。

- **回归**（`↑`，exit 1）⟺ `delta > thr` **且** `cur.ci_lower > base.ci_upper`
  （时间阈值默认 5% / `--threshold-time`，内存 10% / `--threshold-memory`）
- **改进**（`↓`，不 fail）⟺ `delta < -thr` **且** `cur.ci_upper < base.ci_lower`
- **区间重叠**（含均值超阈值但区间重叠）→ `≈` 并标 `(overlap)`，**不** fail
- **缺置信区间**（任一侧 `ci_lower`/`ci_upper` 缺失或 null）→ 回落裸均值判定，标 `(no-ci)`

> 判定用的是结果 JSON 里已有的 `ci_lower`/`ci_upper`（e2e 由 hyperfine 产、micro 用 min/max
> 充当区间），不采集新数据。区间越宽（噪声越大）门禁越保守，正合共享 runner 场景。

输出示例（`↑` 是真回归；+11% 那条均值超阈值但区间重叠 → 判噪声不 fail）：

```
  01_fibonacci [time jit@linux/x64]  90 ms → 130 ms  ↑ 44.4%
  02_math_loop [time jit@linux/x64]  50 ms → 100 ms  ≈ 11.1% (overlap)

❌ 1 regression(s) — delta over threshold AND CI-separated (out of 2 benchmarks)
```

判定逻辑的三态回归 fixtures 在 `bench/testdata/`（overlap→exit 0 / regress→exit 1 / improve→exit 0）。

## 输出格式

`bench/results/e2e.json` 与未来的 baseline 文件都遵循 [baseline-schema.json](baseline-schema.json)：

```json
{
  "schema_version": 2,
  "commit": "9dde4ec",
  "branch": "main",
  "z42vm_version": "0.4.0",
  "timestamp": "2026-07-31T12:00:00Z",
  "benchmarks": [
    {
      "name": "01_fibonacci",
      "tier": "z42-e2e",
      "metric": "time",
      "value": 32.4,
      "unit": "ms",
      "ci_lower": 31.8,
      "ci_upper": 33.1,
      "samples": 10,
      "profile": {
        "mode": { "tiers": ["interp", "jit"], "aot_pkgs": [] },
        "mode_label": "jit",
        "platform": { "os": "linux", "arch": "x64" },
        "caps": ["jit", "native-interop", "threads"]
      }
    }
  ]
}
```

## 添加新 scenario

1. 在 `bench/scenarios/` 加 `<NN>_<name>.z42`
2. 顶部注释说明 workload 与预期输出
3. 用 `Console.WriteLine` 打印一个稳定结果（便于验证编译器输出未漂移）
4. workload 大小让单次运行时间 ≥ 50ms（避免 hyperfine 抖动）

## 设计约定

- 不在 bench 里 IO 文件 / 网络（避免抖动）
- bench/baselines/ 目录用 .gitkeep 占位；实际 baselines 由 CI 上传到独立位置（P1.D）
- 度量单位统一：时间用 ms（hyperfine 输出 s 后转换），内存用 KB
- diff 阈值默认 5%（时间） / 10%（内存）
