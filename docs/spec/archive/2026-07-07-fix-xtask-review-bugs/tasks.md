# Tasks: fix-xtask-review-bugs

> 状态：🟢 已完成 | 创建：2026-07-07 | 完成：2026-07-07 | 类型：fix（最小化模式）

**变更说明：** 修复 `docs/xtask_review.md`（commit 9b0f99b2）中经当前 HEAD 复核仍存在的三个 bug。
**原因：** review 报告写作后 redesign-xtask-test / simplify-xtask-deps 已归档，但这三条不在任一
change 的 Scope 内，仍活。
**文档影响：** 无对外行为/机制变更（都是修正错误命令引用与非确定性），无需文档同步；review 报告本身
在归档提交中补注"已修"。

**子系统锁**：`toolchain`（scripts/xtask 源）。release.yml 属 CI（不上锁，同 docs）。ACTIVE.md 已登记。

## 复核结论（对齐 HEAD，2026-07-07）

- review #3（ci.yml Windows 腿覆盖清零）→ **已由 4e8ea7d9 修复**，不处理。
- review #1 / #2 / #4 → 复核仍在，本变更处理。

## 任务

- [x] 1.1 修 review #1：`scripts/test/xtask_test_changed.z42` 映射表 `xtask test lib`→`xtask test stdlib`
      （`_mapLibraryFile` 219-221 三处 + `_mapFile` toolchain 分支 251）。`test lib` 已无路由，
      改动 stdlib/toolchain 文件时 `test changed` 会 exit 2。
- [x] 1.2 修 review #4：`scripts/xtask_bench.z42`
      - `Directory.Enumerate(scenDir)` 未排序（common-pitfalls §1 违规，`--quick` 选集/结果顺序跨 OS 非确定）→ `_sortedStrings(...)` 包裹
      - vm/stdlib/compiler 三段手写 build-if-missing 且 compiler 无条件重建 → 收敛为 `_ensureToolchainDeps(root)`
- [x] 1.3 修 review #2：`.github/workflows/release.yml` 三处已删命令（对齐 ci.yml 已验证的形态）
      - `package release --rid` → 按 RID 分支：desktop = `package sdk/runtime/workload --no-build`（含 Windows runner-copy）；平台 RID = `package runtime --rid`
      - `release assemble-desktop-workload <v> dist` → `package workload <v> dist`
      - `release gen-release-index <v> dist stable v<v> <v>` → `package index <v> dist stable v<v> <v>`
- [x] 1.4 重建 xtask.zpkg（z42c build scripts/xtask.z42.toml --release）—— 41/41 编译通过
- [x] 1.5 验证：`test lib` 路由 exit 2 / `test stdlib z42.math` exit 0（映射产物可路由）；
      `bench --quick` 跑通 `_ensureToolchainDeps`（warm no-op）+ 有序选集（01_fibonacci/02_math_loop）
- [x] 1.6 归档：review 报告补注 + ACTIVE.md 释放锁

## 备注

- **验证范围说明**：本变更只动 xtask dev-CLI 分派逻辑（test-changed 映射 + bench）+ release.yml（CI）+
  spec 文档，**不触及 compiler/VM/stdlib/语言行为**。故验证 = xtask.zpkg 重编译通过 + 受影响命令实测
  （test changed 路由 / bench 运行与确定性），未跑完整 `xtask test` 语言 gate：其一它与本改动正交，
  其二工作树含 compiler 锁持有者（add-file-level-incremental）未提交的 `TypeChecker.z42` WIP，跑全 gate
  会被无关改动污染且不得连带提交。release.yml 属发布工作流本地不可验，改法逐条对齐 ci.yml 已验证的命令面
  + 核对 router 参数序（`package workload <label> <dist>` / `package index <label> <dist> <channel> <tag> <version>`）。
- review #3（ci.yml Windows 腿覆盖清零）复核已由 `4e8ea7d9` 修复，本变更未处理。
