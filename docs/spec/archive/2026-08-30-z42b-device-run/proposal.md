# z42b-device-run — z42b 接管设备测试 build+deploy+run（统一测试流水线 Slice 3）

> 状态：DRAFT（待 User 确认后进 IMPL）
> 前身：`unify-test-pipeline-z42b`（阶段 ①/②a/②b 已合，见 `archive/2026-08-29-unify-test-pipeline-z42b`）
> 归档设计权威条款：D5「阶段 2」、D2、D6。

## 问题

统一测试流水线的两层模型（**z42b = 单目标执行器 / xtask = 语料·fleet 编排器**）已立起，
但**单目标的「怎么在设备上 build / deploy / run 这一份」仍散在 xtask + 四平台原生壳里**：

- 设备 **build**：`scripts/test/xtask_test_embedded.z42` 的 `_buildIosTesthost` / `_buildAndroidTesthost`
  / `_buildWasmTesthost` 各自调 `cargo build`+`xcodebuild -create-xcframework` / `cargo ndk` /
  `wasm-pack`。
- 设备 **deploy**：②b 已把 `{app,libs,bundle}` 组装下沉 z42b（`z42b test --rid <dev> --out --agent`
  → `_assembleDeployable`）；但把 deployable 摆进 `Tests/.../Resources`、`androidTest/assets`、
  servable dir 的落点仍在 xtask。
- 设备 **run**：**完全在 xtask + CI YAML + 原生壳里**——`IPlatformBackend.RunTests`
  （`xtask_test_platform.z42` 的 Ios/Android/Wasm backend）分别 spawn `xcodebuild test` /
  `gradlew connectedAndroidTest` / `playwright test`；CI 设备 job 里也重复一套等价命令；三个原生壳
  （`Z42EmbeddedTests.swift` / `Z42EmbeddedInstrumentedTest.kt` / `run.js`）各自调 `z42_host_run_app`
  加载 agent、回读 report、断言 `failed==0`。

> **desktop 不在本 change 范围（②b 已完成）**：`xtask_test_embedded.z42:839-858` 的 desktop embedded
> bundle 路径**早已下沉 z42b in-process**（`z42b test --rid host`，②b），删了 desktop 原生 testhost
> spawn。desktop 上唯一剩的原生路径是 `DesktopBackend` 的 R1–R7**嵌入契约测试**（cc r1_r7.c +
> libz42.a，`xtask_test_desktop.z42`）——那测的是 C ABI 嵌入契约、非测试语料 RUN，**不在 Slice 3
> 范围**。故 Slice 3 = 纯 **device（wasm/ios/android）** 的 build+run 接管，无 desktop 部分。

**后果**：四平台各有一套 bespoke build/deploy/run 逻辑 + 报告回收，互相重复；xtask 的 embedded
harness 与 platform backend 两条线对同一件事（跑设备测试）各写一遍；`z42b test --rid` 只做了组装
一半，RUN 半边悬空。这正是归档设计 **D5「阶段 2」** 点名要收敛的返工面。

## 目标

**把单目标的设备 build + deploy + run 全部下沉到 `z42b test --rid <device>`，`IPlatformBackend`
各 backend 收缩为「声明 rid + 转调 z42b + 翻译报告」的薄壳，消除四平台重复。** xtask 只保留语料级
编排（发现 / 编译 / 分片 / 聚合 / 门禁）。设备上的 test-agent 通过 **dogfood `z42 workload install
test`** 就位（消费 Change C 刚发布的 `z42-workload-nightly-test.tar.gz`），真正端到端验证 test
workload 发布物。

### 已裁决（User 2026-08-29，勿重问）

1. **接管范围 = build + deploy + run 全下沉**（不是仅 RUN）。native build（xcframework / cargo-ndk
   / wasm-pack）也移入 z42b；`IPlatformBackend` 各 backend 彻底薄化。对齐归档 D5 阶段 2 字面。
2. **agent 就位 = dogfood `z42 workload install test`**：z42b 设备 RUN 通过 workload-install 拉取
   已发布 test workload 取 agent，而非沿用 xtask 传入的 in-tree `--agent`。这也解释了「等 nightly
   发布」的 gate——Change C 的 `z42-workload-nightly-test.tar.gz` 现已发布，gate 满足。

## 非目标

- **不动语料级编排**：bundle 发现 / 编译 / 分片 / 聚合 / 门禁仍留 xtask（D1）。
- **不动 agent 的执行契约**（`Main: <target> [format] [out-path]`、bundle→report）与共享核
  `Std.Test.BundleRunner`——只改「谁把 agent 送上设备、谁触发、谁收报告」。
- **不做 Slice「设备实际交互 RUN」以外的新平台**：范围锁 wasm / ios(-sim) / android(-emu) +
  desktop（现有四平台），不新增。
- **不碰 z42vm 原生绑定 `z42_host_run_app`**：原生壳仍靠它加载 agent；本 change 只重排 host 侧编排。

