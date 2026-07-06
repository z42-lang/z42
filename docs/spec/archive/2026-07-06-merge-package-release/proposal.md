# Proposal: 合并 package + release → 单一 package router（sdk/runtime/workload/index）

## Why

命令面不对称、且 `package` / `release` 割裂：

1. **`package` 是 rid+profile 驱动的单命令**：`package [profile] [--rid R] [--variant] [--no-build]`
   → `_buildPackageCore` 内部按 rid 分支产不同包（host→desktop SDK、android/ios/wasm→平台包）。
   包**类型**藏在 rid 里，不显式。
2. **`release` router** 挂着两个与打包相关但零散的子命令：`assemble-desktop-workload`（合 4 个
   per-RID desktop workload）、`gen-release-index`（从 SHA256SUMS 生成 release-index.json）。
3. `packages.toml` 已按 **name** 定义包（`[package.sdk]` / `[package.runtime]` /
   `[package.workload-desktop]`），且 `_pkgAssembleFromStaging(packageName,…)` 已是 name 驱动
   ——但命令层没暴露 name。

对齐 `build compiler|stdlib|toolchain|test` 已确立的「动词 + 目标名」心智：**`package` 收成
name 驱动 router，`release` 删除**。

## What Changes

`package` 变 `SubcommandRouter`，4 个子命令：

| 新命令 | 作用 | 取代 |
|--------|------|------|
| `package sdk [--profile] [--no-build]` | 组装 host SDK 包 | `package [release]`（host 分支） |
| `package runtime --rid <rid> [--profile]` | runtime 包（native+stdlib，平台随 rid） | `package release --rid <rid>`（android/ios/wasm） |
| `package workload <label> [dist]` | 合并 desktop workload | `release assemble-desktop-workload` |
| `package index <label> [dist] [channel] [tag] [version]` | 生成 release-index.json | `release gen-release-index` |

- **profile 降为 `--profile release|debug` 选项**（默认 release）——因为旧的 `package release`
  里 "release" 是 profile 位置参数，与子命令名冲突。
- **`rid` 是 `package runtime` 的选项**（平台 runtime 包）。
- **删除 `release` router**（`_releaseRouter` / dispatch）。
- **CI 调用点全部迁移**（见下表）。

## Scope（允许改动的文件）

| 文件 | 类型 | 说明 |
|------|------|------|
| `scripts/xtask_cli.z42` | MODIFY | `package` 单命令 → router（sdk/runtime/workload/index）；删 `release` router + dispatch；`_dispatchPackage` |
| `scripts/package/xtask_package.z42` | MODIFY | `_buildPackageCore`（rid 驱动）拆成 `_packageSdk` / `_packageRuntime`（按 name + rid） |
| `scripts/package/xtask_package_release.z42`（或现 release 实现所在） | MODIFY | `assemble-desktop-workload`/`gen-release-index` 实现挂到 `package workload`/`index` |
| `.github/workflows/ci.yml` | MODIFY | 5 处调用点迁移（见 CI 映射表） |
| `scripts/README.md` | MODIFY | 命令一览：package 子命令；删 release 行 |
| `docs/workflow/` 相关页 | MODIFY | package/release 命令更新 |

**CI 调用点映射**（`.github/workflows/ci.yml`）：

| 行 | 旧 | 新 |
|----|----|----|
| 359 | `package release --no-build` | `package sdk --no-build` |
| 988 | `package release --rid $rid`（android） | `package runtime --rid $rid` |
| 1079 | `package release --rid $rid`（ios） | `package runtime --rid $rid` |
| 1173 | `package release --rid browser-wasm`（wasm） | `package runtime --rid browser-wasm` |
| 1377 | `release assemble-desktop-workload nightly artifacts/release` | `package workload nightly artifacts/release` |
| 1394 | `release gen-release-index nightly artifacts/release nightly nightly nightly` | `package index nightly artifacts/release nightly nightly nightly` |

**只读引用**：`scripts/packages.toml`（包定义）、`scripts/package/xtask_package_assemble.z42`（`_pkgAssembleFromStaging`）。

## Out of Scope

- packages.toml 的包定义本身（已是 name 驱动，不动）。
- `_pkgAssembleFromStaging` / 各 `_package{Desktop,Ios,Android,Wasm}` 内部逻辑（只重挂命令层，不改组装实现）。
- `build package`（若仍存在的旧别名）—— 确认后一并清理或留。

## Open Questions

- [x] 平台包映射（已查证）：`_buildPackageCore` 按 `_ridCategory(rid)` 分支——host→`_packageDesktop`(SDK)+`_buildRuntimePackage`(runtime)；android/ios/wasm→`_package{Android,Ios,Wasm}` **即平台 runtime 包**（可嵌入 native+stdlib）。故 `package runtime --rid X` 内部按 rid category 分派到对应 runtime builder，语义等价。
- **host 拆分的连带**：现 `package release`（host，无 rid）一次产 SDK + runtime + workload 三者；新模型拆成 `package sdk` / `package runtime` / `package workload` 三条。CI `host-package` job（line 359）需相应调用所需子集（大概率 `package sdk`；runtime/workload 若该 job 也需要则并列调）。
- [ ] `--variant` 选项去留（现 `package` 有；倾向保留在 sdk/runtime 上）。
