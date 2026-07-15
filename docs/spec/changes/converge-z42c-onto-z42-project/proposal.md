# Proposal: z42c 收敛到 z42.project —— 根除 manifest-model 重复与 flat-libs 串味

> 状态：DRAFT | 创建：2026-07-10
> 子系统：**compiler**（z42c.project 拆分 + z42c.pipeline/driver 调用点）+ **stdlib**（z42.project 登记 member + 首个 GREEN）
> **不持锁**，开工前按 parallel-development 登记 compiler + stdlib 双锁。
> 这是 `wire-z42b-host-build` / z42b 全流程的**地基前置**：不删这份重复，z42.build 无法进 flat libs。

## Why

z42 的「项目清单模型」现在**存在两份**：

- `src/compiler/z42c.project`（namespace `Z42.Project`）——编译器自用。
- `src/libraries/z42.project`（namespace `Z42.Build.Project`）——z42.build / z42b 发布管线依赖的共享库（按最终形态写好，**尚未接编译**）。

两份的 `ManifestLoader.z42` / `SourceDiscovery.z42` / `PathTemplate.z42` **同文件名、同简单类名**。z42
的 flat `Z42_LIBS` 下**跨 zpkg 方法解析按文件名 first-wins**，故只要 `z42.project.zpkg` 与
`z42c.project.zpkg` 同时在 flat libs，就会串味、**炸 z42c 自举**（[z42.workspace.toml:27-31](../../../src/libraries/z42.workspace.toml) 记载）。这正是 z42.build / z42b 至今
不能进 build 的根因（[wire-z42b-host-build](../wire-z42b-host-build/proposal.md) 明确把本收敛划为独立前置）。

不做会怎样：z42b 全流程（`z42 new`/`z42 build`/真 workload NativeBuild）永远接不通——上层每一步都卡在这份重复上。

## What Changes（根因修复：消除重复，非打补丁）

1. **z42c.project 拆分**（勘察证实「删」不可能——它含编译器 zpkg 后端）：
   - **删除** manifest-model 4 文件（`ProjectModel.z42` / `ManifestLoader.z42` / `PathTemplate.z42` / `SourceDiscovery.z42`）——其职责由 `z42.project` 承担。
   - **保留** zpkg 后端（`ZpkgWriter.z42` / `ZpkgWriterIndexed.z42` / `ZpkgReader.z42` / `ZpkgBuilder.z42` / `PackageTypes.z42` / `CacheStore.z42`）——编译器产物机器，`z42.project` 按设计永不含。后端独立成新包 **`z42c.zpkg`**（名副其实；见 design 决策 1）。
   - `ProjectSkeleton.z42`（过渡占位）随 `PipelineSkeleton.z42` 一并处置（见 design）。
