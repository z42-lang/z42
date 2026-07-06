# Tasks: 合并 package + release → package router

> 状态：🟢 实现完成（待 CI 绿后归档）| 创建：2026-07-06 | 完成：2026-07-06
> 占用子系统：`toolchain`（ACTIVE.md 已登记）
> 变更类型：refactor（命令面重构）+ CI 调用点迁移

## 进度概览
- [x] 阶段 1: package router（sdk/runtime/workload/index）+ 删 release ✅
- [x] 阶段 2: `_buildPackageCore` 拆 name 驱动（_packageSdk/_packageRuntime/_packageWorkloadBuild + _pkgSetupDir/_pkgFinish）✅
- [x] 阶段 3: CI 6 处调用点迁移 ✅
- [x] 阶段 4: 文档 + 本地验证 ✅（CI 绿后归档）

## 裁决记录（User 2026-07-06）
- `package workload`：dual-mode——`<label>` 位参 → 合并（`_releaseAssembleDesktop`）；无 label → 建 per-RID（`--rid` 或 host 默认，`_buildDesktopWorkload`）。
- `package sdk`：只产 SDK（`_packageDesktop`）；host 的 runtime/workload 单独 `package runtime`/`package workload`。CI host-package job 改调三条。
- 平台包映射：`package runtime --rid X` → `_package{Android,Ios,Wasm}`（即平台 runtime 包）。

## 本地验证（macos-arm64）
- `package sdk --no-build` → `z42-0.3.0-macos-arm64-release`（bin/{z42b,z42c,z42d,z42i,z42vm} + SHA-256 OK）✅
- `package runtime --no-build` → `z42-runtime-0.3.0-macos-arm64` ✅
- `package workload`（build host）→ `z42-workload-0.3.0-desktop-macos-arm64`（apphost-<rid> + manifest）✅
- `package -h` 列 sdk/runtime/workload/index；`release` 已删（unknown command）✅
- `_buildPackageCore` 保留（`test dist`/`test all` 内部用完整 host 包）✅
- 平台包（android/ios/wasm）+ nightly workload/index 由 CI 验

## 阶段 1: 命令层
- [x] 1.1 `xtask_cli.z42`：`_packageRouter()`（sdk/runtime/workload/index，profile→--profile，rid→runtime 选项）
- [x] 1.2 删 `_releaseRouter` + `release` 注册 + dispatch
- [x] 1.3 `_dispatchPackage(p, r)`：按子命令分发

## 阶段 2: 实现层
- [x] 2.1 `_buildPackageCore` → `_packageSdk(profile,noBuild)` / `_packageRuntime(rid,profile)`（复用现 desktop/平台分支）
- [x] 2.2 `assemble-desktop-workload` / `gen-release-index` 实现挂 `package workload` / `package index`

## 阶段 3: CI
- [x] 3.1 ci.yml 6 处调用点迁移（见 proposal 映射表）
- [x] 3.2 grep 确认无残留 `-- package release` / `-- release ` 调用

## 阶段 4: 收尾
- [x] 4.1 scripts/README + docs/workflow 命令更新
- [x] 4.2 验证：`package sdk`/`runtime --rid`/`workload`/`index` 各产物正确（host 本地 + 平台由 CI）
- [x] 4.3 ACTIVE.md 释放锁；归档

## 待裁决（proposal Open Questions）
- 平台包（android/ios/wasm）映射 `package runtime --rid` 是否语义等价
- `--variant` 去留
