# proposal — 把同-runner A/B 门禁扩到 micro + criterion tier

> 状态：DRAFT（待 User 6.5 gate 确认）
> 前身：[add-same-runner-ab-bench-gate](../add-same-runner-ab-bench-gate/design.md)（Stage 1，e2e A/B，已合 main #294）
> roadmap Deferred：`ab-bench-micro`（Stage 2）+ `ab-bench-criterion`（Stage 3）

## What

把 Stage 1 落地的**同-runner A/B 回归门禁**（base 与 pr 两套工具链同一 runner 相邻测量、比 ratio=pr/base、
SEM 传播判红）从 **e2e tier 一层**扩到全部三层：

- **Part A — Bencher 富化统计（前置）**：`Std.Test.Bencher` 增加 `MeanNs`/`StdDevNs`（两遍扫已采样数组），
  `printSummary` 行契约追加 `mean=<n>ns stddev=<n>ns`；`BenchStats` / `TestReport` JSON / `MicroBenchAgg` 同步携带。
  A/B 判红的 `_abVerdict` 需要 mean+stddev+n，这是 micro A/B 的硬前置。
- **Part B — micro/stdlib tier A/B 门禁（Stage 2）**：`bench --ab` 新增 micro 通道——把每个
  `src/libraries/<lib>/bench/*` 用 **base 工具链**和 **pr 工具链**各编一次、经 z42b 各跑一次，解析 mean/stddev，
  复用 `_abVerdict` 判红，折进 `ab.json` 并纳入门禁。接线 `bench-pr.yml`。
- **Part C — criterion(Rust) tier A/B 门禁（Stage 3）**：驱动 **criterion 原生的 `--save-baseline` /
  `--baseline` 同-runner 对照**（criterion 自带噪声/离群统计 + 回归判定），仅当 PR 触碰 `src/runtime` 时执行。
  连带处置 `smoke_bench`（纯 Rust sanity，保留但不门禁）。

## Why

缺陷盘点（前身 change / 本程序 memory）里两条 🔴 至今未治：

- **#2 micro/stdlib tier 在 CI 里完全无门禁无 baseline**：所有 stdlib `[Benchmark]` 在 CI 里纯信息性——
  bench-pr.yml 只 diff e2e，bench-update.yml 只存 e2e baseline。stdlib 函数级性能回归无人把关。
- **#6 criterion(Rust) tier 游离**：`gc_cycle_bench` 是真基准但只能 `cargo bench` 手跑，Rust 侧（GC/interp/
  decoder）性能无门禁；`smoke_bench` 是空壳。

Stage 1 证明了同-runner A/B 是「能抓真回归又不假红」的正确形状（cross-runner 噪声在 ratio 里精确抵消）。
把它铺满三层，z42 的性能护栏才完整。

## 非目标（本 change 不做）

- **自适应采样**（原「方向 A」的一部分）：固定 100 采样 + mean/stddev 已给出统计有效的 SEM；自适应采样让 `n`
  随时间预算浮动、复杂化 SEM 输入，且与门禁有效性正交 → 记 Deferred。
- **逐次交错采样**（`ab-interleave-per-run`）：同机相邻已足够抵消 between-run 漂移，保持 Deferred。
- **退休 bench-baselines/bench-update**（`retire-baseline-branch`）：历史 dashboard 保留不动，本 change 不碰。

## 范围 / scope

`toolchain`（xtask bench）+ `stdlib`（z42.test：Bencher/BenchStats/TestReport）+ `runtime`（criterion 接线，
仅 Cargo/bench 脚本层，不改 VM 逻辑）+ `docs`。无 zbc/zpkg **格式** bump（Bencher 行契约是 stdout 文本约定，
非二进制格式）。

## 落地形态

一个 change、一份 design（本目录），实施拆 **3 个逻辑提交 / 视规模拆 PR**：Part A → Part B → Part C，
A 是 B 的前置故先行。每部分各自 GREEN 再进下一部分。
