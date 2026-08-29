# Design — z42b-device-run

> 承接归档 `unify-test-pipeline-z42b` 的 D5「阶段 2」。本文件定 z42b 接管设备 build+deploy+run 的
> 架构与决策；实现分阶段（见 proposal.md PR 计划 + tasks.md）。

## 现状锚点（3ee4567f，正确树；勿信旧树探查）

| 职责 | 现 owner | 落点 |
|------|---------|------|
| 语料 发现/编译/分片/聚合 | xtask（保留） | `xtask_test_embedded.z42:_buildTestBundle` |
| `{app,libs,bundle}` 组装 | **z42b**（②b 已下沉） | `builder_test.z42:_assembleDeployable`→`_stageDeployable` |
| deployable 摆位（Resources/assets/servable dir） | xtask | `_buildIosTesthost:686` / `_buildAndroidTesthost` / `_buildWasmTesthost:583` |
| native build（xcframework/ndk/wasm-pack） | xtask | `_buildIosXcframework:637` 等 |
| 设备 run 触发 | **xtask backend + CI YAML** | `xtask_test_platform.z42` 的 `IosBackend/AndroidBackend/WasmBackend.RunTests`；`ci.yml` device jobs |
| agent 加载 + 报告回读 | 原生壳 | `Z42EmbeddedTests.swift` / `.kt` / `run.js` 调 `z42_host_run_app`，读 out-path 文件 |

**Slice 3 要搬的 = 中间四行**（deployable 摆位 + native build + run 触发 + host 侧报告回收）
下沉 z42b；语料编排（第 1 行）与原生壳内的 `z42_host_run_app` 加载（第 6 行 VM 侧）不动。

> **desktop 已出局（②b）**：`_testEmbedded:839-858` 的 desktop embedded bundle 路径已委托
> `z42b test --rid host`（in-process BundleRunner），无原生 testhost spawn。desktop 仅剩
> `DesktopBackend` 的 R1–R7 嵌入契约测试（cc r1_r7.c，测 C ABI，非语料 RUN）——不在本 change。
> 故 Slice 3 = 纯 device（wasm/ios/android）。骨架由 **wasm** 承载（deployable 已全 z42b，build 最简）。

## D1：verb 形状 —— `z42b test --rid <device>` 的三段职责合一

②b 已有 `z42b test <manifest> --rid <dev> --out <dir> --agent <zpkg>`（只组装 deployable 到
`--out`）。Slice 3 **在同一 verb 上扩语义**，让「有 rid」时 z42b 走完 build→deploy→run→report 全程：

```
z42b test <manifest.json> --rid <device> [--out <dir>] [--agent <zpkg> | (dogfood)]
           [--build]        # 触发 native build（xcframework/ndk/wasm-pack）
           [--run]          # 触发设备 runner + 回收 report（默认：有 rid 即 run，除非 --stage-only）
           [--stage-only]   # 仅组装 deployable（= ②b 现行为，backward-compat / 调试用）
           [--shard k/n]    # 透传给语料层（xtask 已分片，这里仅定位分片产物）
```

- **决策**：不新增 verb，扩展 `--rid` 分支（对齐归档 D4「统一 verb，两条尾分叉」）。`--stage-only`
  保留 ②b 的纯组装行为（xtask 若还想自己 RUN 的过渡期用；PR-5 清理后可评估删）。
- **默认**：`--rid <device>` 不带 flag = build+deploy+run 全程（最常用）；细粒度 flag 供 CI 分步 /
  调试。
- z42b 内新增 `builder_device.z42`（或扩 `builder_test.z42`）承载 per-rid 的 build/run 编排；
  `builder_test.z42` 的 `_assembleDeployable`/`_stageDeployable`（deploy 半）复用不动。

## D2：per-platform 驱动编排（z42b spawn 原生工具）

z42b 不能进 sim/browser 内部，故「run」= host 侧 spawn 原生驱动。每平台一个 driver 抽象
（z42 侧 struct/接口，非 IPlatformBackend——那是 xtask 的，将薄化）：

| rid | build | deploy | run（z42b spawn） | report 回收 |
|-----|-------|--------|------------------|------------|
| ~~desktop/""~~ | — | — | **已 ②b in-process（`--rid host`），无 driver** | — |
| browser-wasm | `wasm-pack build --target web` | servable dir + files.json（z42b 已产 files.json） | `Process(npx playwright test --config …)` | 读 `window.__report`→ playwright artifact / out 文件 |
| iossim-arm64 | cargo×slices + `xcodebuild -create-xcframework` | 拷进 `Tests/Z42VMTests/Resources/embedded` | `Process(xcodebuild test -scheme Z42VM -destination <sim>)`（sim UDID 由 `simctl list` 解析） | agent 写 `NSTemporaryDirectory()/report.json`→原生壳读→ z42b 从 xcresult / 约定 out 收 |
| android-x64 | `cargo ndk -t <abi> build --release` | 拷进 `z42vm/src/androidTest/assets/embedded` | `Process(gradlew :z42vm:connectedAndroidTest)`（emulator 由 CI action 起，z42b 触发 gradle） | agent 写 `cacheDir/report.json`→ z42b 约定 `adb pull` / out |

- **决策（报告回收统一格式）**：z42b 定义 host 侧 `TestReport`（复用 `Std.Test.TestReport`）为**唯一
  跨平台报告类型**。每平台 driver 负责「把原生 runner 的产物（xcresult / junit / window.__report /
  stdout）翻译成 TestReport」。原生壳内部的「读 out-path 文件断言 failed==0」保持不变（VM 侧），
  z42b 额外从设备把 report.json 取回 host 做统一渲染 / exit code。
