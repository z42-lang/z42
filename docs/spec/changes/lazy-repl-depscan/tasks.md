# Tasks: REPL 依赖世界惰性扫描

> 状态：🔴 DRAFT（待 User 6.5 裁决 D1–D3）| 创建：2026-07-29
> 拟占子系统：`compiler` + `stdlib`(z42.ir) + `toolchain`

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
