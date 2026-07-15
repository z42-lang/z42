# Tasks: split-package-desktop-fn

> 状态：🟢 已完成 | 创建：2026-07-15 | 完成：2026-07-15

**变更说明：** 拆 `_packageDesktop`（80 非注释行，超函数硬限 60，review §2.8）——把 [2c/5] 段（apphost stub + 8 zpkg 构建 + 5 `z42b publish`）抽 `_pkgStageToolchainComponents`，8 连发 `_z42cBuildToml` 收敛为数组循环。纯机械搬移零行为变化。
**原因：** review §2.8 表列「`_packageDesktop` ~152 | 8 连发 `_z42cBuildToml` + 5 组 `_z42bPublish` 改数组循环」。
**文档影响：** 无（纯内部函数提取，不改命令面/行为/机制；文件头函数清单已同步；`docs/xtask_review.md` §2.8 行状态由归档时标注）。

## §2.8 拆 _packageDesktop（commit 1）
- [x] 1.1 抽 `_pkgStageToolchainComponents(root, z42cDir, cargoOut, cargoTarget, rid, rel) -> int`——[2c/5] 段自足搬出（内部算 libs/hostVm/apphostStub/z42bZpkg，均不被 `_packageDesktop` 后续使用）
- [x] 1.2 8 连发 `_z42cBuildToml` → `buildTomls[8]` + while 循环（顺序逐字保持：desktop/ios/android/wasm workload + launcher + builder + devtools + interactive）
- [x] 1.3 5 组 `_z42bPublish` 保留显式（各带不同 ✓ staged 日志），(toml, stage) 对逐字保持
- [x] 1.4 `_packageDesktop` [2c/5] 段改 1 行 helper 调用；主函数 **80→43 非注释行**（入 60 硬限）；文件头函数清单加 helper
- [x] 1.5 `git diff` 核对：8 build toml 顺序 + 5 publish (toml,stage,log) 逐路径等价；brace 42/42、paren 243/243 平衡；文件 334→320（仍 >300 软限但**净减 14 行**，非新增违规）

## 阶段 2: 验证
- [x] 2.1 CI 验证 → **run 29422619619：`compile-toolchain`（linux-x64/macos-arm64）+ `package-host`（4 OS 全 success）绿，0 失败**。`_packageDesktop` 编译 + 桌面 SDK 打包行为均验证通过；余下运行中的 job（test-vm-jit/test-stdlib-interp/test-cross-zpkg/compile-test-assets）测 VM/stdlib/golden，与本 packaging-only 改动无关
- [x] 2.2 CI 绿后归档 + 释放 toolchain 锁（本次会话）

## 备注
- 本环境冷检出：纯机械提取，靠 `git diff` 核对等价性；编译正确性 + 打包行为由 CI 的 host-package job 验。
- 直接在 main 开发（User 指示，不切分支）；push 后盯 host-package CI，红则 revert。
- 共享工作树有并行 session 的 `migrate-stdlib-to-params` WIP（stdlib/runtime）——本 change 仅动 `scripts/package/`（toolchain），子系统不冲突；commit 一律显式 `git add` 指定文件、不用 `-A`。
