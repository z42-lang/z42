# Proposal: 接入 z42b —— `z42 new` / `z42 build` 端到端可用

> 状态：DRAFT（前置：`converge-z42c-onto-z42-project` 落地 + extract-compile-pipeline-api 落地）
> 子系统：stdlib（z42.build 建清单）+ z42c（Z42cCompiler 适配）+ toolchain（z42b 登记 + launcher 转发）
> **不持锁**，开工前按 parallel-development 重新登记。
>
> **前置刷新（2026-07-10）**：原前置表已陈旧——`toolchain` 锁已释放（compile-once-toolchain
> 已归档）、`retire-test-runner` 已完成归档（2026-06-30）。新增关键前置见下「## 前置依赖」：
> **z42.project 登记为 build member 会与 z42c.project 触发 flat-libs 同名串味**，故 z42.project
> 的 member 登记 + z42c.project 删除必须先由 `converge-z42c-onto-z42-project` 完成；本变更
> Scope 里的「workspace 加 z42.project 成员」随之移交 converge，本变更只登记 z42.build。

## Why

z42.project / z42.build / z42b（builder）三者已是**组织好的 PARKED 代码**（见 commits
08e840d5 / 5020b0d5 / c1d068d2 / 98cebedd），但**未接编译**：无 `*.z42.toml`、未登记
workspace/xtask/CI、launcher 不转发。结果 `z42 new` / `z42 build` 还不能用。

本变更把这套 PARKED 代码**接入**：建清单 → 登记构建 → 注入真实编译器（Z42cCompiler:
ICompiler 包装 z42c.pipeline 的内存编译）→ launcher 转发动词 → publish z42b apphost 到 bin。
落地后 `z42 new <name>` 脚手架、`z42 build` 产平台无关 app.zpkg 端到端可用。

**不做**（明确划走）：
- **test / bench**：归 `retire-test-runner`（前置反射 Method.Invoke 0.3.12）。
- **z42c 收敛到 z42.project**（删 z42c.project 自带 ProjectModel/ManifestLoader/SourceDiscovery/
  PathTemplate，改引用 z42.project）：高风险（改编译器 manifest 模型 flat→composed），独立变更
  `converge-z42c-onto-z42-project`。本变更 z42c 侧**只加** Z42cCompiler 适配，不动现有 manifest 路径。
- **平台 workload 尾相位**（Configure/GenerateProject/NativeBuild/Package 的真实实现）+ native 原语
  （Sign/Archive/Hash/Download）：publish/export 的平台落地另起；本变更 publish 仅桌面 apphost
  （复用现有 launcher_export 的 Apphost.Produce 或迁移，见 design Open Question）。

## What Changes

1. **包清单**：新增 `z42.project.z42.toml` / `z42.build.z42.toml` / `z42.builder.z42.toml`。
2. **构建登记**：`src/libraries/z42.workspace.toml` 加 z42.project / z42.build 成员；z42b 经 xtask
   `build` + sdk 组装登记；CI 对应 job 纳入。拓扑序：z42.project → z42.build → z42c.* → z42.builder。
3. **Z42cCompiler : ICompiler**（住 z42c.pipeline，依赖 z42.build 接口）：包装
   `CompileInMemory`/`CompilePackage`（extract-compile-pipeline-api），把 `CompileRequest`
   （SourceDir/Deps/Profile）→ 编译 → 写 `OutputZpkg`，返回 `CompileResult`。
4. **z42b `_hostCompiler()`**：`NoCompiler` → `new Z42cCompiler()`。
5. **launcher 转发**：`z42 new/build/publish/export` → spawn `z42vm z42b.zpkg -- <verb> ...`
   （复用 `add-workload-command-dispatch` 的 `SubcommandRouter.AddSpawn` 原语；见 design）。
6. **publish z42b apphost 到 bin**：xtask build/sdk 产 `z42b` apphost（同 `z42` launcher 模式）。

## 前置依赖

