# Tasks: upgrade-apple-toolchain

> 状态：🟢 已完成 | 创建：2026-09-15 | 完成：2026-09-15

**变更说明：** macOS / iOS 工具链升到最新——CI runner `macos-15`（Xcode 16.x）→ `macos-26`（Xcode 26.6 / iOS SDK 26.5），
SwiftPM `swift-tools-version` 5.9 → 6.0（Swift 6 语言模式），`xcode_min` 15.0 → 16.0，并清掉三处与 `versions.toml` 脱钩的硬编码。
**原因：** 承接 #635 / #639「各依赖升到最新」的 Apple 侧。Android 侧版本全是手工 pin 的，Apple 侧则一直跟 runner 镜像漂：
`macos-26` 已是 `macos-latest`；App Store 自 2026-04 起要求 iOS 26 SDK 构建，导出的 app 要上架就必须 Xcode 26。
**文档影响：** `docs/workflow/ci.md`、`docs/design/testing/cross-platform-testing.md`、`.claude/rules/version-bumping.md`（artifact 名）、
`docs/design/{compiler/project.md,toolchain/export.md,toolchain/platform-export-lifecycle.md}`（`min_ios` 默认值）。

## 已裁决 / 边界

- **不动最低部署版本**（iOS 16.0 / macOS 13.0）——那是用户可见的设备支持面，不是工具链；要抬需单独裁决。
- **`xcode_min` 取 16.0 而非 26.0**：16 是 swift-tools 6.0 的真实下限；定 26 会让仍在 Xcode 16 的本地开发机直接告警，
  而代码并不需要 26。CI 与发布走 `macos-26` 镜像默认 Xcode（26.x），上架要求由 CI/发布构建满足。
- **导出的 `.xcodeproj` 格式不动**（`objectVersion = 56` / `SWIFT_VERSION = 5.0`）：`objectVersion 56` 所有现役 Xcode 都能开；
  Xcode 26 新建 app 模板默认仍是 Swift 5 语言模式，给用户 app 强开 Swift 6 严格并发不合适；只抬 `LastUpgradeCheck`
  会掩盖「推荐设置」提示而不真正应用，属于假升级。
- **不 pin Xcode 小版本**（不设 `DEVELOPER_DIR`）：镜像每出一个补丁版就替换旧补丁版，pin 小版本会周期性断；
  跟镜像默认 + CI 日志可查即可。

## 任务

- [x] 1.1 CI runner `macos-15` → `macos-26`：`ci.yml`（7 处 matrix/runs-on）、`release.yml`（3）、`jit-fixpoint-check.yml`（1）
- [x] 1.2 `xtask-bootstrap-artifact/action.yml` 下载名 `toolchain-macos-15` → `toolchain-macos-26`（上传端是 `toolchain-${{ matrix.os }}`，必须同步）
- [x] 1.3 `Package.swift`（仓内 harness）+ `xtask_package_ios.z42` 生成的 SDK `Package.swift`：`swift-tools-version` 5.9 → 6.0
- [x] 1.4 `versions.toml` `build.ios.xcode_min` 15.0 → 16.0；`iOSWorkload.z42` `RequireTool(">=15.0")` → 16.0
- [x] 1.5 `xtask_install.z42` Xcode 检查从「永远打 ✓」改为真比较（低于 `xcode_min` 即 warning，与 rust 检查同款）
- [x] 1.6 `builder_device_ios.z42` `IPHONEOS_DEPLOYMENT_TARGET` 硬编码 "16.0" → 读 `versions.toml` `platform.ios.min_ios`
- [x] 1.7 `launcher_export.z42` `min_ios` 缺省 "15.0" → "16.0"（SDK xcframework 以 16.0 为下限编译，缺省 15.0 会产出链接告警的工程）
- [x] 1.8 文档同步（见上「文档影响」）
- [x] 2.1 本地：Swift 6 模式类型检查 Sources + Tests（Xcode 16.4 / Swift 6.1）
- [x] 2.2 本地：xtask / z42b / launcher 编译 + `deps check` 正反对照 + iOS Simulator 端到端（shard 1/3）
- [x] 2.3 CI：PR 全套 + 手动 `workflow_dispatch` 跑 `test-ios-sim` 三分片（macos-26 首跑）
- [x] 2.4 归档

## 备注

- 2.2 本地结果（Xcode 16.4 / macOS 15.6.1，种子来自 #649 的 z42c）：xtask / z42b / launcher 编译通过；`deps check`
  打 `✓ Xcode 16.4 (≥ 16.0)`，把 `xcode_min` 临时改 26.0 即 `warning: Xcode 16.4 < ... 26.0`（阴性对照）；
  iOS Simulator：z42b 读 `min_ios` → cargo 两个 slice → xcframework → `xcodebuild test`（swift-tools 6.0）：
  R1–R7 7/7，embedded shard 1/3 **1126/1127**。唯一失败 `classes/static_ctor_with_instance_ctor` 是 #650
  随修复一起加的用例，本地种子 z42c 早于 #650 ⇒ 种子旧的假红（见 memory「worktree 供种的四类假故障」），与本变更无关；以 CI 为准。
- 2.3 CI 结果（head `da581c87`）：PR CI 29 pass / 4 skip；手动 run 34873829174 全部 38 个 job 绿，其中 `test-ios-sim` ×3 在
  `Image: macos-26-arm64` 上每片 `junit.xml (8 cases, 0 failed)`、0 条并发诊断 warning；`package-ios`、`test-host(macos-arm64)`、
  `test-stdlib-interp(macos-arm64)`、`compile-toolchain(macos-arm64)` 均绿。`release.yml` / `jit-fixpoint-check.yml` 的 macos-26 要等下次触发才实跑。
