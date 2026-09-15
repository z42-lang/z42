# Tasks: 编译器输出变了，版本号必须累加 —— CI 守门 + 自举路径不读缓存

> 状态：🟢 已完成 | 完成：2026-09-15
> 变更类型：`fix`（最小化模式；xtask + CI，不改语言 / IR / VM）
> 所属程序：缓存与构建号 PR-B（memory `z42-cache-and-build-id`）。

**变更说明：** workspace 构建即将读增量缓存（PR-D）。缓存键里认编译器的只有 `CacheStore.CompilerFingerprint`
+ zbc/zpkg 格式 Minor；前者手工维护，从 #157 起只累加过 5 次，codegen 实际改动远多于此。漏改的后果：改了编译器
再 `build stdlib`，stdlib 命中旧编译器产物。User 裁决：编译器身份就用编译器自己的版本号（不让 VM 算），
「凡是影响输出的改动都累加」由 CI 强制。

**原因 / 修法：**
1. **CI 守门**：`xtask test fingerprint --base <base 树>`——本树编译器以 `--no-incremental` 重编 base 的 stdlib 源码，
   与 base 编译器的产物逐包比字节；变了而三元组（指纹 / zbc Minor / zpkg Minor）没变 ⇒ 红。挂在 bench-pr
   （它已在同 runner 建好 base 编译器 + base stdlib，本步只多一次 stdlib 编译）；格式代差时跳过。
   本树编译器编不过 base 源码 ⇒ `::warning::` 明示未测，不判红。
2. **必须真编的构建不读缓存**：`_z42cWorkspaceBuild` 加 `noIncremental`；不动点 gen2 传 true（命中缓存 = 没测）；
   ci-bootstrap 两代自举 4 处、bench-pr base/PR 编译器构建 2 处显式 `--no-incremental`（今天 workspace 还不读缓存，
   这是 PR-D 的前置兜底）。

**文档影响：** `.claude/rules/version-bumping.md`「编译器语义指纹」加 CI 守门节（删掉已否决的 build_id 聚合 follow-up）；
`docs/workflow/testing/verify-by-change.md` 速查表加一行。

- [x] 1.1 `scripts/test/xtask_test_fingerprint.z42`（新）+ `scripts/cli/xtask_cli_test.z42` 注册 `test fingerprint`
- [x] 1.2 `scripts/common/xtask_common.z42::_z42cWorkspaceBuild` 加 `noIncremental`；调用点：不动点 gen2 = true，其余 false
- [x] 1.3 `.github/workflows/bench-pr.yml`：新步骤 Compiler fingerprint guard；base/PR 编译器构建加 `--no-incremental`
- [x] 1.4 `.github/actions/ci-bootstrap/action.yml`：两代自举 4 处加 `--no-incremental`
- [x] 1.5 文档：version-bumping.md、verify-by-change.md
- [x] 1.6 本地验证（14s）：A/A（同一编译器、stdlib 由它自建）⇒ 25 包逐字节一致、通过；**阴性对照**：driver 改一处产物字节
      （`_stabilizeSourceIdentity` 的 SourceHash）只重建编译器 ⇒ 25 包变化、红并给出修复提示；再把指纹 5→6（base 用符号链接树保留旧值）⇒ 通过。已还原
- [x] 1.7 GREEN：`xtask test` 全 stage 绿（10m36s），自举不动点 3/3
