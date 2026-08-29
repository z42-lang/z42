# Tasks: 隔离 repl cdylib（libz42_repl 移出共享 bin/）

**变更说明：** host-only `libz42_repl` 从 SDK 共享 `bin/` 移到 z42i 组件的 `programs/z42i/`；
repl 探针增 `<sdk>/programs/z42i/` 派生查找路径。消除通用 ext 扫描器对 `repl` 的 spurious WARN。
**原因：** repl cdylib 与通用 z42vm 共处 `bin/` → ext 扫描器 dlopen 它 → `ignoring unknown lib repl`
WARN → 污染 golden（`xtask_test_vm` 并 stderr）。物理隔离到 programs/z42i/ 根治。
**文档影响：** repl_native.rs 顶注 + 打包注释（已随代码改）；无独立 doc 页需改（机制注释就地）。

> 状态：🟢 已完成 | 完成：2026-08-29 | 创建：2026-08-29 | 类型：fix(runtime+packaging)
> 分支/worktree：isolate-repl-cdylib（基于 origin/main）

- [x] 1.1 `repl_native.rs::candidates()` 加 `<sdk>/programs/z42i/`（current_exe→bin→sdk_root/programs/z42i）
- [x] 1.2 `repl_native.rs` 顶注 + `candidates` doc 更新（说明隔离与 no-WARN 理由）
- [x] 1.3 `xtask_stage_components.z42`：`_pkgStageZ42vm` 删 bin/ repl 拷贝；新增 `_pkgStageReplCdylib`
- [x] 1.4 `xtask_package_desktop.z42`：捕获 z42i staging dir + publish 后调 `_pkgStageReplCdylib`
- [x] 1.5 `xtask_package.z42`：`_copyNativeLibs` repl-skip 注释更新（逻辑不变）
- [x] 1.6 `xtask_test_stage_components.z42`：断言 repl 不在 bin/、在 programs/z42i/
- [x] 1.7 GREEN：`cargo build --release`（runtime 编译）——本地；打包/no-WARN 行为交 CI
- [x] 1.8 归档 + PR

## 备注
- assemble 是整目录树 `Directory.Copy`（`xtask_package_assemble.z42`），故 programs/z42i/ 下新增
  libz42_repl 自动并入 pkgDir，无需改 interactive 组件 payload 配置。
- 本地不可验：SDK 完整打包 + REPL 实跑（探针从 programs/z42i/ 加载）+ no-WARN → 交 CI/手验。
  本地只验 cargo build（runtime 编译）+ stage-components 单测（若跑 xtask test）。
- 关联：memory `xtask-test-z42home-repl-warn-pollution`（WARN 现场）；`extract-repl-native-cdylib`（cdylib 由来）。