- **emulator/sim 生命周期**：`iossim` 的 sim boot 由 `xcodebuild -destination` 隐式管理（z42b 只传
  destination）；android emulator AVD 起停仍由 CI action `reactivecircus/android-emulator-runner`
  持有（port 到 z42 是独立大工程，非本 change——见非目标），z42b 在 emulator 已起的前提下触发
  gradle。这条 **backend 薄化的边界**在 android 上不是 100%：z42b 接管 build+deploy+gradle 触发，
  emulator 供给仍 CI。design 显式记此不对称，避免误以为 android 完全脱离 CI YAML。

## D3：`IPlatformBackend` 薄化终局

`xtask_test_platform.z42` 的 `IPlatformBackend { BuildProject; Assets; RunTests }` 四实现
（Desktop/Wasm/Ios/Android）在本 change 后收缩为：

```
// 薄壳：声明 rid + 转调 z42b + 翻译报告。不再自带 build/deploy/run bespoke 逻辑。
BuildProject(root) → z42b test <manifest> --rid <rid> --build --stage-only
RunTests(root)     → z42b test <manifest> --rid <rid> --run   （回收 z42b 输出的 TestReport）
```

- R1–R7 嵌入契约测试（platform backend 现跑的）与 embedded bundle 测试**合流**：都成「z42b test
  --rid」的输入语料，backend 不再区分两条线。这消除 `xtask_test_embedded.z42` 与
  `xtask_test_platform.z42` 对同一平台各写一遍的重复。
- **风险点**：R1–R7 与 embedded bundle 当前在同一次 sim/emu boot 里跑（`IosBackend.RunTests` 一次
  `xcodebuild test` 同跑两者）。合流时须保持「一次 boot 跑全部」，别拆成两次（boot 昂贵）。z42b
  driver 的 run 须支持「一个 destination 上跑多个 test target」。**IMPL 时重点验**。

## D4：dogfood `z42 workload install test`（agent 就位）

设备 RUN 的 agent 来源从 xtask 传入的 in-tree `--agent` 切到 workload-install：

- z42b driver 在 deploy 前确保 test workload 就位：查本地 SDK `programs/`（或 workload 安装目录）
  是否已有 `z42.testagent.zpkg`；无 → `z42 workload install test`（launcher 命令面已支持，Change C
  已让其可发布）。
- **Open D4a（CI 网络 / 离线）**：CI 设备 job 每次 `workload install test` 打网络拉 nightly archive
  不理想（慢 + 脆）。候选：① CI 预先把已发布 `z42-workload-nightly-test.tar.gz` 下载为本地 archive，
  `workload install test --from <local>`（若 launcher 支持本地源）；② 或 CI 用 `install-z42.sh` 供种
  时一并带上 test workload，z42b 只做「确保就位」不实际联网。**IMPL PR-5 定**——倾向 ①/② 的离线源，
  dogfood 的是「install 机制 + 发布物格式」，不必是「每次真打公网」。
- **Open D4b（本地开发）**：本地 `xtask test` 跑 desktop 不需要 workload（in-tree agent 直用）。
  dogfood 仅施加于**设备 rid + CI**。desktop 保持 in-tree `_ensureTestAgent` 快路径。

## D5：分阶段落地时序（呼应 proposal PR 计划）

- **PR-1**：verb 骨架（`--build/--run/--stage-only` 解析 + `IDeviceDriver` + TestReport 翻译层）+
  **wasm driver** 下沉。本地验 build+deploy；playwright RUN 交 CI。
- **PR-2/3**：ios / android driver 逐个下沉（build+deploy+run），对应 backend 薄化 + CI job 改调。
  可并行（不同平台文件）。
- **PR-4**：dogfood workload-install 切换 + 死代码清理 + 文档。
- 每阶段独立 GREEN + 自举字节不动 + 无格式 bump。z42b 源新用 stdlib API 前 grep 自查两-nightly。

## D6：文档落地（无文档 = 未完成）

- 机制 SoT：`docs/book/src/toolchain/test-pipeline.md`（②b 已建）加「设备 build/deploy/run 由 z42b
  编排」一节 + per-platform driver 表 + dogfood workload 流程图（mermaid）。
- `roadmap.md`：flip `unify-test-pipeline-z42b` 的 Slice 3 / 阶段 2 为完成。
- 各 PR 归档到 `docs/spec/archive/`（末个 PR 合并时统一归档本 change，或每 PR 增量归档——按
  workflow 阶段 9）。

## User 已确认（2026-08-29，勿重问）

1. **PR-1 范围**：~~desktop~~ → **wasm 先行**（desktop 已 ②b 完成，见上）。verb 骨架 + IDeviceDriver +
   TestReport 翻译层 + wasm driver。build+deploy 本地可验。
2. **D2 报告回收 = 双读过渡**：z42b 从设备取回 report.json 做统一渲染 + exit code；**原生壳内断言
   保持不动**（原生壳仍自断言 failed==0 + z42b 另收一份）。缩小 blast radius。
3. **D4a dogfood = 离线本地 archive 源**：CI 预下载已发布 `z42-workload-nightly-test.tar.gz` 作本地源，
   **不每次打公网**。dogfood 的是「install 机制 + 发布物格式」。
4. **android 不对称 = emulator 仍留 CI action**：`reactivecircus/android-emulator-runner` 持有
   emulator AVD 生命周期，z42b 只接管 build+deploy+gradle 触发。port emulator 生命周期是独立工程，
   非本 change。
