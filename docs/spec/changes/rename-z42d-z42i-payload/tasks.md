# Tasks: devtools/interactive payload 目录 + 组件名统一为 z42d/z42i

**变更说明：** 把 devtools/interactive 的 SDK payload 目录（`programs/devtools`/`programs/interactive`）
与 packages.toml 组件名（`[component.devtools]`/`[component.interactive]`）统一改成缩写 `z42d`/`z42i`，
与 apphost bin 名（`bin/z42d`/`bin/z42i`）及 z42b 布局（`bin/z42b` + `programs/z42b`）对齐。
**变更类型：** refactor（纯路径/名字重命名，零 zbc/zpkg 格式 bump，无行为语义变化）。

**原因：** bin 已是缩写 `z42d`/`z42i`，但 payload 目录与组件名仍是全称，风格不统一。
z42b/z42c 的 bin/payload/组件名三轴一致（都是 z42b/z42c），devtools/interactive 应看齐。

**保持不变**（对齐 z42b：`programs/z42b/z42.builder.zpkg`）：源目录 `src/toolchain/devtools`·
`src/toolchain/interactive`、toml 文件名 `z42.devtools.z42.toml`·`z42.interactive.z42.toml`、
zpkg 文件名 `z42.devtools.zpkg`·`z42.interactive.zpkg`、project.name。

**文档影响：** 两个 README component 段、`docs/design/toolchain/repl.md`、
`docs/book/src/compiler/project-build.md`、`docs/spec/changes/fix-repl-sdk-compiler-closure/proposal.md`
的 payload 路径引用。archive（冻结历史）不动。

## 任务

- [x] 1.1 toml payload 字段：`programs/devtools`→`programs/z42d`、`programs/interactive`→`programs/z42i`
      （`src/toolchain/{devtools,interactive}/core/*.z42.toml`；含 toml 内 `[component.*]` 注释）
- [x] 1.2 packages.toml：`[component.devtools]`→`[component.z42d]`、`[component.interactive]`→`[component.z42i]`、
      `sdk.include` 两项、组件分类注释
- [x] 1.3 打包 staging：`_pkgStageDir(root, "devtools"/"interactive")`→`"z42d"/"z42i"` + 日志/注释
      （`scripts/package/xtask_package_desktop.z42`）
- [x] 1.4 packages-config 断言：`sdk.include[6/7].name`、`_pkgcfgFindComponent` 键 + 变量名
      （`scripts/package/xtask_test_packages_config.z42`）
- [x] 1.5 launcher 运行期路径 `programs/interactive`→`programs/z42i` + 注释/报错文案
      （`src/toolchain/launcher/core/launcher_cli.z42`）
- [x] 1.6 dist 测试路径 + SKIP 文案（`scripts/test/xtask_test_dist.z42`）
- [x] 1.7 文档同步：两个 README、repl.md、project-build.md、fix-repl-sdk-compiler-closure/proposal.md
- [x] 1.8 验证：`xtask test packages` → `packages-config: PASS (3 packages, 9 components)`；
      live 引用 grep 清零（archive 除外）。**打包/dist 全链本地被 pre-existing 种子漂移
      （`__int32_equals` 已于 2026-08-27 从 native 移除，主树 `.z42` 种子滞后）阻断——与本次
      重命名无关，GREEN 以 CI 为准（cold/packaging 路径 CI 验证，见 bootstrap-seed.md）。**
