# Tasks — z42b-device-run

> 分阶段（proposal PR 计划）。每阶段 = 一 PR = 独立 GREEN + 自举字节不动。`[ ]` 待做。
> desktop 已 ②b 完成，不在范围；Slice 3 = 纯 device（wasm/ios/android）。骨架由 wasm 承载。

## PR-1：verb 骨架 + wasm 全接管（build+deploy 本地可验）

- [ ] `builder_cli.z42` 的 `z42b test` 加 `--build` / `--run` / `--stage-only` 选项解析
      （`--rid`/`--out`/`--agent` 已存在）。
- [ ] 新增 `builder_device.z42`：per-rid 编排入口 + `IDeviceDriver { Build(); Deploy(); Run()→TestReport }`
      抽象（z42 侧）；`_assembleDeployable`/`_stageDeployable`（deploy 半，②b 已有）复用。
- [ ] TestReport 翻译层：设备 runner 产物（wasm=`window.__report` JSON）→ `Std.Test.TestReport` 统一
      类型 + 渲染 + exit code。
- [ ] **wasm driver**：build（`wasm-pack build --target web`）+ deploy（stage {app,libs,bundle}+
      files.json + 拷 pkg/harness index.html/run.js）+ run（`Process(npx playwright test --config
      playwright.embedded.config.ts)`）+ 回收 `window.__report`→TestReport。
      = 把 `xtask_test_embedded.z42:_buildWasmTesthost` + `WasmBackend.RunTests` 的逻辑下沉。
- [ ] `WasmBackend`（`xtask_test_wasm.z42`）薄化为转调 z42b；`_buildWasmTesthost` 改调 z42b（或先仅
      新增 z42b 能力 + 保留 xtask 过渡，二选一 IMPL 定，倾向缩小 blast radius）。
- [ ] `ci.yml` `test-wasm-browser` job 改调 z42b verb（替换 inline playwright 命令）。
- [ ] GREEN：`xtask test`（勿 export Z42_HOME）全绿 + self-host 3/3 字节不动 + 无格式 bump。
      本地验 wasm build+deploy（若装 wasm-pack/playwright 可 smoke run）；playwright RUN 交 CI。
- [ ] 文档：`test-pipeline.md` 加 verb 扩展 + wasm driver 说明（per-platform 表 PR-2+ 补全）。

## PR-2：ios 全接管 ✅

- [x] ios driver（新 `builder_device_ios.z42`）：build（`_buildIosXcframework` 逻辑下沉：cargo×slices +
      `xcodebuild -create-xcframework`）+ deploy（stage `Resources/embedded`）+ run（`xcodebuild test
      -scheme Z42VM -destination <sim>`；sim UDID 由 `xcrun simctl list devices available` 解析）+ 回收
      报告（解析 `Test Case … passed/failed` → junit.xml，z42b 自写）。
- [x] 保持「一次 sim boot 同跑 R1–R7 + embedded bundle」（design D3）：`--run` = 单个
      `xcodebuild test -scheme Z42VM`，一 scheme 同含 Z42VMTests + Z42EmbeddedTests，一次 boot。
- [x] `IosBackend`（`xtask_test_ios.z42`）薄化：`RunTests` → `_runIosTesthost`（删本地 xcodebuild /
      simUDID / junit）；`_buildIosXcframework` 从 xtask 移入 z42b（`_buildIosTesthost` → z42b `--build`）。
- [x] `ci.yml` `test-ios-sim` 改调 z42b：`test platform ios run` → `test embedded --rid iossim-arm64 --run`。
- [x] 验证：本地 macOS（Xcode 16.4）验 z42b `--build`（xcframework）+ `--run`（sim）；GREEN `xtask test`
      全绿 + self-host 字节不动 + 无格式 bump。CI `test-ios-sim`（macos-15）以 CI 为准。

## PR-3：android 全接管 ✅

- [x] android driver（新 `builder_device_android.z42`）：build（`cargo ndk -t <abi> build --release` →
      jniLibs；NDK+ABI 由 rid 解析）+ deploy（stage embedded corpus 进 `androidTest/assets/embedded`）
      + run（`Process(gradlew :z42vm:connectedAndroidTest)`；emulator 由 CI action / 本地 test.sh 供给）
      + 报告（gradle 自产 junit，z42b 转达 exit code，CI 报告路径不变）。
- [x] `AndroidBackend`（`xtask_test_android.z42`）：embedded build+deploy 下沉 z42b（`_buildAndroidTesthost`
      → z42b `--build`，`cargo-ndk` 内联删除移入 z42b）；CI RUN 走 z42b `--run`。**RunTests 保留
      test.sh** 作本地 emulator-lifecycle 路径——emulator AVD 生命周期仍留 CI action / test.sh（design
      D2 不对称，emulator 供给不进 z42b）。
- [x] `ci.yml` `test-android-emu` 改调 z42b：reactivecircus action 的 `script` 内
      `./gradlew :z42vm:connectedAndroidTest` → `test embedded --rid android-x64 --run`（logcat/tombstone
      诊断包裹保留；`_root()` 用 `git rev-parse` 故 cwd 无关）。
- [ ] 验证：CI `test-android-emu`（Linux+KVM）绿。**本地不可验**（无 NDK/emulator，且 libffi-sys 类
      交叉编译本机环境性挂起）→ 交 CI tier-2（nightly-only，不在 PR 上跑）。

## PR-4：dogfood workload-install + 清理

- [ ] z42b device driver 的 agent 来源：从 in-tree `--agent` 切到「确保 test workload 就位」
      （查已装 → 无则 `z42 workload install test`，design D4）。
- [ ] D4a：CI 离线 workload 源接线（预下载 `z42-workload-nightly-test.tar.gz` 本地源，不每次公网）。
- [ ] 删死代码：xtask 里被 z42b 取代的 `_buildWasmTesthost`/`_buildIosTesthost`/`_buildAndroidTesthost`
      的 build/deploy/run 残留、`IPlatformBackend` 冗余方法体。
- [ ] 文档收口：`test-pipeline.md` per-platform driver 表 + dogfood mermaid；`roadmap.md` flip Slice
      3 完成；归档本 change 到 `docs/spec/archive/`。
- [ ] 验证：CI 设备 job 从已发布 nightly workload 拉 agent 端到端绿 = Change C 发布物闭合。

## 全程铁律（每 PR）

- [ ] 本地 `xtask test` 全绿（勿 export Z42_HOME）+ self-host 3/3 字节不动 + 无 zbc/zpkg 格式 bump。
- [ ] z42b 源新用 stdlib API 前 grep 自查两-nightly 纪律（bootstrap-seed.md 轴②）。
- [ ] 设备 RUN 正确性以 CI tier-2 job 为准（本地不可全验）。
- [ ] 每 PR 合并前并入 main 最新改动 + 重跑 GREEN；合并后删分支/worktree。
