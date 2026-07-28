# Tasks: REPL 依赖世界惰性扫描

> 状态：✅ 已归档 2026-07-29（PR #65 合并 d8f67641）｜安全版 IMPL| 创建：2026-07-29
> 占子系统：`compiler`(z42c.pipeline DepScan) + `toolchain`(Script.z42)

## 已实现（安全版：eager BuildWorld + 惰性 Rebuild）
决策收敛：不上有风险的 base 链闭包（纯惰性 306ms），改 **eager BuildWorld 全部（world 完整→base 链
天然可解、零闭包风险）+ 惰性 Rebuild/DepIndex**。实测拆分证明可行：open 406 + BuildWorld 496 +
Rebuild(全) 1767 + DepIndex(全) ~1674。

- `DepScan.z42`：`DepScanResult` 加惰性字段（Opened/OpenedDirs/OpenedNames/Loaded）；`ScanDirsLazy`
  （eager BuildWorld 全 + 全量 nsMap + 只对 prelude Rebuild/DepIndex）；`EnsurePackageLoaded(scan,ns)`
  加载**所有**声明该 ns 的包（非 first-wins——共享 ns 如 Std 多包并存）。ScanDirs 全量路径不动。
- `Script.z42`：首轮 `ScanDirsLazy` + eager-load 默认 using 包（常用类型/补全即时可用）；`_compileSrc`
  load-and-retry（E0401 → 按 using 加载未加载包 → 重编一次）。

## 验证（current z42c，实测）
- **一致性（关键安全网）**：`ScanDirsLazy + EnsurePackageLoaded(全 ns)` vs 全量 `ScanDirs` 聚合
  **完全一致**（362 模块 / 672 类 / 6823 方法 / 2689 字段，`CONSISTENT=1`）。首次跑抓到并修了
  共享-ns first-wins 漏包 bug（357→362）。
- **性能**：LAZY 首轮 scan **1358ms** vs FULL **4313ms**（~68%）；REPL `-c "1+2"` 端到端 **1.86s**
  （含 VM 启动 + 默认 4 包 eager，vs 全量 ~4.5s，~58%）。
- **回归**：repl_completion / member / import / decls_multiline 四测全绿（惰性 + 默认 using）。

## 依赖 / 排队
- **依赖 #64（默认 using）**：eager-load 默认包让常用类型/导入补全（#62）即时可用；本分支已含默认
  using（若 #64 先合并，rebase 去重）。
- **compiler 锁**被 unify-run-modes 占；**self-host 字节不动点以 CI 为权威**（本地 `xtask build compiler` 过）。

## 后续（本次不做）
- 纯惰性 306ms（base 链 DEPS 闭包，照搬 VM lazy_loader）——收益 1358→306ms，但闭包有静默丢成员风险，
  留独立 change。
- A 异步预热：D 落地后首轮 ~1.8s，收益边际，暂挂。

## 决策待定（见 proposal.md）
- D1 触发策略：load-on-using(A,推荐) vs load-on-reference(B)
- D2 默认 using 包何时加载：首用即加载(A,推荐) vs 启动即加载(B)
- D3 base 链闭包：照搬 VM lazy_loader transitive 闭包(A,推荐)

## 阶段（D1=A/D2=A/D3=A 下，待确认后回填）
- [ ] 阶段 0 — 一致性基线：抓一组"惰性 vs 全量 scan 结果比对"的判据（同输入两路径 Exported/DepIndex 一致）
- [ ] 阶段 1 — `DepScan.ScanDirsLazy`：prelude world（z42.core BuildWorld/Rebuild）+ 全量廉价 nsMap（Open+ReadNamespaces）
- [ ] 阶段 2 — `EnsurePackageLoaded(scan, ns)`：FileOf(ns)→磁盘读→并入（真实 pkgDir）+ DEPS/base 链闭包展开（照搬 lazy_loader）+ 环安全
- [ ] 阶段 3 — 闭包正确性：新增祖先后受影响包回填重扫（或保证加载顺序）
- [ ] 阶段 4 — Script.z42 接线：Create 用 ScanDirsLazy；每轮 using 处理接 EnsurePackageLoaded
- [ ] 阶段 5 — 测试：正确性（跨包继承成员不丢 vs 全量一致）+ 性能（首轮 ~300ms；首次引用包后可用）
- [ ] 阶段 6 — 文档：repl.md 惰性机制页 + compiler 设计页 + roadmap

## 前置 / 阻塞
- **compiler 锁**被 unify-run-modes 占 → IMPL 排队（DRAFT 不占锁）。
- 依赖 #64（默认 using）合并——D2 与默认 using 交互。

## 实测依据
full scan 4343ms / 374 模块 vs core-only 306ms / 72 模块 → 天花板 ~93%。