2. **z42c 改引用 z42.project 的组合式模型**：z42c.pipeline / z42c.driver 约 30 处调用点从**扁平**字段（`pm.Name` / `pm.IncludeGlobs` / `pm.HasOutputDir`）改为**组合式**（`pm.Project.Name` / `pm.Sources.Include` / `pm.Build.HasOutputDir`）。采纳 z42.project 的最终形态（[philosophy：最终方案优先](../../../.claude/rules/philosophy.md)），不回填扁平层。
3. **z42.project 登记为 build member**：新增 `z42.project.z42.toml` + 进 `src/libraries/z42.workspace.toml` `default-members`，产 `z42.project.zpkg`；补首个 round-trip [Test]（其 README 承诺的「接入时 GREEN」）。
4. **拓扑与 CI**：z42.project 先于 z42c.* 构建；z42c.pipeline/driver deps 把 `z42c.project` 换成 `z42.project`（模型）+ `z42c.zpkg`（后端）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42.project/z42.project.z42.toml` | NEW | 包清单（deps: z42.core, z42.toml, z42.io）|
| `src/libraries/z42.project/tests/manifest-roundtrip/` | NEW | 首个 GREEN：toml→组合式模型 round-trip [Test] |
| `src/libraries/z42.workspace.toml` | MODIFY | `default-members` 加 `z42.project`（+ 若后端改名，改 `z42c.zpkg` 引用；见 design）|
| `src/compiler/z42c.project/src/ProjectModel.z42` | DELETE | manifest-model → z42.project |
| `src/compiler/z42c.project/src/ManifestLoader.z42` | DELETE | 同上（消除同名串味源）|
| `src/compiler/z42c.project/src/SourceDiscovery.z42` | DELETE | 同上 |
| `src/compiler/z42c.project/src/PathTemplate.z42` | DELETE | 同上 |
| `src/compiler/z42c.project/src/ProjectSkeleton.z42` | DELETE/MOVE | 过渡占位，随 PipelineSkeleton 处置（design 定）|
| `src/compiler/z42c.project/` → `src/compiler/z42c.zpkg/`（重命名）| RENAME | 后端独立包（含 Zpkg*/PackageTypes/CacheStore）；含其 `.z42.toml` name 改 |
| `src/compiler/z42.workspace.toml` | MODIFY | member `z42c.project`→`z42c.zpkg`；deps 拓扑 |
| `src/compiler/z42c.pipeline/z42c.pipeline.z42.toml` | MODIFY | deps: 去 `z42c.project`，加 `z42.project` + `z42c.zpkg` |
| `src/compiler/z42c.driver/z42c.driver.z42.toml` | MODIFY | 同上 |
| `src/compiler/z42c.driver/src/Main.z42` | MODIFY | ~15 调用点 flat→composed（`pm.Project.*`）|
| `src/compiler/z42c.driver/src/BuildPaths.z42` | MODIFY | `pm.*Dir`→`pm.Build.*`、`pm.Name`→`pm.Project.Name` |
| `src/compiler/z42c.pipeline/src/WorkspaceBuild.z42` | MODIFY | LoadWorkspace/Load 返回组合式；`ws.Members`/`pm.Project.*` |
| `src/compiler/z42c.pipeline/src/PipelineSkeleton.z42` | MODIFY | ProjectSkeleton 处置 |
| `src/compiler/z42c.pipeline/src/DepScan.z42`、`IncrementalBuild.z42` | MODIFY | zpkg 后端引用改 `z42c.zpkg`（ZpkgReader/CacheStore）|
| `src/compiler/z42c.driver/src/IndexedDist.z42`、`IncrementalDriver.z42` | MODIFY | 同上（ZpkgWriterIndexed/CacheStore/PackageTypes）|
| `src/libraries/z42.project/README.md` | MODIFY | 去「Parked」，标已接入 + GREEN |
| `docs/design/compiler/project.md` | MODIFY | 单一 manifest 模型 SoT 更新（组合式；z42c 引用 z42.project）|
| `docs/spec/changes/ACTIVE.md` | MODIFY | 登记 compiler + stdlib 双锁 |

**只读引用**：
- `src/libraries/z42.project/src/*.z42` — 组合式模型字段（迁移映射依据）
- `.claude/rules/bootstrap-seed.md` — 分阶段/种子约束（死结判定）
- `scripts/build/xtask_stdlib.z42`（`_assembleAllLibs`）、`xtask_compiler.z42`（self-host gate）— flat libs 装配 + 不动点门禁

## Out of Scope
- z42.build / z42.builder 建清单 + z42b 接线（→ `wire-z42b-host-build`）。
- Z42cCompiler : ICompiler 适配（→ `extract-compile-pipeline-api` / wire-z42b）。
- 真 workload NativeBuild（apphost 从源码 cargo 编）（→ 后续 `desktop-nativebuild-apphost`）。
- 组合式模型新增字段的功能扩展（Profiles/Exes/Platform 已在 z42.project，但本变更只保证 z42c 现用字段等价迁移，不新接特性）。

## Open Questions（design 定 / 实测定）
- [ ] **【死结·必先实测】** present-but-unconsumed 的 `z42.project.zpkg`（z42c 尚未引用它、z42c.project 仍在）会不会炸 z42c 自举？决定分阶段形态（见 design 决策 3）。**tasks 阶段 0 spike 先答此题，再定后续所有排序。**
- [ ] zpkg 后端包名：`z42c.zpkg`（推荐）还是保 `z42c.project` 名只剥 manifest-model？（design 决策 1）
- [ ] z42.project 的 `WorkspaceManifest` 缺 `SharedVersion`/`SharedLicense`（z42c.project 有解析）——z42c 现无消费者，确认可弃还是补回 z42.project？（design 决策 2）
- [ ] `ProjectSkeleton`：删用法还是随后端包保留？
