# Tasks: fix — 门禁在 regen 前重建 debug z42vm

> 状态：🟢 已完成 | 创建：2026-07-16 | 完成：2026-07-16
**变更说明：** `xtask test` 把 `_buildDebugVmAndCompression()` 移到 golden regen 之前。
**原因：** golden regen（`_regenGolden`）用 `_activeVm(root,"debug")` 解析 debug z42vm，但门禁把 debug vm 的构建排在 regen **之后**（`_testAll`/`_testE2eCore`）。当变更新增 VM builtin 时，stale debug vm 不识新 builtin → regen 阶段 panic「unknown builtin」→ 全体 golden 假失败（实测 209/211）。且 regen 返回 1 触发早退，debug vm 构建那步根本跑不到。
**文档影响：** `docs/book/src/dev/test-gate.md`（若有该机制页则补一句「debug vm 先于 regen 构建」）；否则 tasks 备注即可。

## 变更点
- [x] 1.1 `scripts/test/xtask_test.z42` `_testAll`：`_buildDebugVmAndCompression()` 移到 `_regenForTest()` 之前
- [x] 1.2 `scripts/test/xtask_test.z42` `_testE2eCore`：debug vm 构建（`if (!haveTc)`）移到 `_regenForTest()` 之前
- [x] 1.3 验证：`./xtask test` **GREEN**（XTASK_EXIT=0，0 fail，自举不动点 7/7）；debug vm 实测在 regen 前重建（mtime 刷新）。根因失败模式此前已实证（stale debug → 209 假失败；fresh debug → 全绿）
- [x] 1.4 文档同步：`docs/book/src/dev/test-gate.md` mermaid 顺序修正（debug vm → regen）+ 加原理说明

## 备注
- 根因：build 顺序倒置（debug vm 建在 regen 后），非 regen/vm 逻辑本身错。
- `_regenCore` 内建的是 **release** vm（`_buildRuntime`），regen 却用 **debug** vm——两者剥离由来已久（debug 给 golden run 更好 panic）；本 fix 只纠正「debug 必须先于其消费者 regen 构建」。
- toolchain 路径（CI `--toolchain`）不受影响：`_activeVm` 在 toolchain active 时返回 toolchain vm（fresh），stale-debug 陷阱只在本地 build-tree 路径出现。
