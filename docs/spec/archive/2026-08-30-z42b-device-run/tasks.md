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

## PR-4：dogfood workload + 清理 ✅

- [x] z42b device driver 的 agent 来源：从 in-tree `--agent` **彻底切换**到从 z42b 自己 SDK 解析已装
      `test` workload（`_ensureAgent`：扫 `<home>/runtimes/*/workloads/test/z42.testagent.zpkg`，缺则
      `<home>/z42 workload install test --from $Z42_WORKLOAD_SRC`）。`--agent` 已全删（z42b 三 driver +
      builder_cli 选项 + xtask harness plumbing）。
- [x] D4a（pivot，User 2026-08-30）：CI 设备 job 是 dev-tree 无 launcher → **不下载 archive**，改
      「编译时输出 agent 到 z42b SDK home workload 目录」：`package workload test dev` →
      `artifacts/build/runtime/runtimes/dev/workloads/test/z42.testagent.zpkg`，z42b 直接命中（wasm/ios/
      android 三 job 均加此 provision 步）。install-if-missing 保留给真实已装 SDK。
- [x] 删死代码：`_z42bStageDeployable`（PR-2/3 后已无调用者）、`--agent` CLI 选项、device 路径的
      `_ensureTestAgent` 调用（`_ensureTestAgent` 本体保留，`package_test` 仍用）。`IPlatformBackend`
      `RunTests` 薄化在 PR-2/3 已完成（ios→z42b，android 保 test.sh design D2），无 PR-4 新增死代码。
- [x] 文档收口：`test-pipeline.md` 加「test-agent 解析：dogfood」节 + 更新 device 签名（删 `--agent`）；
      `roadmap.md` flip Slice 3 完成；归档本 change 到 `docs/spec/archive/`。
- [x] 验证：本地验 z42b `_ensureAgent` 解析（agent 预置 workload 目录→stage-only 命中；空 home→清晰
      install 提示）+ z42.builder/xtask 编译 + 自举字节不动。设备 RUN 端到端交 CI tier-2（nightly-only,
      本地无 wasm-pack/xcode/ndk）。

## 全程铁律（每 PR）

- [ ] 本地 `xtask test` 全绿（勿 export Z42_HOME）+ self-host 3/3 字节不动 + 无 zbc/zpkg 格式 bump。
- [ ] z42b 源新用 stdlib API 前 grep 自查两-nightly 纪律（bootstrap-seed.md 轴②）。
- [ ] 设备 RUN 正确性以 CI tier-2 job 为准（本地不可全验）。
- [ ] 每 PR 合并前并入 main 最新改动 + 重跑 GREEN；合并后删分支/worktree。
