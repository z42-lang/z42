# Proposal: 隔离 repl cdylib —— libz42_repl 移出共享 bin/

> 状态：DRAFT（User 裁决 B2 + dylib 输出到 programs/z42i/，2026-08-29）| 类型：fix(runtime+packaging)

## Why

host-only 的 REPL 行编辑 cdylib `libz42_repl.{dylib,so,dll}`（`extract-repl-native-cdylib`）当前打包
进 SDK 的**共享 `bin/`**（`_pkgStageZ42vm`）。但通用 ext 扫描器 `native::ext::load_all` 扫描运行
z42vm 的 **exec_dir**（即 `bin/`）dlopen 每个 `libz42_*`，其 `load_one` 只认 `compression` → 对
`repl` 喷 `tracing::warn!("ext: ignoring unknown lib repl")`。SDK VM（`bin/z42vm` / `bin/z42i`）跑
**任何**程序都会喷这行 WARN 到 stderr。

实害已现：golden 测试 harness（`xtask_test_vm`）把 stderr 并入 stdout 比对，当 `xtask test` 用
SDK VM 跑 goldens 时，这行 WARN **污染每个 golden → 满屏假红**（详见 memory
`xtask-test-z42home-repl-warn-pollution`）。根因是 repl cdylib 与通用 VM 共处 `bin/`——它本该只由
repl 专属探针 `corelib::repl_native` 加载，不该被通用 ext 扫描碰到。

## What Changes

1. **打包**：`libz42_repl` 从 `bin/` 移到 z42i 组件的 `programs/z42i/`（beside `z42.interactive.zpkg`）。
   通用 z42vm 的 exec_dir 扫描不含 `programs/z42i/` → 不再 spurious WARN。
2. **runtime 探针**：`corelib::repl_native::candidates()` 增一条从 `current_exe`（`<sdk>/bin/<app>`）
   派生的 `<sdk>/programs/z42i/` 查找路径，让 z42i apphost 仍能加载 REPL 编辑器。dev cargo-target
   路径（binary 旁）保留不变。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/corelib/repl_native.rs` | MODIFY | `candidates()` 加 `<sdk>/programs/z42i/` 派生路径 + 顶注 |
| `scripts/package/xtask_stage_components.z42` | MODIFY | `_pkgStageZ42vm` 移除 bin/ 拷贝；新增 `_pkgStageReplCdylib`（→ programs/z42i/） |
| `scripts/package/xtask_package_desktop.z42` | MODIFY | z42i publish 后调 `_pkgStageReplCdylib`（复用捕获的 staging dir） |
| `scripts/package/xtask_package.z42` | MODIFY | `_copyNativeLibs` 的 repl-skip 注释更新（逻辑不变，仍排除 repl 出 native/） |
| `scripts/package/xtask_test_stage_components.z42` | MODIFY | 加断言：repl 不在 bin/、在 programs/z42i/ |

**只读引用**：`src/runtime/src/native/ext.rs`（ext 扫描机制）、`scripts/package/xtask_package_assemble.z42`
（assemble = 整目录树 `Directory.Copy`，故 programs/z42i/ 下新增文件自动并入 pkgDir，无需改 payload 配置）。

## Out of Scope
- `load_one` 对 repl 报 WARN 本身（也可在 ext 侧静默跳过；本 change 走「物理隔离」路线，更彻底——
  repl 根本不进通用扫描目录，不依赖 ext 侧维护一张「已知非 ext lib」名单）。
- z42i apphost 位置（保持 `bin/z42i`，B2 方案）。

## Open Questions
- 无（B2 + programs/z42i/ 已定）。
