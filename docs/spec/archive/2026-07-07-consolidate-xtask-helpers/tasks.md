# Tasks: consolidate-xtask-helpers

> 状态：🟢 已完成 | 创建：2026-07-07 | 完成：2026-07-07 | 类型：refactor（最小化模式）

**变更说明：** 落地 `docs/xtask_review.md` 第二节代码收敛的**安全可验证子集**——helper 贯彻 +
注释腐烂清理 + 小修。行为保持不变（byte-identical 定位/构建）。
**原因：** 已有 helper（`_selfContainedDriverDir`/`_compilerMembers`）未被半数调用点复用；
大重构改了代码留下描述已删 C# 种子的腐烂注释。
**文档影响：** 无对外行为/机制变更（纯内部收敛 + 源码注释）；README 命令面不变，无需同步。

**子系统锁**：`toolchain`（scripts/xtask 源）。ACTIVE.md 已登记。

## 范围（本次做）
- [x] 1.1 §2.2：新增 `_driverZpkg(root)`，retrofit 5 处 driver-zpkg 字面量
      （bench / test_lib / compiler_e2e / bootstrap_check / package_desktop）—— 实测 0 处裸字面量残留
- [x] 1.2 §2.3：`_assembleDriverHome` 硬编码 7 成员 → `_compilerMembers(root)`（drift 防护）——
      核对 workspace.default-members = 同一 7 成员集（order-independent copy，byte-equivalent）
- [x] 1.3 §2.9：注释腐烂——`xtask_stdlib.z42` 头部 C# 种子 5 步流程 + body C# fallback 注释重写为
      现行 z42c-only + `_ensureSeed`；清 test-runner 幽灵注释（test_dist ×2 / test_vm / test_assets /
      `_assembleAllLibs`）；`xtask.z42` dotnet primer + 幽灵 helper 列表（`_sh`/`_at`/`_join`）；
      `_ensureDriverVm` C# Driver 注释。（余 12 处低信号 test-runner 注释留后续 docs-cleanup）
- [x] 1.4 §2.10 小修：`_stageToolchain` 补 `_procEnd`；`_testAll()`→`_testAll(false)`；
      `_mapFile` `scripts/xtask`→`scripts/` 前缀；`test dist` help（interp→both）；`bench stdlib` 标签
- [x] 1.5 重建 xtask.zpkg 41/41 通过 + smoke：命令树 4 项 -h exit 0；`bench --quick` exit 0
      （driver-zpkg + ensureToolchainDeps + 有序枚举 01/02）；`_compilerMembers` 已被 `_assembleAllLibs`
      现役使用故等价性成立（cross-zpkg 全跑 z42c 建 fixture >2min，未整跑）
- [x] 1.6 归档 + 释放锁

## Out of Scope（本次不做，需更重验证的留后续独立 change）
- §2.1 `_z42cWorkspaceBuild` helper（3 处 workspace 构建）——触自举不动点路径，需 `test compiler`
  全绿验证；当前工作树含 compiler 锁持有者未提交 WIP（`TypeChecker.z42`），全 gate 会被污染，
  留 clean-tree 时做。
- §2.5 package 四平台 scaffold 提取 / §2.6 golden 枚举器合并 / §2.8 超长函数拆分——需 packaging /
  full gate 验证，同上留独立 refactor。

## 备注
（实施中记录）
