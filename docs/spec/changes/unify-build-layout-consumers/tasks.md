# Tasks: z42b / xtask 的产物布局改用 BuildLayout（unify-build-layout 的 PR-B + PR-C）

**状态：🟡 进行中 | 开始：2026-09-30**

类型：`refactor` + 用户可见默认变更（z42b 默认布局与 z42c 对齐；User 2026-09-30 裁决「现在直接做」）→ 最小化模式。
叠在 unify-build-layout（PR-A）之上；另含 fix-z42b-source-tree-outputs（#975）的提交（改同一批 z42b 文件，#975 合入后变基去重）。

## 改动

- **z42b `_computeDirs`**：= `BuildLayout.Resolve` + z42b 自有的中间目录 `<output_dir>/build`。
  此前自持一套默认 `<src>/artifacts/<name>/<profile>/{.cache,build,dist}`（不认 workspace、模板只展开两个变量），
  与 z42c 的 `<src>/artifacts/<profile>` + `<src>/dist` 对不上：同一工程 z42b / z42c 各编一份放两处，launcher `z42 run` 只认后者。
  `_dirsResolve` 随之删除。
- **z42b publish**：`_pubResolveZpkg` / `_pubDefaultPublishDir` / `_pubHooksInterDir` 一律取 `BuildLayout.Resolve`（release）；
  手抄的 `_pubWorkspaceOutputDir` 删除。**用户可见**：裸工程（未配 output_dir）`z42 publish` 默认目录由 `<清单目录>/publish`
  变为 `${output_dir}/publish`（= 文档一直写的默认）。
- **xtask**：`_toolchainDistDir` / `_toolchainZpkg` / `_desktopPublishDir` 取 `BuildLayout.Resolve`；`_wsOutputDir` / `_outputDirOf`
  删除（它把 dist 的 `${output_dir}` 替换成未解析的原始串、未配 output_dir 时退回清单目录）；`_memberDist` / `_memberCache` /
  `_wsBuildRoot` 委托 `WorkspaceLayout`。
- **测试**：reuse-parent smoke 的父包 dist 位置改由 `BuildLayout.Resolve` 算（不再写死 `artifacts/dt.internal`）。

## 进度概览

- [x] z42b `_computeDirs` / publish 三处
- [x] xtask toolchain 路径 + workspace 成员路径
- [x] reuse-parent smoke
- [x] 完整 GREEN（8m40s，全阶段通过，gen1==gen2 9/9）；`build toolchain` 四组件仍为 `toolchain/<c>/{dist,publish}`；跑完后 `src/tests` 以外源码树零产物目录
- [x] z42b 闭包 e2e 写死了 z42b 旧 dist 位置 `artifacts/<name>/release/dist` → 改由 `BuildLayout.Resolve` 计算
- [ ] PR CI
