# 测试流水线：两层模型（z42b 执行器 + xtask 编排器）

> **页型**: 机制页 ｜ **状态**: ✅ ②b + Slice 3 wasm(PR-1)/ios(PR-2) 已实现（host 执行 + 设备 build/deploy/run 归 z42b；android → PR-3）｜ **代码**: `src/toolchain/builder/core/builder_test.z42` · `builder_device.z42` · `builder_device_ios.z42` · `src/libraries/z42.test/src/BundleRunner.z42` · `scripts/test/xtask_test_embedded.z42` · `src/toolchain/workload/test/agent/`
> **相关**: [xtask](../dev/xtask.md) · [测试门禁](../dev/test-gate.md) ｜ **对齐**: 2026-08-29

## 概述

z42 把「跑测试」拆成清晰两层，单一 owner、以 **bundle manifest** 为缝：

| 层 | 角色 | 职责 |
|----|------|------|
| **z42b** | 单-bundle **执行器** | 编译→部署→运行**一个**目标 / bundle：`z42b test <target> [--rid <rid>]` |
| **xtask** | 语料 / fleet **编排器** | repo 语料发现 / 编译 / 分片 / 聚合 / 门禁 |

**缝** = 一份 test-bundle：`manifest.json`（`{cases:[…]}`）+ 可部署布局 `{app,libs,bundle}`。
xtask 组装 bundle，交 z42b 执行（host）或组装设备 deployable（device）。

```
                  ┌──────────── xtask（fleet 编排器）────────────┐
 语料发现/编译/分片 → _buildTestBundle → manifest.json + {app,libs,bundle}
                  └──────────────────────┬───────────────────────┘
                                         │ 缝：bundle manifest / deployable
                  ┌──────────────────────▼───────────────────────┐
                  │            z42b（单-bundle 执行器）            │
 host:   z42b test <manifest.json> --rid host          → BundleRunner.RunBundle（in-process）
 device: z42b test <manifest.json> --rid <dev> --out D --agent A → 组装 {app,libs,bundle} deployable
                  └──────────────────────┬───────────────────────┘
                                         │ device deployable
        native build（wasm-pack / xcframework / cargo-ndk）+ 设备 RUN
        （Playwright / xcodebuild-sim / gradle）—— 仍在 xtask（CI-gated）
```

## 机制 / 实现

### 共享执行核：`Std.Test.BundleRunner`

一份 test-bundle 的**执行算法**沉在 stdlib 的 `z42.test`，被两个宿主共用：

- **on-device agent**（`Z42.TestHost.Agent`，嵌入式 z42vm，每个平台同一段字节码）；
- **host 上的 z42b**（in-process，z42b 本就跑在 z42vm 上、依赖 z42.test）。

```
BundleRunner.RunBundle(BundleCase[] cases, string bundleDir, string libsDir) → TestResult[]
    ├── golden case（Entry != null） → ModuleLoader.RunGoldensIsolated（每例独立 VM）+ 比对 stdout/expected
    └── unit case  （Entry == null） → Runner.RunModuleResults（共享 VM）
    → 聚合成一份 TestResult[]（golden 先、unit 后）
```

**关键设计**：`BundleRunner` **不吃 manifest 路径、不解析 JSON**——`z42.test` 是基础库，`TestReport`
刻意手写 JSON 以**避免依赖 z42.json**。因此 manifest.json → `BundleCase[]` 的解析留在**各宿主**
（agent 与 z42b 各自本就依赖 z42.json，各写一小段 adapter），只**共享算法**（golden 隔离比对 + unit 跑 +
聚合），I/O glue 按宿主分。这样一份 bundle 在 host（z42b）与设备（agent）上跑出**逐字节一致**的报告。

### host 路径：z42b in-process，无 testhost/agent 进程

