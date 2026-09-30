# Tasks: xtask 顶层产物目录集中定义 + `xtask layout` 查询口

**状态：🟡 进行中 | 开始：2026-09-30**

类型：`refactor`（路径不变，只收敛定义）→ 最小化模式。

整理 `artifacts/` 布局的第 1 步（另一半是 CI 侧的 `add-ci-xtask-shim`）：**先把路径收到一处，不挪任何目录**。
之后每挪一个桶只改 `xtask_layout.z42` 一行。

## 进度概览

- [x] 阶段 1: `xtask_layout.z42` 新增 ③ 顶层桶一节（`_artifactsDir` / `_scratchDir` / `_tmpDir` / `_toolsDir` /
      `_devSdkDir` / `_pubStagingRoot` / `_packagesDir` / `_releaseDir` / `_testReportsDir` / `_benchDir` /
      `_profileDir`）+ `_cargoTargetDir` / `_buildTestOutDir`
  - [x] `_packagesDir`（原 common）、`_pubStagingRoot`（原 stage_components）移入，**删掉原定义**
        （z42 自由函数跨文件同名会静默只留一份）
- [x] 阶段 2: 替换 ~40 处 `"artifacts/…"` 字面量（build / test / package / install / bench / profile）
- [x] 阶段 3: `xtask layout [key]`（CLI 注册 + `_layoutCmd`）
- [x] 阶段 4: 文档（xtask.md 命令表、artifacts-layout.md）
- [x] 阶段 5: GREEN（`xtask test`，11m51s，全阶段通过）
- [ ] 阶段 6: PR CI + 归档

## 有意保留的字面量

- `scripts/hooks/hooks.z42`：z42b publish 时单独编译的 hooks 工程，调不到 xtask 函数。
- `xtask_test_targets.z42` 的 `<toml 目录>/artifacts/{test-targets,dt.internal}`：那是 **z42b** 相对工程目录的
  输出（写进源码树的已知问题），属于后续「修 z42b 源码树写入」一步。
- `xtask_compiler_e2e_cache.z42` / `xtask_test_cross.z42` 的 `<proj>/artifacts/<profile>`：用户工程的 z42c 默认布局，不是 xtask 的桶。
- `xtask_test_changed.z42`：按路径前缀分类改动，不是写入位置。

## 顺带发现（记录，不在本 change 修）

- 用 z42c 直接编 `scripts/xtask.z42.toml` 时，它的 path 依赖 `z42.project` / `z42.build` 被编进**源码树**
  （`src/compiler/z42.project/{dist,artifacts}/`）：单独编 workspace 成员不继承 `[workspace.build].output_dir`。
  被 .gitignore 挡住不脏提交，但违反「artifacts/ 是唯一输出根」。CI 的 ci-bootstrap [2/5] 同样如此。
