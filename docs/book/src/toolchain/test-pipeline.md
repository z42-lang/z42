# 测试流水线：两层模型（z42b 执行器 + xtask 编排器）

> **页型**: 机制页 ｜ **状态**: ✅ ②b + Slice 3 wasm(PR-1)/ios(PR-2)/android(PR-3) 已实现（host 执行 + 设备 build/deploy/run 归 z42b）｜ **代码**: `src/toolchain/builder/core/builder_test.z42` · `builder_device.z42` · `builder_device_ios.z42` · `builder_device_android.z42` · `src/libraries/z42.test/src/BundleRunner.z42` · `scripts/test/xtask_test_embedded.z42` · `src/toolchain/workload/test/agent/`
> **相关**: [xtask](../dev/xtask.md) · [测试门禁](../dev/test-gate.md) ｜ **对齐**: 2026-08-30

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

`z42b test <manifest.json> --rid <device> [--out <dir>] [--build-root <repo>]`
（libs 走 `Z42_LIBS`）。设备 RID 上 z42b 走完整流水线——sub-step flag 选粒度（默认无 flag =
build+run）。**test-agent 不再由 xtask 经 `--agent` 交付**，而是 z42b 从自己 SDK 里已装的 `test`
workload 解析（见下「test-agent 解析」）：

| flag | 语义 |
|------|------|
| `--stage-only` | 仅组装 deployable `<out>/{app,libs,bundle}`（②b 行为；`browser-wasm` 额外产 `files.json`）。枚举前按 ordinal 排序保确定序（[common-pitfalls §1](../../../.claude/rules/common-pitfalls.md)） |
| `--build` | native 平台构建 + deploy（stage + 平台产物就位） |
| `--run` | 触发设备 runner + 回收报告 |

per-platform driver（`builder_device.z42` wasm；`builder_device_ios.z42` ios；`builder_device_android.z42` android）：

| rid | build（`--build`） | run（`--run`，z42b spawn 原生工具） | report |
|-----|-------|------|--------|
| `browser-wasm` | `wasm-pack build --target web` + stage {app,libs,bundle}+files.json + 拷 pkg/harness | `npx playwright test --config playwright.embedded.config.ts` | playwright exit code（run.js 自断言 `window.__report`；z42b 转达 verdict） |
| `ios-arm64` / `iossim-arm64` | cargo × slices（host + device/sim）+ `xcodebuild -create-xcframework` + stage embedded corpus 进 XCTest `Resources/embedded` | `xcodebuild test -scheme Z42VM -destination <sim>`（sim UDID 由 `xcrun simctl` 解析；**一次 boot 同跑 R1–R7 + embedded**） | 解析 `Test Case … passed/failed` → `artifacts/test-reports/ios/junit.xml`（z42b 自写，格式同 xtask 共享 writer） |
| `android-arm64` / `android-x64` | `cargo ndk -t <abi> build --release` → jniLibs（NDK+ABI 由 rid 解析）+ stage embedded corpus 进 `androidTest/assets/embedded` | `gradlew :z42vm:connectedAndroidTest`（**一次 emulator run 同跑 R1–R7 + embedded**；emulator 由 CI action / 本地 test.sh 供给，**非 z42b**，design D2 不对称） | gradle 自产 junit（`z42vm/build/outputs/androidTest-results/…`；z42b 转达 exit code，报告路径不变） |

xtask（fleet orchestrator）保留**语料发现/编译/分片**与 native 工具**供给**（node/Xcode/NDK），把单目标
build/deploy/run 交给 z42b：`_build{Wasm,Ios,Android}Testhost` → z42b `--build`；`_run{Wasm,Ios,Android}Testhost` →
z42b `--run`。`IPlatformBackend` 的 `RunTests` 相应薄化为委托 z42b（ios `IosBackend.RunTests` →
`_runIosTesthost`）。**android 例外**：`AndroidBackend.RunTests` 保留本地 `test.sh`（emulator boot +
gradlew）作本地 emulator-lifecycle 路径——CI 走 reactivecircus action 内改调 z42b `--run`，emulator AVD
生命周期按 design D2 留在 CI action / test.sh，不进 z42b。

### test-agent 解析：dogfood `test` workload（Slice 3 PR-4）

设备 RID 上运行的 on-device test-agent（`z42.testagent.zpkg`）不再是 xtask 树内现编、经 `--agent`
交给 z42b 的临时产物，而是**已发布 `test` workload 的 payload**（package-test-workload / Change C）。
z42b 从**自己所在 SDK** 解析它，dogfood workload 的安装位置：

```
_ensureAgent()（builder_device.z42）:
  1. home = Z42_HOME | reverse(Z42_PORTABLE_VM)          # <sdk>/bin/z42vm → <sdk>
  2. 扫 <home>/runtimes/<ver>/workloads/test/z42.testagent.zpkg（版本目录排序，任一命中即用）
  3. 缺 → spawn `<home>/z42 workload install test [--from $Z42_WORKLOAD_SRC]`（离线源 / 网络）后重扫
```

- **真实已装 SDK（本地 / 分发）**：`z42 workload install test` 已把 agent 放进 `<home>/runtimes/<ver>/
  workloads/test/`（步骤 2 命中）；首次缺失走步骤 3 的 install-if-missing。
- **CI 设备 job（dev-tree，无 launcher）**：编译时把现编 agent 直接输出到 z42b SDK home 的同一 workload
  目录——`package workload test dev` 产 `z42.testagent.zpkg` → 拷进 `<home>/runtimes/dev/workloads/test/`
  （`<home>` = `artifacts/build/runtime`，即 `Z42_PORTABLE_VM` 反推）。z42b 步骤 2 直接命中 → **无需
  launcher spawn、无需下载 archive**。这样 CI dogfood 的是「workload 布局 + z42b 解析」，用的是 current
  源现编的 agent（非下载已发布 archive）。

### RID 值域

`host`（默认，in-process）｜ `browser-wasm` ｜ `ios-arm64` ｜ `iossim-arm64` ｜ `android-arm64` ｜
`android-x64`。`--build`/`--run` 需 `--build-root`；`--build` 另需 `--out`（agent 由 z42b 从 workload
解析，见上）；未知 RID 报错列出合法值。
