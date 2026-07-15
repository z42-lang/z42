# Tasks: z42c 收敛到 z42.project

> 状态：🔴 DRAFT（待 6.5 审批）| 创建：2026-07-10
> 子系统：compiler + stdlib（双锁）| 前置：无（本变更是 wire-z42b 链的地基）
> **阶段 0 spike 出结果前，不写任何 z42c 切换代码**（design 决策 3）。

## 进度概览
- [x] 阶段 0: 死结实测 spike → **结论：共存即炸**（2026-07-11，见 design 决策 3）
- [ ] 阶段 0.5: 分阶段定夺 micro-probe（异文件名同类名是否避碰 + ci-bootstrap 种子机制）→ 定 rename-前置 vs atomic
- [ ] 阶段 1: z42.project 登记 member + 首个 GREEN
- [ ] 阶段 2: z42c.project 拆分（后端改名 z42c.zpkg + 删 manifest-model）
- [ ] 阶段 3: z42c 调用点 flat→composed 迁移
- [ ] 阶段 4: 验证（自举不动点 + 全 gate + 种子边界）+ 文档同步

## 阶段 0: 死结实测 spike（design 决策 3）✅ 完成 2026-07-11
- [x] 0.1 临时给 z42.project 加 toml + 进 default-members（z42c 不动）
- [x] 0.2 `build stdlib`（23 succeeded，z42.project 干净编译）+ `test compiler` → **崩**：`Z42.Build.Project.SourceDiscovery.Discover` 被 first-wins 误绑，还烤进 z42c.pipeline.zpkg。**共存即炸**坐实
- [x] 0.3 结果写回 design 决策 3；**2-nightly「先发 z42.project」路径作废**；回滚 spike 改动（`git diff` 净）+ 用两代自举 scratch 重播种恢复自举 7/7 green
- [x] 0.4 判定：须「rename-前置」或「atomic」，二者仍卡 ci-bootstrap 种子轴 → 阶段 0.5 micro-probe 定夺（非拆 publish-z42-project）

## 阶段 0.5: 分阶段定夺 micro-probe（阶段 2 实施前必做）
- [ ] 0.5a probe：z42c.project 的 SourceDiscovery.z42 改名（类名留）+ z42.project 共存 → 跑 `test compiler`，验「异文件名同类名」是否避开 first-wins
- [ ] 0.5b 核 `.github/actions/ci-bootstrap`：编当前 z42c 源用 seed-stdlib 还是 fresh-rebuilt-stdlib（定 atomic 是否可行）
- [ ] 0.5c 据 a/b 结果定 rename-前置 / atomic，写回 design + 停下与 User 确认最终排序

## 阶段 1: z42.project 登记 + 首个 GREEN（其 README 承诺的「接入时验证」）
- [ ] 1.1 `z42.project.z42.toml`（deps: z42.core / z42.toml / z42.io）
- [ ] 1.2 进 `src/libraries/z42.workspace.toml` `default-members`（拓扑：先于 z42c.*）
- [ ] 1.3 `tests/manifest-roundtrip/` [Test]：toml→组合式模型全段 round-trip + PathTemplate/SourceDiscovery
- [ ] 1.4 `xtask build stdlib` 产 `z42.project.zpkg` + `xtask test stdlib z42.project` 绿

## 阶段 2: z42c.project 拆分（refactor，先与迁移分离提交）
- [ ] 2.1 重命名 `src/compiler/z42c.project/` → `src/compiler/z42c.zpkg/`（含 `.z42.toml` name）
- [ ] 2.2 删 manifest-model 4 文件（ProjectModel / ManifestLoader / SourceDiscovery / PathTemplate）
- [ ] 2.3 ProjectSkeleton 处置（删用法 / 随后端包留，按 design）
- [ ] 2.4 `src/compiler/z42.workspace.toml` member 名 + z42c.pipeline/driver deps：去 z42c.project，加 z42.project + z42c.zpkg
- [ ] 2.5 zpkg 后端引用点改名（DepScan/IncrementalBuild/IndexedDist/IncrementalDriver 的 Zpkg*/CacheStore/PackageTypes）

## 阶段 3: 调用点 flat→composed 迁移（design Field Mapping 逐条）
- [ ] 3.1 `z42c.driver/src/Main.z42`（~15 处：`pm.Project.*` / `pm.Sources.*`）
- [ ] 3.2 `z42c.driver/src/BuildPaths.z42`（`pm.Build.*` / `pm.Project.Name`）
- [ ] 3.3 `z42c.pipeline/src/WorkspaceBuild.z42`（组合式返回 + `ws.Members`/`pm.Project.*`）
- [ ] 3.4 `PipelineSkeleton.z42` ProjectSkeleton 收尾

## 阶段 4: 验证 + 文档
- [ ] 4.1 `cargo build`（z42vm）无错（VM 零改动，仅确认）
- [ ] 4.2 `xtask test`（e2e + cross-zpkg + stdlib + compiler 自举不动点 7/7 byte-identical + vscode-syntax）全绿
- [ ] 4.3 `xtask test bootstrap`（种子边界，按阶段 0 定的排序）
- [ ] 4.4 dist 对账（若增量对账器可用）逐字节等价——迁移行为等价的硬证
- [ ] 4.5 文档：z42.project README 去 Parked、`docs/design/compiler/project.md` 单一模型 SoT、ACTIVE.md 释放双锁
- [ ] 4.6 CI `bootstrap-no-csharp` 绿（cold 路径权威，push 后盯）

## 备注
- spike（阶段 0）是硬门：其结果可能把本 change 一拆为二（publish-z42-project + 本体），届时回阶段 3 更新 Scope/拆 change 并请 User 重新确认。
- 阶段 2（refactor 改名）与阶段 3（迁移）分开提交（code-organization：拆分与功能变更分离）。
