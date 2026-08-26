# Design: 同-runner A/B 对照门禁（Stage 1: e2e）

## Architecture

```
                    ┌─────────────── 同一个 PR job / 同一台 runner ───────────────┐
  checkout PR ─────▶│ bootstrap PR 源 → pr_vm / pr_libs / pr_driver (ambient)     │
  checkout base ───▶│ bootstrap base 源 → base_vm / base_libs / base_driver       │
  (base.sha)        │                                                             │
                    │  pr_xtask bench --ab \                                       │
                    │     --base-vm base_vm --base-libs base_libs \               │
                    │     --base-driver base_driver --threshold-time 0.10         │
                    │        │                                                     │
                    │        ▼  每个 scenario：                                     │
                    │   base.zbc = base_vm base_driver --emit-zbc scn             │
                    │   pr.zbc   = pr_vm   pr_driver   --emit-zbc scn             │
                    │   hyperfine 'base_vm base.zbc …' 'pr_vm pr.zbc …'  (同机相邻) │
                    │        │  results[0]=base(mean,stddev), results[1]=pr(…)     │
                    │        ▼                                                     │
                    │   _abVerdict: ratio=pr/base; 95%下界 R_lower(SEM 传播)       │
                    │   回归 ⟺ R_lower > 1+thr                                     │
                    └─────────────────────────────────────────────────────────────┘
                                  exit 1 = 回归(fail job) / 0 = 无 / 2 = 工具错
```

**跨-runner 偏移抵消原理**：base 与 pr 两次测量在同一台机器、同一 job、相邻数秒内完成 → 该机器的整体
快慢因子 `k` 同时乘进两侧，`ratio = (k·t_pr)/(k·t_base) = t_pr/t_base` → `k` 约掉。剩下的只有 within-run
抖动，可由 SEM 量化。这是**唯一**能凭一次比较区分真回归与机器噪声的结构。

## Decisions

### Decision 1: A/B 交错 = hyperfine 双命令单 invocation

**问题：** 怎么让 base/pr 共享同一 runner 状态？
**选项：** A — hyperfine 一次给两条命令（同 invocation，同机相邻）；B — 顺序两次独立 bench + 复用 `--diff`。
**决定：** 选 **A**。B 的两次 bench 相隔数分钟（各自 warmup+runs 全跑完），job 内 runner 可能已漂移，且
`--diff` 的 min/max-overlap 判红即便同机也漏 10% 回归（内部已验证）。A 让两侧相邻、且直接拿到两条
mean/stddev 算比值 CI。hyperfine 稳定版对多命令是「cmd1 全跑→cmd2 全跑」于同一 invocation——非逐次
交错，但同机相邻已足够抵消 between-run；逐次交错属过度工程，不做。

### Decision 2: 判红 = 比值 95% 下界 > 阈值（SEM 传播）

**问题：** 同机数据下如何「既紧又不假红」地判红？
**决定：** 对每场景（每 mode）：
```
SEM_b = stddev_b / sqrt(n_b);   SEM_p = stddev_p / sqrt(n_p)
ratio = mean_p / mean_b
relSE = sqrt( (SEM_p/mean_p)^2 + (SEM_b/mean_b)^2 )    // 商的误差传播
R_lower = ratio * (1 - Z * relSE)                       // Z=1.96 (95%)
回归 ⟺ R_lower > 1 + thr        // 「95% 置信真实比值超过阈值」
```
- confound 已被同-runner 抵消 → SEM（within-run）在此**统计有效**，这是 P0 情形下不成立、现在成立的关键。
- `R_lower ≤ 1+thr`：无法有把握判回归 → 非回归（标 `(overlap)`），保住不假红。
- `R_upper = ratio*(1+Z*relSE) < 1-thr`：显著提速 → 信息性标 `(faster)`，不影响退出码。
- 阈值 `thr` 默认 **0.10**（沿用 0.2.3 退出标准）；`Z=1.96`。stddev/n 缺失（不应发生）→ 回落
  `ratio > 1+thr` 裸比值（标 `(no-ci)`），与 P0 no-ci 回落同构。

### Decision 3: CI 建 base 工具链 + z42vm 复用优化

**问题：** base ref 怎么取、base 工具链怎么建、成本多大？
**决定：**
- base ref = `${{ github.event.pull_request.base.sha }}`（PR 的目标分支分叉点），第二个 `actions/checkout`
  用 `path: base-src` + `ref: <base.sha>` 落地。
- **base 工具链建法（实施期修正）**：**不复用 `ci-bootstrap` composite 建 base**。原设计设想「对
  base-src 跑 ci-bootstrap」，但该 composite 头一步 `cd "$(git rev-parse --show-toplevel)"` 会把工作目录
  锁回 **PR checkout**（`uses:` 步骤无法改工作目录、composite 不接受 root 入参），无法指向 base-src。
  改用**已 bootstrap 的 PR z42c 直接编 base 源**：
  - **base z42c** = `PR z42c.driver` 编 `base-src/src/compiler`（`driver -- build --workspace --release`，
    Z42_LIBS=PR alllibs）。**新编译器编旧源码**恒成立（staged-bootstrap 纪律：新 z42c 能编旧源；base 是
    PR 祖先）。
  - **base stdlib** = 上一步产出的 **base z42c** 编 `base-src/src/libraries`。
  - base flat libs（`--base-libs`）= base z42c 6 siblings + base stdlib 拼一个平铺目录；
    `--base-driver` = 其中的 `z42c.driver.zpkg`。
  - **为何测量仍有效**：只测 scenario **运行时**。base driver 虽由 PR z42c 生成其字节码，但它运行的是
    **base 的 codegen 逻辑** → 产出 **base 风格的 scenario .zbc**，再由 base_vm 跑。base driver 自身跑多快
    （编译期）不进测量。故「PR z42c 编 base driver」对 scenario 运行时零影响，A/B 语义完整。
