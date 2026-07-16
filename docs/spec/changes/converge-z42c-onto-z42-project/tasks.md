# Tasks: z42c 收敛到 z42.project

> 状态：🔴 DRAFT（待 6.5 审批）| 创建：2026-07-10
> 子系统：compiler + stdlib（双锁）| 前置：无（本变更是 wire-z42b 链的地基）
> **阶段 0 spike 出结果前，不写任何 z42c 切换代码**（design 决策 3）。

## 进度概览
- [x] 阶段 0: 死结实测 spike → **结论：共存即炸**（2026-07-11，见 design 决策 3）
- [x] 阶段 0.5: probe 定夺 → **改走 path C（User 裁决 2026-07-16）：根治短类名 first-wins**，
      不走 rename-前置/atomic。0.5a：串味 key 是 `ShortCls.Method`（按类名非文件名，改文件名无用）；
      0.5b：ci-bootstrap fast-path 用 seed-stdlib。→ 前置 `fix-crosspkg-static-ns-collision`（已合并
      main `737e7e82`：using-scoped 解析，VM 零改动）根治共存串味 → converge 不再需 rename/CI 手术。
- [x] 阶段 1: z42.project 登记 member + 首个 GREEN（2026-07-16，本 commit）
- [ ] 阶段 2: z42c 切引用 z42.project + 删 z42c.project manifest-model + 后端改名 z42c.zpkg
      （**晚一个 nightly**：种子轴——z42c 源 use z42.project 需种子 stdlib 已含 z42.project.zpkg，
      即阶段 1 先随一个 nightly 发布）
- [ ] 阶段 3: z42c 调用点 flat→composed 迁移（并入阶段 2）
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

## 阶段 1: z42.project 登记 + 首个 GREEN（其 README 承诺的「接入时验证」）✅ 完成 2026-07-16
- [x] 1.1 `z42.project.z42.toml`（deps: z42.core / z42.io / z42.toml）
- [x] 1.2 进 `src/libraries/z42.workspace.toml` `default-members`（含更新旧「暂不入 build」注释）
- [x] 1.3 `tests/manifest_roundtrip.z42` [Test]：toml→组合式 ProjectManifest/WorkspaceManifest round-trip
- [x] 1.4 `z42.project.zpkg` 编出；首个执行发现并修 **ManifestLoader.ParseWorkspaceText bug**——
      ctor 多传 4 个 `hasVer/ver/hasLic/lic`（Decision 2 已弃 SharedVersion 但 loader 未同步），
      被 z42c 静默按位吞掉致 `OutputDir` 空；删这 4 个实参 + `[workspace.project]` 解析后
      `output_dir` 正确回读（standalone 实证 `out=out/x`）。
- 自举安全：与 `fix-crosspkg-static-ns-collision` 同期实测 with/without z42.project 编出 z42c
  逐字节 **7/7 相同**（z42c 未切引用，z42.project 存在与否零影响）。

> **备注：暴露一个 z42c 缺陷（待独立跟进）**：z42c **未校验 ctor 实参数量**——16 实参传给
> 12 形参 ctor 未报错、静默按位截断（正是本 ManifestLoader bug 潜伏的原因）。应加 arity 校验
> 报诊断。属编译器，越出本 change scope，单列跟进。

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
