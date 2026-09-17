# 测试流水线：两层模型（z42b 执行器 + xtask 编排器）

> 对齐：2026-09-17（change `restructure-docs-three-books`）｜ 代码：`src/toolchain/builder/core/builder_test.z42`、`builder_device*.z42`、`src/libraries/z42.test/src/BundleRunner.z42`、`scripts/test/xtask_test_embedded.z42`、`src/toolchain/workload/test/`
>
> gate 由哪些 stage 组成 → [测试门禁](test-gate.md)；各层怎么单跑、用例放哪 →
> [测试怎么跑](testing.md)。本页写**架构**：一份 test-bundle 是怎么从仓库语料走到设备上的。

z42 把「跑测试」拆成两层，各有单一 owner，**以 bundle manifest 为缝**：

| 层 | 角色 | 职责 |
|---|---|---|
| **z42b** | 单-bundle **执行器** | 编译 → 部署 → 运行**一个**目标 / bundle：`z42b test <target> [--rid <rid>]` |
| **xtask** | 语料 / fleet **编排器** | 仓库语料发现、编译、分片、聚合、门禁 |

缝 = 一份 test-bundle：`manifest.json`（`{cases:[…]}`）+ 可部署布局 `{app, libs, bundle}`。
xtask 组装 bundle，交 z42b 执行（host）或组装设备 deployable（device）。

```
                  ┌──────────── xtask（fleet 编排器）────────────┐
 语料发现/编译/分片 → _buildTestBundle → manifest.json + {app,libs,bundle}
                  └──────────────────────┬───────────────────────┘
                                         │ 缝：bundle manifest / deployable
                  ┌──────────────────────▼───────────────────────┐
                  │            z42b（单-bundle 执行器）            │
 host:   z42b test <manifest.json> --rid host          → BundleRunner.RunBundle（in-process）
 device: z42b test <manifest.json> --rid <dev> --out D → 组装 {app,libs,bundle} deployable
                  └──────────────────────┬───────────────────────┘
                                         │ device deployable
        native build（wasm-pack / xcframework / cargo-ndk）+ 设备 RUN
        （Playwright / xcodebuild-sim / gradle）
```

## 1. 共享执行核：`Std.Test.BundleRunner`

一份 test-bundle 的**执行算法**沉在 stdlib 的 `z42.test`，被两个宿主共用：on-device agent
（`Z42.TestHost.Agent`，嵌入式 z42vm，每个平台同一段字节码）与 host 上的 z42b（in-process，
z42b 本就跑在 z42vm 上、依赖 `z42.test`）。

```
BundleRunner.RunBundle(BundleCase[] cases, string bundleDir, string libsDir) → TestResult[]
    ├── golden case（Entry != null） → ModuleLoader.RunGoldensIsolated（每例独立 VM）+ 比对 stdout/expected
    └── unit case  （Entry == null） → Runner.RunModuleResults（共享 VM）
    → 聚合成一份 TestResult[]（golden 先、unit 后）
```

**关键设计**：`BundleRunner` **不吃 manifest 路径、不解析 JSON**——`z42.test` 是基础库，
`TestReport` 刻意手写 JSON 以**避免依赖 `z42.json`**。因此 `manifest.json` → `BundleCase[]` 的解析
留在**各宿主**（agent 与 z42b 各自本就依赖 `z42.json`，各写一小段 adapter），只**共享算法**
（golden 隔离比对 + unit 跑 + 聚合），I/O glue 按宿主分。这样一份 bundle 在 host 与设备上跑出
**逐字节一致**的报告。

## 2. host 路径：z42b in-process，没有 testhost / agent 进程

`z42b test <manifest.json> --rid host`（或省略 rid）→ `builder_test` 识别 `.json` →
`_runBundleHost`：解析 manifest → `BundleRunner.RunBundle` → 按 `--format`（`pretty` / `json`）渲染
+ 退出码（任一失败 = 1）。golden 仍由 `RunGoldensIsolated` 起独立 VM，隔离语义不变。
xtask 的 `_testEmbedded` desktop 分支因此收缩为「组 bundle → 委托 `z42b test --rid host`」。

> **`--format json` 的契约是 stdout 只有机器可读的那一份。** 构建进度（`compiled: …`）必须改走
> stderr，否则消费方解析不了——实测症状是 xtask 转发 bench 时基线**静默捕获到 0 条**（且不报错）。
> 由 `BuildLog.SetToStderr(format == "json")` 保证。

`z42b test` 的其余目标形态（非 `.json`）：`.zbc` / `.zpkg` 直接当已编译产物跑；否则当项目清单处理
——清单里声明了 `[[test]]` / `[[bench]]` 等 dev target 就**按目标编译 + 运行**（目标在内存里派生
manifest、不落文件，父包身份直达编译器 ⇒ 测试可见父包 internal），没声明则回落到「编译项目自身」。