| 前置 | 状态（2026-07-10 刷新）| 原因 |
|------|------|------|
| `converge-z42c-onto-z42-project` | 🟡 DRAFT（本轮起草）| z42.project 登记为 build member + 删 z42c.project 的 manifest-model，**根除 flat-libs 同名串味**——否则本变更登记 z42.project 即炸 z42c 自举 |
| `extract-compile-pipeline-api` | 🟡 DRAFT（**磁盘无容器，仅 ACTIVE.md 挂名，待建**）| Z42cCompiler 包装其 `CompileInMemory`/`CompilePackage` |
| toolchain 子系统锁 | ✅ 已释放（compile-once-toolchain 已归档）| z42b 登记 + launcher 改动需 toolchain 锁空闲 |
| `retire-test-runner`（test/bench 划走）| ✅ 已完成归档 2026-06-30 | 本变更 Out of Scope 的 test/bench 已由其落地 |
| `SubcommandRouter.AddSpawn` | 见 add-workload-command-dispatch（可前借）| launcher 转发机制 |

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| ~~`src/libraries/z42.project/z42.project.z42.toml`~~ | — | **移交 `converge-z42c-onto-z42-project`**（z42.project member 登记必须与 z42c.project 删除同批，否则串味）|
| `src/libraries/z42.build/z42.build.z42.toml` | NEW | 包清单（deps: z42.core, z42.project）|
| `src/toolchain/builder/core/z42.builder.z42.toml` | NEW | exe 包清单（deps: z42.build, z42.project, z42c.pipeline, z42.cli）|
| `src/libraries/z42.workspace.toml` | MODIFY | default-members 加 **z42.build**（z42.project 由 converge 先登记）|
| `src/compiler/z42c.pipeline/src/Z42cCompiler.z42` | NEW | `Z42cCompiler : ICompiler` 包装内存编译 |
| `src/compiler/z42c.pipeline/z42c.pipeline.z42.toml` | MODIFY | 加 dep z42.build（仅接口）|
| `src/toolchain/builder/core/builder.z42` | MODIFY | `_hostCompiler()` 返回 `new Z42cCompiler()` |
| `src/libraries/z42.cli/src/SubcommandRouter.z42` | MODIFY | `AddSpawn(verb, desc, zpkgPath)`（若 add-workload-command-dispatch 未先落）|
| `src/toolchain/launcher/core/launcher_cli.z42` | MODIFY | 注册 new/build/publish/export 转发到 z42b |
| `scripts/xtask_cli.z42` | MODIFY | `build z42b` + sdk 组装含 z42b apphost |
| `scripts/xtask_compiler_z42.z42` 或新 `xtask_builder.z42` | MODIFY/NEW | z42b 构建编排 |
| `.github/workflows/*.yml` | MODIFY | CI 纳入 z42.project/z42.build/z42b 构建 |
| `docs/design/toolchain/build-orchestrator.md` | MODIFY | 从「前瞻草案」更新为已接入状态 + 接入决策 |
| `docs/spec/changes/ACTIVE.md` | MODIFY | 登记本变更子系统占用 |

**只读引用**：
- `docs/spec/changes/extract-compile-pipeline-api/design.md` — CompileInMemory/CompileResult 形状
- `src/toolchain/launcher/core/launcher_export.z42` — 现有 desktop publish（apphost 迁移参考）
- `docs/design/toolchain/launcher-command-dispatch.md` — 三层命令分发

## Out of Scope
- test / bench（retire-test-runner）；z42c→z42.project 收敛（converge-z42c-onto-z42-project）
- 平台 workload 尾相位真实实现 + native 原语；Pipeline Resolve/Trim/Assets 相位体深加工
- 自定义 `build/` 路径（生成一次性 driver）

## Open Questions
- [ ] launcher→z42b 转发用 `AddSpawn`（spawn-leaf）还是 SDK programs 目录发现（launcher-command-dispatch 第二层）？→ design 定。倾向先 AddSpawn（最小）。
- [ ] desktop publish 迁移 launcher_export → z42b，还是 z42b publish 暂调现有 Apphost.Produce？→ design 定。
- [ ] z42b 依赖 z42c.pipeline 会否让 z42b 体积/构建过重？是否需要 ICompiler 中立微库先行？→ design 评估（ICompiler.z42 已记计划重构）。
