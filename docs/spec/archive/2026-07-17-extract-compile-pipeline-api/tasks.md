# Tasks: extract-compile-pipeline-api

> 状态：🟢 已完成 | 完成：2026-07-17 | 归档：2026-07-26（追补 tasks + 归档）

> 补记（2026-07-26）：本 change 的核心（`PackageCompile` 库级 API）已于 2026-07-17 由 commit
> `1f3f9229` 落地并合入 main（proposal/design 头部即标「已实施」），但当时**未补 tasks.md、未 mv 到
> archive、ACTIVE.md 仍记「SPEC DRAFT 0/30」**。归档扫描（workflow 阶段 0）发现此陈旧账本，按
> 规范冲突检测补齐归档。follow-up `perf-optimize-repl-eval` 的 ② 落地 design.md Deferred 的
> `extract-compile-pipeline-api-future-scan-provider`（CachedScan 复用）。

## 进度概览
- [x] PackageCompile.Compile 核心抽取（⑥⑧⑩ 平移）— commit 1f3f9229
- [x] z42c.driver/_build 改委托 — commit 1f3f9229
- [x] PackageCompile 单测（4 绿）
- [x] GREEN：self-host 7/7 gen1==gen2 逐字节 + xtask test compiler

## Deferred（转 perf-optimize-repl-eval）
- extract-compile-pipeline-api-future-scan-provider：`CompileInputs.CachedScan` 跨调用复用
  DepScanResult（REPL 每轮跳过 ~3s 全量 DepScan）——由 perf-optimize-repl-eval ② 落地。
- extract-compile-pipeline-api-future-blob-provider：`DepScan.ScanBlobs`（依赖不在磁盘）——仍延后。
