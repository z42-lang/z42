# Tasks: move-scripting-to-libraries

## 1. 物理搬迁
- [x] 1.1 `git mv src/toolchain/scripting src/libraries/z42.scripting`（源/测试/README/toml 整体）
- [x] 1.2 `z42.scripting.z42.toml` 头注释更新（物理落 stdlib、不再走特例步）
- [x] 1.3 `z42.scripting/README.md`「位置」段 + 构建命令改 stdlib workspace

## 2. workspace / build 重接
- [x] 2.1 `src/libraries/z42.workspace.toml` `default-members` 追加 `z42.scripting`（末尾 + 注释）
- [x] 2.2 `xtask_toolchain.z42`：删 `_buildScriptingLib`/`_buildReplStackToml`/`_scriptingLibsDir`，加 `_buildReplLib`
- [x] 2.3 `xtask_toolchain.z42` `_ensureToolchainDeps` 改调 `_buildReplLib`；`_buildToolchain` interactive 用纯 libs
- [x] 2.4 `xtask_package_desktop.z42` `_pkgStageToolchainComponents` 同步（`_buildReplLib` + interactive 纯 libs）
- [x] 2.5 `src/toolchain/repl/z42.repl.z42.toml` 头注释更新（普通 toolchain build 步）

## 3. 文档同步
- [x] 3.1 `organization.md`：scripting 入 stdlib 特例注 + 工具链库行补前端 z42c.core/syntax
- [x] 3.2 `src/libraries/README.md` 表增 `z42.scripting` 行
- [x] 3.3 `src/toolchain/README.md` 表删 `scripting/` 行（repl 行描述微调）
- [x] 3.4 `docs/design/toolchain/repl.md`：scripting 已搬 libraries
- [x] 3.5 `repl-input-completeness.md` 代码指针路径（Completeness → libraries；ReplEditing → repl，修 PR1 遗留）
- [x] 3.6 `bench/repl/BASELINE.md` + `repl_tests.rs` 注释路径

## 4. 验证（CI 权威）
- [ ] 4.1 push → 盯 compile-toolchain / compile-test-assets / verify-selfhost / test-consume / test-host
- [ ] 4.2 全绿 → rebase main 最新 → 重跑 GREEN → 合并 → 删分支/worktree
