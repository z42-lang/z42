# Tasks: perf-workspace-carry-depscan（流程优化 F2）

> 状态：🟢 已完成 | 创建：2026-08-21 | 完成：2026-08-22 | 类型：perf/refactor（纯内部，字节不动点）

**变更说明：** 给 `DepScan.ScanDirs` 加**进程级 zpkg memo**（`DepScanCache`）——把「同一 zpkg 被
workspace 各成员重复 `ZpkgReader.Open` + `TsigReconcile.Rebuild`」的 O(N²) 降成 O(N)。

**原因（profile 实测）：** `build --workspace` 逐成员 `PackageCompile.Compile` → 每个 `DepScan.ScanDirs`
把 libsDirs 里**所有** zpkg 重开 + 重建 TSIG 一遍。24 个 stdlib 成员实测 **DepScan 合计 ~20s（≈ 编译核心
时间的 60%）**，每成员恒定 ~850ms（与成员自身大小无关，纯重复劳动，且每次都跑不缓存 → warm==clean）。

**最本质设计（避免 carry-forward 的顺序/过滤字节风险）：** 不复用整个 scan（其 DepIndex 按 declaredDeps
过滤、Exported[] 排序均是每成员/顺序敏感的），而是**只 memo 最贵的两块纯函数原语**：`ZpkgReader.Open`
结果 + 每包 `TsigReconcile.Rebuild` 结果。ScanDirs 算法/排序/declaredDeps 过滤/self-exclude 全不变 →
**字节不动点天然成立**。合法性：Open 是 zpkg 字节纯函数；包 P 的 TSIG 只依赖 P + 祖先（拓扑序保证在场）
→ 跨成员恒定。key=path（进程内 path→内容稳定不变式，见 DepScanCache 头注）。

**文档影响：** z42c.pipeline README 核心文件表 +DepScanCache.z42；`docs/book` 机制页（DepScan/workspace
构建）补 F2 memo 原理（归档前）。

## 任务
- [x] 1.1 新建 `DepScanCache.z42`：`CachedZpkg`（path→ZpkgInfo+懒填 Tsig）+ `DepScanCache.Get(path)`
- [x] 1.2 `DepScan.ScanDirs`：pre-open 循环改 `DepScanCache.Get`（命中零 I/O）；加 `cached[]` 平行数组
- [x] 1.3 `DepScan.ScanDirs`：main-loop TSIG 重建改缓存查（`cached[pi].Tsig` 懒填）
- [x] 1.4 字节不动点守卫：F2 编出 stdlib 合并 sha256 **== baseline `27631e03…`**（逐字节一致 ✅）
- [x] 1.5 性能对比（instrumented，同款 PROF apples-to-apples）：
      **DepScan 合计 19,996ms → 5,730ms（-14.3s，-71%）**，buildcus 持平（13,022→12,776ms 噪声）。
      每成员 DepScan ~850ms → ~210ms（首成员 660ms=冷缓存填充）。全量 `build stdlib` 97.85s→86.16s（-12%）。
- [x] 1.6 z42c.pipeline README 核心文件表 +DepScanCache.z42 行；docs/book `compiler/project-model.md`
      新增「跨成员依赖扫描 memo（F2）」机制节 + 实现表登记

## 验证
- [x] V1 完整 `xtask test` 全绿（F1 base 94b05050，可编译）：**self-host 不动点 5/5 gen1==gen2 逐字节**
      （含 z42c.pipeline，证明缓存确定性）· golden regen 267/0 · e2e 256/0 · cross-zpkg · stdlib [Test]
      · vscode-syntax → GREEN all stages (C#-free)。
      注：origin/main tip 9336f322（PR4a #245 后）当前**最新 nightly 种子编不过**（GeneratorDriver.z42
      引导窗口，与 F2 无关且在依赖序上早于 z42c.pipeline）→ tip 的 rebase-后 GREEN 交 CI 权威判定
      （bootstrap-seed.md：种子受限的本地路径以 CI 为准）。F2 rebase 到 tip 干净（不碰 PackageCompile）。