- **z42vm 复用优化**：若 `git diff --quiet <base.sha>..HEAD -- src/runtime` 为真（本 PR 未动 Rust VM），
  base 与 pr 的 z42vm 字节等价 → **base 复用 pr_vm**，跳过 base 的 cargo release 建（最重一环）；动了
  `src/runtime` 才对 base-src cargo build z42vm。
- **格式-bump 边角（已知瞬态）**：PR bump zpkg 格式且**同时**动 src/runtime 时，base driver 是 PR(新)格式
  而 base_vm 是 base(旧)格式读不了 → 该 PR 当次 bench 可能红。与 bootstrap-seed.md「格式-bump 周期 bench
  瞬态红、随 nightly 自愈」一致，不阻塞（格式-bump 时 perf 对照本就意义有限）。
- 成本：动 runtime 的 PR ~2× 构建（Swatinem 缓存 Rust 依赖，净增一次 crate 编译 + 一次 base z42c+stdlib
  warm 建）；仅动 stdlib/compiler 的 PR 因复用 vm，增量只有 base z42c+stdlib 一次 warm 建。bench-pr 的
  `timeout-minutes` 由 30 提到 **45** 兜底。

### Decision 4: 退休 bench-baselines 作门禁 source，保留作历史 dashboard

**问题：** 跨-runner baseline 分支还留吗？
**决定：** 门禁**只**走同-runner A/B，不再 fetch bench-baselines。但 `bench-update.yml` 与 baselines 分支
**保留不动**——它每日单快照仍是有用的**历史趋势**记录（信息性 dashboard 数据源），只是不再喂门禁。
（彻底删 baseline 历史属另一决策，不在本 change。）

### Decision 5: `bench --ab` 也落一份 ab-result.json（信息性 artifact）

除人读输出 + 退出码外，`bench --ab` 写 `bench/results/ab.json`：每场景 `{name, mode, base_mean, base_stddev,
pr_mean, pr_stddev, ratio, r_lower, verdict}`。供 CI 上传 artifact / 未来 dashboard。**不复用 baseline-schema**
（那是单侧 baseline 格式；A/B 是双侧对照，另立最小结构）。

## Implementation Notes

- `_benchAb(ParseResult)`：几乎复刻 e2e 主循环（场景枚举、caps 过滤、mode sweep 已在 `_bench`），差异是
  「每场景编两份 zbc + hyperfine 双命令 + verdict」而非「编一份 + 单命令 + `_benchObj`」。抽公共
  场景枚举/编译 helper，避免复制 60 行（保持函数 <60 行硬限；`_benchAb` 拆 `_abOneScenario`/`_abVerdict`）。
- hyperfine 双命令 JSON：`results` 数组两元素，`results[0]`=第一条(base)，`results[1]`=pr；各有
  `mean`/`stddev`/`min`/`max`/`median`。用 `_abVerdict(baseObj, prObj, thr)` 纯函数算 R_lower + verdict
  → **可单测**（喂构造的 mean/stddev，断言 verdict）。
- `Std.Math.Sqrt`（`Math.z42:81`）算 SEM 与 relSE。样本数 n = hyperfine `--runs`（两侧同 runs）。
- base_driver 需 base 的 z42c.driver.zpkg + base flat libs（`_assembleAllLibs` 语义，但指向 base-src 的
  build 输出）；CI 把 base 工具链产物路径经 `--base-*` 传入。
- scenario **源用 PR 的**（同一 workload 两套工具链编译/运行）；若 PR 改了场景源，base 侧也编同一份 PR 源
  （测「同 workload、两 toolchain」，正确）。

## Testing Strategy

- 单元测试：`_abVerdict` 纯函数——(a) 真回归（ratio=1.2, 小 stddev）→ R_lower>1.1 → 回归；(b) 噪声
  （ratio=1.15, 大 stddev）→ R_lower≤1.1 → 非回归(overlap)；(c) 提速（ratio=0.8）→ faster；(d) no-ci 回落。
  放 `scripts/test/` 或 z42.test 下的 xtask 单测入口。
- 端到端：worktree 内手工造 base/pr 两套工具链（可用 origin/main 作 base、当前分支作 pr）跑一次
  `bench --ab --quick`，确认输出 + 退出码 + ab.json。
- CI 自验：本 PR 自身触发 bench-pr → A/B 对照它自己的 base（应无回归，绿）= 新门禁通路自证。
- 完整 GREEN：`xtask test`（bench 相关不在默认 stage，另需 `xtask build toolchain` 后手验 `bench --ab`；
  CI bench-pr 为最终权威）。

## Deferred（登记到 roadmap Deferred Backlog Index）

- **ab-bench-micro（Stage 2）**：把同-runner A/B 扩到 micro/stdlib tier（缺陷 #2）。前置 = Bencher 加
  mean/stddev（原方向 A 工作，此时才有意义）。
- **ab-bench-criterion（Stage 3）**：criterion（Rust）tier 接线或砍占位（缺陷 #6）。
- **ab-interleave-per-run**：逐次交错采样（比 hyperfine 双命令更抗 job 内漂移），当前非必要。
- **retire-baseline-branch**：若历史 dashboard 另有承载，彻底删 bench-baselines/bench-update。