`z42b test <manifest.json> --rid host`（或省略 rid）→ `builder_test._runModule` 识别 `.json` →
`_runBundleHost`：解析 manifest → `BundleRunner.RunBundle` → 按 `--format`（pretty/json）渲染 + 退出码
（任一失败 = 1）。**host 不再 spawn desktop testhost + agent 进程**——z42b 直接调 z42.test。
golden 仍由 `RunGoldensIsolated` 起独立 VM（隔离语义不变）。

xtask 的 `_testEmbedded` desktop 分支因此收缩为「组 bundle → 委托 `z42b test --rid host`」。

### device 路径：z42b 接管 build + deploy + run（Slice 3）

`z42b test <manifest.json> --rid <device> [--out <dir>] [--agent <zpkg>] [--build-root <repo>]`
（libs 走 `Z42_LIBS`）。设备 RID 上 z42b 走完整流水线——sub-step flag 选粒度（默认无 flag =
build+run）：

| flag | 语义 |
|------|------|
| `--stage-only` | 仅组装 deployable `<out>/{app,libs,bundle}`（②b 行为；`browser-wasm` 额外产 `files.json`）。枚举前按 ordinal 排序保确定序（[common-pitfalls §1](../../../.claude/rules/common-pitfalls.md)） |
| `--build` | native 平台构建 + deploy（stage + 平台产物就位） |
| `--run` | 触发设备 runner + 回收报告 |

per-platform driver（`builder_device.z42` wasm；`builder_device_ios.z42` ios；android → PR-3）：

| rid | build（`--build`） | run（`--run`，z42b spawn 原生工具） | report |
|-----|-------|------|--------|
| `browser-wasm` | `wasm-pack build --target web` + stage {app,libs,bundle}+files.json + 拷 pkg/harness | `npx playwright test --config playwright.embedded.config.ts` | playwright exit code（run.js 自断言 `window.__report`；z42b 转达 verdict） |
| `ios-arm64` / `iossim-arm64` | cargo × slices（host + device/sim）+ `xcodebuild -create-xcframework` + stage embedded corpus 进 XCTest `Resources/embedded` | `xcodebuild test -scheme Z42VM -destination <sim>`（sim UDID 由 `xcrun simctl` 解析；**一次 boot 同跑 R1–R7 + embedded**） | 解析 `Test Case … passed/failed` → `artifacts/test-reports/ios/junit.xml`（z42b 自写，格式同 xtask 共享 writer） |

xtask（fleet orchestrator）保留**语料发现/编译/分片**与 native 工具**供给**（node/Xcode/NDK），把单目标
build/deploy/run 交给 z42b：`_build{Wasm,Ios}Testhost` → z42b `--build`；`_run{Wasm,Ios}Testhost` →
z42b `--run`。`IPlatformBackend` 的 `RunTests` 相应薄化为委托 z42b（ios `IosBackend.RunTests` →
`_runIosTesthost`）。

### RID 值域

`host`（默认，in-process）｜ `browser-wasm` ｜ `ios-arm64` ｜ `iossim-arm64` ｜ `android-arm64` ｜
`android-x64`。`--build`/`--run` 需 `--build-root`；`--build` 另需 `--out` + `--agent`；未知 RID 报错列出合法值。

## Deferred / Future Work

### test-pipeline-android-device-run: z42b 接管 android build + deploy + run（Slice 3 PR-3）

- **来源**：z42b-device-run（wasm PR-1 / ios PR-2 已落）
- **当前状态**：android（`AndroidBackend`）仍是 xtask 自带 `cargo-ndk` build + `gradlew
  connectedAndroidTest`；embedded corpus 组装已经 z42b（`--stage-only`）。
- **触发条件**：PR-3 把 android 的 build/deploy/run 下沉 z42b driver（emulator AVD 生命周期仍留 CI
  action `reactivecircus/android-emulator-runner`，design D2 不对称）。

### test-pipeline-dogfood-workload-install: 设备 agent 走 workload-install（Slice 3 PR-4）

- 设备 driver 的 agent 来源从 in-tree `--agent` 切到「确保 test workload 就位」（查已装 → 无则
  `z42 workload install test`）；CI 用离线 archive 源，不每次打公网。
