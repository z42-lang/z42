# Tasks: finish-xtask-review-cleanup

> 状态：🟢 已完成 | 创建：2026-07-08 | 完成：2026-07-08 | 类型：refactor（最小化模式）

**变更说明：** 落地 `docs/xtask_review.md` §2 剩余的**安全可验证**收敛项：§2.7 两套字符串排序
合一、§2.9 剩余 test-runner 幽灵注释。
**原因：** consolidate-xtask-helpers 已做 §2 大部分安全子集，这两项当时留尾；现补齐。
**文档影响：** 纯内部（helper 收敛 + 注释），无对外行为变更，无需文档同步。

**子系统锁**：`toolchain`（scripts/xtask 源）。ACTIVE.md 已登记。与并行 compiler WIP（另一会话）
不同子系统、不冲突。

## 范围（安全可验证：xtask 重编 + smoke，不重建 z42c）
- [x] 1.1 §2.7：`_sortedStrings`（common，选择排序返回副本）改为「copy + `_sortStrings`」薄包装
      （`_sortStrings` = golden 的原地插入排序），删重复排序算法；两者行为一致（升序确定性）。
- [x] 1.2 §2.9：剩余 12 处 `z42-test-runner` 注释 → z42b（test_lib / test_lib_units / compiler）。
- [x] 1.3 重建 xtask.zpkg + smoke（bench --quick 用 `_sortedStrings`；test changed --dry-run 用 `_sortStrings`）。
- [x] 1.4 归档 + 释放锁。

## Out of Scope（需干净 compiler 树 / 更重验证，留后续）
- §2.7 copy 变体合一（`_copyZpkgs`/`_copyStdlibZpkgs`/`_stageCopyExt`→`_copyAll`）——只由 cross-zpkg/
  packaging gate smoke，当前树含 compiler WIP 污染这些 gate。
- §2.1 `_z42cWorkspaceBuild` helper（需 `test compiler` 不动点，compiler WIP 污染）。
- §2.5 package 四平台 scaffold / §2.6 golden 枚举器合一 / §2.8 超长函数拆分（需 packaging/e2e/fixpoint）。