## 约束 / 风险

- **本质约束**：z42b 跑在 z42vm 上，**不能**成为模拟器 / 浏览器*内部*的 runner。「z42b 接管 RUN」
  = z42b 在 host 侧 spawn 原生驱动（xcodebuild / gradlew / playwright）启动设备、注入 deployable、
  回读 report——把现在散在 xtask backend + CI YAML 的这层编排收敛进 z42b 一个 verb。
- **本地不可全验**：ios-sim（需 macOS+Xcode）/ android-emu（需 KVM）/ wasm-browser（需 playwright）
  的实际 RUN **CI-gated**（tier-2，仅 schedule/dispatch）。本地只能验 desktop 路径 + 各 verb 的
  参数校验 / 组装 / dry-run。**设备 RUN 正确性以 CI 为准**（bootstrap-seed.md 阶段 8 GREEN 判定）。
- **两-nightly 自举纪律**：z42b 源若新用 stdlib API（如 workload-install 的编程接口），受
  bootstrap-seed.md 轴② 约束——用它前该 API 须已随一个 nightly 发布。IMPL 时 grep 自查。
- **dogfood workload-install 的 CI 代价**：设备 job 增加一次 workload 拉取（网络 + 解压）。需确认
  CI 能离线 / 用本地已发布 archive 而非每次打网络（见 design D4 open）。
- **大范围重构风险**：四平台 × (build+deploy+run) 一次全搬 = 不可评审的 mega-PR。**必须分阶段**
  （见下 PR 计划），每阶段独立 GREEN + 自举字节不动。

## 分阶段 PR 计划（先来后到，每阶段独立 GREEN + PR）

> 目的：把「四平台 × 三职责」拆成可评审、可 CI 验证的增量；desktop 先行（本地可全验）打通 verb 骨架，
> 设备平台逐个接入，最后收敛 backend + CI + dogfood。

> 事实校正（2026-08-29）：desktop embedded 已 ②b 下沉 z42b in-process，**无 desktop 接管**。骨架改由
> **wasm** 承载（其 deployable 已 100% z42b + files.json，native build 最简 wasm-pack，build+deploy
> 本地可验，仅 playwright RUN 上 CI）。计划 5→4 个 PR。

- **PR-1（verb 骨架 + wasm 全接管；build+deploy 本地可验）**：给 `z42b test --rid` 补 `--build`/
  `--run`（在既有 `--out` 组装之上加「native build 触发 + RUN 触发 + 收报告」），定义统一
  `IDeviceDriver { Build; Deploy; Run→TestReport }` 抽象 + 「设备 RUN 产物 → `Std.Test.TestReport`」
  翻译层，首个实现 = **wasm driver**：build（`wasm-pack build --target web`）+ deploy（stage
  {app,libs,bundle}+files.json + pkg/harness）+ run（spawn `playwright test`，回读 `window.__report`）。
  `WasmBackend`（`xtask_test_wasm.z42`）薄化为转调 z42b；CI `test-wasm-browser` 改调 z42b verb。
  本地验 build+deploy；playwright RUN 交 Linux CI。
- **PR-2（ios 全接管）**：xcframework build + Resources deploy + `xcodebuild test` run + 回读报告
  下沉 z42b；`IosBackend` 薄化；CI `test-ios-sim` 改调 z42b。macOS 本地部分可验 + CI。
- **PR-3（android 全接管）**：cargo-ndk build + assets deploy + emulator/`connectedAndroidTest` run
  下沉 z42b；`AndroidBackend` 薄化（emulator AVD 生命周期仍留 CI action `reactivecircus/...`，
  z42b 只管 build+deploy+触发 gradle）；CI `test-android-emu` 改调 z42b。
- **PR-4（dogfood workload-install + 清理）**：设备 RUN 的 agent 来源从 in-tree `--agent` 切到
  `z42 workload install test`（离线本地 archive 源，不每次打公网）；删除 xtask 里遗留的 build/deploy/run
  死代码、`IPlatformBackend` 冗余；文档收口（`test-pipeline.md` 机制页 + roadmap flip）。

> 每个 PR 都是「一逻辑单元」，独立 `type(scope): 描述` 提交、独立 GREEN、独立开 PR（合并前并入 main
> 最新改动 + 重跑 GREEN）。PR-1 打通骨架后 PR-2/3 可并行（不同平台文件，语义耦合低），PR-4 收尾。

## 验证策略

- 每 PR：本地 `xtask test` 全绿 + 自举 self-host 3/3 字节不动 + 无 zbc/zpkg 格式 bump。
- 各 driver 的 build+deploy 本地可验（wasm-pack / xcframework(macOS) / stage）；设备 RUN 交对应 CI
  tier-2 job（PR 里手动 `workflow_dispatch` 触发一次确认绿，或依赖 nightly sweep）。
- PR-4 的 dogfood：CI 设备 job 从已发布 nightly workload 拉 agent 跑通 = Change C 发布物端到端闭合。
