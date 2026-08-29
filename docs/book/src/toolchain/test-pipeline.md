# 测试流水线：两层模型（z42b 执行器 + xtask 编排器）

> **页型**: 机制页 ｜ **状态**: ✅ ②b 已实现（host 执行 + 设备组装归 z42b）｜ **代码**: `src/toolchain/builder/core/builder_test.z42` · `src/libraries/z42.test/src/BundleRunner.z42` · `scripts/test/xtask_test_embedded.z42` · `src/toolchain/workload/test/agent/`
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

### device 路径：z42b 组装 deployable，xtask 保留 native build + RUN

`z42b test <manifest.json> --rid <device> --out <dir> --agent <z42.testagent.zpkg>`（libs 走
`Z42_LIBS`）→ `_assembleDeployable` → `_stageDeployable`：把 agent + flat stdlib + bundle 落成
`<out>/{app,libs,bundle}`；**`browser-wasm` 额外产 `files.json`**（VFS→url 映射，浏览器不能枚举远端目录）。
枚举前按 ordinal 排序，保证确定序（见 [common-pitfalls §1](../../../.claude/rules/common-pitfalls.md)）。

xtask 的 `_build{Wasm,Ios,Android}Testhost` 把**语料组装步**委托 z42b；**native 平台构建**
（wasm-pack / xcframework / cargo-ndk）与**设备 RUN 交接**（Playwright / xcodebuild-sim / gradle）
**保持在 xtask**——前者是 z42b `build`/`export` 的边界，后者是外部触发（CI-gated）。

### RID 值域

`host`（默认，in-process）｜ `browser-wasm` ｜ `ios-arm64` ｜ `iossim-arm64` ｜ `android-arm64` ｜
`android-x64`。设备 RID 需 `--out` + `--agent`；未知 RID 报错列出合法值。

## Deferred / Future Work

### test-pipeline-future-device-run: z42b 接管设备端实际 RUN（Slice 3）

- **来源**：wire-z42b-embedded-test（②b）
- **触发原因**：设备 RUN 编排（驱动 Playwright / xcodebuild-sim / gradle）连 xtask 目前也只是
  CI-gated 外部触发，非「搬迁已有逻辑」而是新建；本轮聚焦 host 执行 + 设备组装。
- **前置依赖**：设备 RUN 的统一抽象（跨 wasm/ios/android 的 deploy+run+回读报告）。
- **触发条件**：需要 z42b 单命令完成「设备 deploy→run→收报告」时。
- **当前 workaround**：xtask 打印 RUN 交接命令；CI 的 wasm/ios/android job 执行实际 RUN。
