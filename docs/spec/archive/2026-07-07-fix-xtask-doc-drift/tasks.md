# Tasks: fix-xtask-doc-drift

> 状态：🟢 已完成 | 创建：2026-07-07 | 完成：2026-07-07 | 类型：docs（直接实施）

**变更说明：** 落地 `docs/xtask_review.md` 第四节文档去漂移——批量替换已删/改名命令、清 `--scope`
幽灵、校正 bootstrap-seed.md env 变量、test-runner 幽灵与死链。
**原因：** merge-package-release / redesign-xtask-test / simplify-compiler-build 归档时文档半径
被低估，operational 手册整页引用已 exit 2 的命令；规则文档写错 env 变量名（操作依据危险）。
**文档影响：** 本身即文档变更。仅动 **live 文档**（docs/workflow/、docs/book/src/dev/、
scripts/README.md、.claude/rules/）；**不动** docs/spec/archive/（历史不可变）与 docs/design/（已冻结）。

**子系统锁**：docs/规则不上锁（parallel-development 协议）。

## 命令迁移映射（事实源：scripts/xtask_cli.z42 当前命令面）
- `xtask package release`（host）→ `xtask package sdk`；`--rid <平台rid>` → `xtask package runtime --rid <rid>`
- `xtask build package [release]` → `xtask package sdk [--profile debug]`
- `xtask release assemble-desktop-workload <L> [dist]` → `xtask package workload <L> [dist]`
- `xtask release gen-release-index …` → `xtask package index …`
- `xtask regen` → `xtask build test`；`build test --no-stdlib`（flag 不存在）→ `xtask build test`
- `xtask test vm` → `xtask test e2e`；`test cross-zpkg` → `test e2e --dir cross-zpkg`
- `xtask test lib [X]` → `xtask test stdlib [X]`
- `z42-test-runner` → `z42b`（z42.builder 反射 test/bench host）
- `--scope=…` / `--parallel` / test-all `--quick`/`--with-dist`：z42 版 xtask 从未实现 → 删claim/标未实现；
  GREEN「commit 前 --scope=full」→「commit 前跑完整 `xtask test`」
- `./xtask help` → `./xtask -h`（help 非命令）

## 任务
- [x] 1.1 operational 手册 sweep（subagent）：20 live 文件（docs/workflow/*.md + docs/book/src/dev/*.md
      + scripts/README.md）——package release→sdk/runtime--rid 分派、release assemble/gen-index→package
      workload/index、regen/test vm/cross-zpkg/test lib/z42-test-runner 全替、`--scope`/`--parallel`
      两大节重写为现行三缩窄手段 + 未实现注、`xtask help`→`-h`、死链 build/z42c→build/compiler。
      验证：无误报 `package sdk --rid`；未碰 archive/design。
- [x] 1.2 .claude/rules/bootstrap-seed.md §4.3：env 解析顺序改现行 `Z42_HOME → Z42_PORTABLE_VM 反推
      → ./.z42`；「CI 设 Z42_TOOLCHAIN」→「Z42_HOME」；`scripts/build/xtask_bootstrap_check.z42`
- [x] 1.3 .claude/rules/workflow.md §4.2：GREEN「commit 前 --scope=full」→ 完整 `xtask test`；
      顺带 GREEN gate step `test lib`→`test stdlib`、`build package release`→`package sdk`（§4.5/4.1）
- [x] 1.4 .claude/rules/parallel-development.md：`test --scope` 心智模型 → `test changed` 子系统划分
- [x] 1.5 grep 验证：live 文档零残留 stale 命令（仅剩 4 处「--scope 未实现」注，全意图内）；
      build.md `xtask_regen.z42` 死路径 → `build/xtask_test_assets.z42`；README stage-list vm→e2e
- [x] 1.6 归档

## Out of Scope（留后续）
- §4.6 design/testing/testing.md 冻结与 workflow「详见」断链（结构性，需迁移决策）
- §4.4 GREEN stage 清单跨 6 处 SoT 完全收敛（本次只修明显矛盾/命令名，不做整体 SoT 重构）
- docs/design/*（冻结，不在本次）

## 备注
（实施中记录）
