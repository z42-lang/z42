# workflow/testing/platform-tests.md

本地从零跑 **wasm / iOS / Android** 平台测试（R1–R7 嵌入契约）。框架设计见
[`docs/design/testing/cross-platform-testing.md`](../../design/testing/cross-platform-testing.md)；
CI 接线见 [`../ci.md`](../ci.md)。

统一入口（接口驱动框架，add-platform-test-pipeline）。xtask 是原生 apphost，**直接跑**：

```bash
./xtask test platform <wasm|ios|android|all> [build|assets|run]
```

三阶段，省略 step = 全跑 `build → assets → run`：

| step | 做什么 |
|------|--------|
| `build` | ① 构建平台原生工程（wasm-pack / xcframework / AAR）|
| `assets` | ② 编 R1–R7 fixtures→`.zbc` + 收 stdlib zpkg 进平台 bundle（+wasm `files.json`）|
| `run` | ③ 跑测试（Playwright / `swift test` / emulator）|

---

## Step 0 — 基础工具链 + apphost（一次性，所有平台共用）

平台测试依赖**已构建的 z42 工具链**（编译器 + VM + stdlib），并把 xtask 编成原生
apphost `./xtask`。从零：

```bash
# 1. z42c 编译器（z42 自举）+ Rust VM（release）
./xtask build compiler
cargo build --release --manifest-path src/runtime/Cargo.toml

# 2. xtask.zpkg + 原生 apphost ./xtask
Z42_LIBS="$PWD/.z42/libs" z42c build scripts/xtask.z42.toml --release
z42 publish scripts/xtask.z42.toml              # 读 [platform.desktop] → 仓库根 ./xtask

# 3. stdlib dist（index.json 等）
./xtask build stdlib
```

> apphost 机制 + 完整 build 配方见 [`../building/README.md`](../building/README.md)。
> `./xtask` 已 gitignore（原生、平台相关、重生不提交）。

### 冷启动（连 `z42` launcher 都没有）

apphost 由 `z42 publish` 产出，需要 launcher。若完全没有 z42，用构建出的
`z42vm` 直跑 `xtask.zpkg`，指好 stdlib，先把 stdlib + apphost 备出来：

```bash
vm="$PWD/artifacts/build/runtime/release/z42vm"
libs="$PWD/artifacts/build/libraries/dist/release"
Z42_LIBS="$libs" z42c build scripts/xtask.z42.toml --release
Z42_PORTABLE_VM="$vm" Z42_LIBS="$libs" "$vm" artifacts/xtask/xtask.zpkg -- build stdlib
# 此后即可 z42 publish → ./xtask，或继续用 "$vm" artifacts/xtask/xtask.zpkg -- <cmd> 形式
```

---

## wasm（最易，无需模拟器）

**额外前置**：`wasm-pack` + wasm32 target + 本地 node（Playwright 用）。一次性：

```bash
./xtask deps install --os wasm   # wasm-pack + wasm32-unknown-unknown + 本地 Node LTS（wasm 必备）
```

跑：

```bash
./xtask test platform wasm
# ① wasm-pack build web+nodejs → ② fixtures+stdlib+files.json
# → ③ npm install + playwright install chromium（首次下载 ~280MB）+ R1–R7
```

> 没装本地 node 也行：框架会回退到 PATH 上的 `node`。
> JUnit 输出：`artifacts/test-reports/wasm/junit.xml`。

---

## iOS（需 macOS + Xcode）

**额外前置**：Xcode（`xcodebuild`，自行安装）+ Rust iOS + host slice target。

```bash
./xtask deps install --os ios    # aarch64-apple-ios{,-sim} + aarch64-apple-darwin
```

跑（③ = **真 iOS Simulator** via `xcodebuild test`）：

```bash
./xtask test platform ios
# ① cargo×targets + xcframework（含 ios-device/sim/macos slice）
# → ② fixtures+stdlib 进 Tests bundle
# → ③ xcodebuild test 在 iOS Simulator 跑 R1–R7（IosBackend 自动选 simctl 第一个可用 iPhone）
```

> 指定模拟器：`Z42_IOS_DEST='id=<udid>'`（或 `platform=iOS Simulator,name=...`）覆盖默认。
> JUnit（解析 xcodebuild 的 Test Case 行）→ `artifacts/test-reports/ios/junit.xml`，无需 xcbeautify。
>
> iOS cargo build 自动带 `IPHONEOS_DEPLOYMENT_TARGET=platform.ios.min_ios`
> （否则 zlib-ng C 部署目标与 rlib 不匹配 → `___chkstk_darwin` 链接失败）。

---

## Android（需 NDK + emulator）

**额外前置**：`cargo-ndk` + NDK + android target + JDK 17。一次性：

```bash
./xtask deps install --os android       # cargo-ndk + NDK + rust android targets（build tier 必备）
# emulator tier（emulator + system-image + AVD + Gradle，~4 GB / 10-15 min）没有命令：
# `test platform android` 的 run 步骤检测缺失后自动安装（用到才装）
```

跑（③ 桥接 `platforms/android/test.sh`，自动起 emulator）：

```bash
eval "$(./xtask deps env)"   # 设 ANDROID_NDK_HOME
./xtask test platform android
# ① cargo-ndk×ABIs + gradle AAR → ② fixtures+stdlib 进 assets
# → ③ test.sh：起 emulator + gradlew :z42vm:connectedAndroidTest（R1–R7）
```

---

## 三平台一把跑

```bash
./xtask test platform all     # wasm → ios → android（首失败即停）
```

> 仅当本机三套工具链齐备时有意义；缺哪个平台先单独跑。

## 与主 GREEN gate 的关系

平台测试**不在** `./xtask test`（host 6 stages）内——它们需各自的重型工具链，
按需单独跑。CI 各平台独立 job 跑（见 [`../ci.md`](../ci.md)），结果以 GitHub Check 呈现。

## 嵌入 smoke corpus（移动/wasm 跑多少用例）

`test embedded --rid <wasm|ios|android>` 把 src/tests goldens + stdlib `[Test]` 汇成一个 bundle
穿过嵌入 VM 跑。受限平台上完整 corpus（~500 例）太慢（曾超时 / 撞 job 时限），故取
**smoke 子集（cap=60）**——定位是「验嵌入执行路径通不通」，非全覆盖（全覆盖是 desktop：不 cap）。
子集按**类别 round-robin 采样**（tidy-test-system），每个类别都有代表 case，不再偏向字母序靠前的类。
缺目标平台能力（socket/threads/native-fs）的用例整例排除。机制详见
[`../../design/testing/embedded-app-run.md` §5.7](../../design/testing/embedded-app-run.md)；
用 `xtask test list --rid <rid>` 可查哪些用例会被排除。
