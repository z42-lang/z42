# Tasks: 构建布局统一到一个识别 workspace 的解析（PR-A：z42.project + z42c）

**状态：🟡 进行中 | 开始：2026-09-30**

类型：`refactor` + 用户可见默认变更（User 2026-09-30 裁决「现在直接做」「path 依赖继承 B 所在 workspace」）→
最小化模式，决策与风险记在本文件。「源码树零写入」系列第 ⑥⑦ 步，拆三个 PR：
- **PR-A（本 PR）**：z42.project 的 `WorkspaceLayout` + `BuildLayout.Resolve` 识别 workspace；z42c 全部调用点随之统一
  （单独构建成员、path 依赖代建、analyzer path 依赖、generated 落点）；preserved 早退加格式版本守卫；文档。
- PR-B：z42b `_computeDirs` / publish 路径 / hooks 与 launcher 改用 `BuildLayout`，z42b 默认根与 z42c 对齐。
- PR-C：xtask 删掉手抄的级联（`_toolchainDistDir` / `_outputDirOf` / `_wsOutputDir` / `_memberDist` / `_memberCache`），直接调 z42.project。

## 问题

「一个工程的产物落哪」有多套实现、默认值互不一致，文档还自相矛盾：
- z42.project `BuildLayout.Resolve`（z42c / launcher / z42b clean）只看清单自己的 `[build]`；
- `WorkspaceBuild.PlanLayout`（只有 `z42c build --workspace` 用）才兑现 `[workspace.build]`；
- 于是 workspace 成员被**单独构建**或被**当作 path 依赖代建**时走单工程默认 ⇒ 产物进源码树（`src/compiler/z42.project/dist` 等），
  且与 workspace 构建的位置对不上；
- z42-toml.md 说 workspace 未声明 output_dir 时默认 `artifacts/${project_name}/${profile}`，实现却直接报错。

## 方案

- `WorkspaceLayout`（z42.project）：`FindWorkspaceFor(memberDir)`（最近的 z42.workspace.toml + members/exclude 模式匹配）、
  `OutputTemplate`（未声明 → `artifacts/${project_name}/${profile}`）、`MemberOutputDir`、`MemberCacheDir`（自 pipeline 下沉）。
- `BuildLayout.Resolve`：清单既没配 output_dir 也没配 dist_dir、且是 workspace 成员 ⇒ 成员布局
  （output = 展开模板、cache = workspace cache_dir ?? `${output_dir}/.cache` + 防碰撞、dist = `${output_dir}/dist`、
  generated = 成员 generated_dir ?? `${output_dir}/generated`）。签名不变 ⇒ 8 个调用点一次性统一。
- `WorkspaceBuild.PlanLayout` / `ResolveMemberCacheDir` 委托 `WorkspaceLayout`（一份实现）；不再因缺 output_dir 返回 null。

## 风险与防线（User 选择「继承」前已告知跨版本耦合）

继承后，不同代的 z42c（SDK / 上一版 nightly 种子 vs 树内 gen1）可能先后往**同一个** workspace 成员 dist / cache 写
（例：SDK 的 z42c 编 xtask 时代建 path 依赖 z42.project）。cache 按「编译器语义指纹」作 key，格式 bump 未必改指纹；
preserved 早退此前只核对 dist 的 pack 位 ⇒ 可能把旧代写下的旧格式 zpkg 当「未变」留下，被当前 VM 按 strict-pin 拒载。
**防线**：`_distModeMatches` 同时核对 zpkg header 的 major / minor == 本编译器写出的版本，不等即重建。
（冷启动 `_ensureSeed` 本就会把种子代产物放进这些目录，混代在现状里已存在，此防线对其同样有益。）

## 进度概览

- [x] z42.project：`WorkspaceLayout.z42`（新）；`BuildLayout.Resolve` 成员分支 + 文件头说明
- [x] z42c.pipeline：`PlanLayout` / `ResolveMemberCacheDir` 委托
- [x] z42c.driver：`_distModeMatches` 格式版本守卫
- [x] e2e：xtask_compiler_e2e_cache.z42 ⑤（成员单独构建 / path 依赖代建均落成员布局、成员目录旁零写入）
- [x] 文档：z42-toml.md（[build] 字段表、成员继承规则、成员判定、默认模板、示例注释；publish_dir 归属 [platform.desktop]）
- [x] 完整 GREEN（8m48s，全阶段通过；gen1==gen2 9/9；`src/compiler/z42.{project,build}` 下不再生成 `dist/`）
- [ ] PR CI