## 3. device 路径：z42b 接管 build + deploy + run

`z42b test <manifest.json> --rid <device> [--out <dir>] [--build-root <repo>]`（libs 走 `Z42_LIBS`）。
sub-step flag 选粒度，默认无 flag = build + run：

| flag | 语义 |
|---|---|
| `--stage-only` | 仅组装 deployable `<out>/{app,libs,bundle}`（`browser-wasm` 额外产 `files.json`）。枚举前按 ordinal 排序保确定序 |
| `--build` | native 平台构建 + deploy（stage + 平台产物就位）|
| `--run` | 触发设备 runner + 回收报告 |

per-platform driver（`builder_device.z42` 管 wasm；`builder_device_ios.z42`；
`builder_device_android.z42`）：

| rid | build | run（z42b spawn 原生工具）| report |
|---|---|---|---|
| `browser-wasm` | `wasm-pack build --target web` + stage `{app,libs,bundle}` + `files.json` + 拷 pkg/harness | `npx playwright test --config playwright.embedded.config.ts` | playwright 退出码（`run.js` 自断言 `window.__report`）|
| `ios-arm64` / `iossim-arm64` | cargo × slices（host + device/sim）+ `xcodebuild -create-xcframework` + stage embedded 语料进 XCTest `Resources/embedded` | `xcodebuild test -scheme Z42VM -destination <sim>`（sim UDID 由 `xcrun simctl` 解析；**一次 boot 同跑全部**）| 解析 `Test Case … passed/failed` → `artifacts/test-reports/ios/junit.xml` |
| `android-arm64` / `android-x64` | `cargo ndk -t <abi> build --release` → jniLibs（NDK + ABI 由 rid 解析）+ stage 语料进 `androidTest/assets/embedded` | `gradlew :z42vm:connectedAndroidTest`（**一次 emulator run 同跑全部**）| gradle 自产 junit |

合法 RID 值域：`host`（默认，in-process）、`browser-wasm`、`ios-arm64`、`iossim-arm64`、
`android-arm64`、`android-x64`；未知 RID 报错并列出合法值。`--build` / `--run` 需 `--build-root`，
`--build` 另需 `--out`。

xtask 保留**语料发现 / 编译 / 分片**与 native 工具**供给**（node / Xcode / NDK），把单目标的
build/deploy/run 交给 z42b。**android 是有意的不对称**：emulator 的 AVD 生命周期
（boot + 关机）留在 CI action 或本地 `test.sh`，**不进 z42b**——z42b 只管「在一台已经在跑的设备上
构建并执行」，供给设备本身不是它的职责。

## 4. test-agent 的解析：dogfood `test` workload

设备 RID 上运行的 on-device test-agent（`z42.testagent.zpkg`）不是 xtask 树内现编、经参数交给 z42b
的临时产物，而是**已发布 `test` workload 的 payload**（见[打包引擎](packaging.md) §4）。
z42b 从**自己所在的 SDK** 解析它（`_ensureAgent`，`builder_device.z42`）：

```
1. home = Z42_HOME | reverse(Z42_PORTABLE_VM)            # <sdk>/bin/z42vm → <sdk>
2. 扫 <home>/runtimes/<ver>/workloads/test/z42.testagent.zpkg（版本目录排序，任一命中即用）
3. 缺 → spawn `<home>/z42 workload install test [--from $Z42_WORKLOAD_SRC]` 后重扫
```

- **真实已装 SDK**：`z42 workload install test` 已把 agent 放进 workload 目录（步骤 2 命中），
  首次缺失走步骤 3 的 install-if-missing。
- **CI 的设备 job（dev-tree，没有 launcher）**：编译时把现编 agent 直接输出到 z42b 所属 SDK home
  的同一 workload 目录——`package workload test dev` 产出后拷进 `<home>/runtimes/dev/workloads/test/`
  （`<home>` 由 `Z42_PORTABLE_VM` 反推）。步骤 2 直接命中 ⇒ **无需 launcher spawn、无需下载归档**。
  这样 CI dogfood 的是「workload 布局 + z42b 解析」这套机制，用的却是当前源现编的 agent。

## 5. 为什么是这个形状

| 问题 | 取向 |
|---|---|
| 执行算法放哪 | 沉进 stdlib（`z42.test`），**两个宿主共用一份**——否则 host 与设备会各写一套，报告迟早不一致 |
| manifest 解析放哪 | 留在宿主侧。基础库不能为了解析 JSON 而依赖 `z42.json`，而两个宿主本来就依赖它 |
| 设备构建放哪 | z42b。它是「单-bundle 执行器」这个角色的自然延伸；xtask 只做 fleet 级的事 |
| emulator / simulator 的生命周期放哪 | **不在 z42b**（android 尤其明显）。供给一台可用设备是 CI / 本地脚本的事 |
