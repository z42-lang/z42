# 平台构建与嵌入

> 对齐：2026-09-17（change `restructure-docs-three-books`）｜ 代码：
> `src/toolchain/workload/{wasm,ios,android,desktop}/platform/`、
> `src/toolchain/workload/platform-contract.md`、
> `scripts/test/xtask_test_{platform,wasm,ios,android,desktop}.z42`、
> `scripts/install/xtask_install{,_android}.z42`、`versions.toml`
>
> 嵌入宿主的**契约**（C ABI、三层架构）见[嵌入宿主](../runtime/embedding.md)与
> [参考手册 · C ABI](../../../reference/src/embedding/c-abi.md)；打发行包见[打包与发版](release.md)。

把 z42 VM 编进浏览器 / iOS / Android 宿主，并在那些宿主上跑 R1–R7 嵌入契约测试。三个平台统一
三段结构：**① host 环境准备 → ② 编 facade → ③ 跑测试**。要读这页的场景：改
`src/toolchain/workload/` 下的 facade、或者 CI 某个平台 job 红了要在本地复现。

## 0. 三平台共同的前置

平台测试消费**已构建的 z42 工具链**（编译器 + VM + stdlib）——facade 要把 stdlib zpkg 收进
自己的 bundle（wasm 的 `js/stdlib/`、iOS 的 `Resources/stdlib/`、Android 的 `assets/stdlib/`）。

```bash
./xtask build compiler     # 或由 ./scripts/install-z42.sh 直接提供
./xtask build stdlib
cargo build --release --manifest-path src/runtime/Cargo.toml
```

产出 `artifacts/build/compiler/z42c.driver/release/dist/z42c.driver.zpkg` +
`artifacts/build/libraries/dist/release/*.zpkg`。报 `error: z42c not built` 就是这一步没做。

统一入口是三段式的：

```bash
./xtask test platform <desktop|wasm|ios|android|all> [build|assets|run]
```

| step | 做什么 |
|---|---|
| `build` | 构建平台原生工程（apphost / wasm-pack / xcframework / AAR） |
| `assets` | 编 R1–R7 fixture → `.zbc`，收 stdlib zpkg 进平台 bundle（wasm 还写 `files.json`） |
| `run` | 跑测试（C ABI harness / Playwright / `xcodebuild test` / emulator） |

省略 step = `build → assets → run` 全跑。`test platform all` 按 desktop → wasm → ios → android
顺序跑，首失败即停——只有本机四套工具链齐备时才有意义。

## 1. wasm

最容易的一个：不需要模拟器。

```bash
./xtask deps install --os wasm     # wasm-pack + wasm32-unknown-unknown + 本地 Node LTS
./xtask test platform wasm         # 三段全跑
```

`deps install --os wasm` 把 Node 装到 `artifacts/tools/node`（版本由 `versions.toml`
`[toolchain.node].version` 钉住）；PATH 上已有满足 `min_version` 的 node 也行，两者皆缺时
测试步骤会自动装。Rust 1.88+ 装 `wasm-pack` 必须带 `--locked`，否则 `cargo-platform` 版本冲突。

分段跑：

```bash
./xtask test platform wasm build     # wasm-pack web + nodejs → pkg-web/ pkg-nodejs/
./xtask test platform wasm assets    # fixtures + stdlib + files.json
```

`run` 段做 `npm install` + `playwright install chromium`（首次约 280 MB）再跑 R1–R7，
JUnit 落 `artifacts/test-reports/wasm/junit.xml`。

`pkg-web/`（浏览器）与 `pkg-nodejs/`（Node）是标准 wasm-bindgen npm 包，宿主 `import` 后加载
`.zbc` + `js/stdlib/`。JS / TS API 与错误码见 `src/toolchain/workload/wasm/README.md`。
可跑的最小示例在 `src/toolchain/workload/wasm/platform/demo/`：浏览器 demo 需要一个会正确
发送 `application/wasm` 与 `text/javascript` MIME 的静态服务器（`miniserve` 或
`python3 -m http.server`），**必须走 HTTP**——`file://` 直接打开会因 CORS + 不能 fetch wasm 而失败。

| 症状 | 原因 |
|---|---|
| `wasm-pack: command not found` | 前置没装 |
| `fixture missing: hello.zbc` / `stdlib libs dir not found` | `assets` 段没跑，或 §0 没做 |
| `Z42VMError: undefined function ...` | `js/stdlib/` 是空的，重跑 `build` + `assets` |
| DevTools 报 `Failed to load module` / MIME 错 | 服务器没给 `.wasm` 送 `application/wasm` |

## 2. iOS

只能在 **macOS host** 上编。

```bash
sudo xcodebuild -license accept && xcode-select --install
xcode-select -p                    # 应输出 .../Xcode.app/Contents/Developer
./xtask deps install --os ios      # aarch64-apple-ios{,-sim} + aarch64-apple-darwin
./xtask test platform ios
```

`build` 段串接三个 target 的 `cargo build` + `xcodebuild -create-xcframework`，产出
`Z42VM.xcframework/`（含 `ios-arm64/` 与 `ios-arm64_x86_64-simulator/` slice）+
`Resources/stdlib/*.zpkg`。`run` 段用 `xcodebuild test` 在**真 iOS Simulator** 上跑 R1–R7，
默认取 `simctl` 的第一个可用 iPhone；`Z42_IOS_DEST='id=<udid>'`（或
`platform=iOS Simulator,name=...`）覆盖。JUnit 从 `xcodebuild` 的 Test Case 行解析，
落 `artifacts/test-reports/ios/junit.xml`，不需要 xcbeautify。

iOS 的 cargo build 自动带上 `IPHONEOS_DEPLOYMENT_TARGET=platform.ios.min_ios`（`versions.toml`），
否则 zlib-ng 的 C 部署目标与 rlib 不匹配 → `___chkstk_darwin` 链接失败。

宿主侧用 SwiftPM 引：

```swift
.package(path: "<repo>/src/toolchain/workload/ios/platform"),
// target deps: .product(name: "Z42VM", package: "Z42VM")
```

```swift
import Z42VM
let vm = try Z42VM(zpkgResolver: BundleZpkgResolver())
vm.stdoutHandler = { bytes in textArea.append(String(decoding: bytes, as: UTF8.self)) }
let m = try vm.loadZbc(Data(contentsOf: zbcURL))
_ = try vm.invoke(try vm.resolveEntry(m, fqn: "App.Main"))
```

`BundleZpkgResolver` 是 `Z42VM.init` 的默认参数（从 main bundle 的 `stdlib` 子目录取），
所以最简用法不用自己传 resolver。Swift API 与错误码见
`src/toolchain/workload/ios/README.md`。

| 症状 | 原因 |
|---|---|
| `xcrun: command not found` | Xcode 未装或 `xcode-select -s` 没指对 |
| `linker not found for aarch64-apple-ios` | rustup target 漏装 |
| `dyld: Library not loaded` | xcframework slice 选错（真机用了 simulator slice 或反之） |

## 3. Android

```bash
# JDK 17+ 与 rust target
rustup target add aarch64-linux-android x86_64-linux-android
cargo install cargo-ndk

./xtask deps install --os android   # SDK + NDK → artifacts/tools/android-sdk，不污染系统
eval "$(./xtask deps env)"          # 导出 ANDROID_NDK_HOME 等
./xtask test platform android
```

32 位 ABI（armv7 / x86）不在支持面内。SDK / NDK 的版本由 `versions.toml` `[build.android]`
钉住；直接下载的产物（cmdline-tools / gradle）按其中的 `sha256` 校验，不符即中止，
NDK 与 system-image 走 sdkmanager 自校验。装好之后**不需要任何环境变量**：
`AndroidBackend._resolveSdk` / `_resolveNdk` 会从 `artifacts/tools/android-sdk` 解析，
并给 cargo-ndk 和 gradle 注入 `ANDROID_HOME` / `ANDROID_SDK_ROOT` / `ANDROID_NDK_HOME` /
`ANDROID_NDK` / `ANDROID_NDK_ROOT`（cmake 定位 NDK 靠的是 `ANDROID_NDK` / `ANDROID_NDK_ROOT`
这两个，不是 `ANDROID_NDK_HOME`）。

要用现成的 Android Studio SDK 就显式指过去，这些 env 优先于仓库内的：

```bash
export ANDROID_HOME="$HOME/Library/Android/sdk"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/<与 versions.toml 一致的版本>"
export PATH="$PATH:$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools"
```

`deps install --os android` 只装 build tier；emulator tier（emulator + system-image + AVD +
Gradle，约 4 GB / 10~15 分钟）没有单独命令，`run` 步骤检测到缺失时自动装。`run` 桥接
`src/toolchain/workload/android/platform/test.sh`（起 emulator +
`gradlew :z42vm:connectedAndroidTest`）——这是整条链上仅存的一个 shell 脚本。

`build` 段跑 `cargo ndk -t arm64-v8a -t x86_64 build --release` + `./gradlew :z42vm:assembleRelease`，
产出 `z42vm-release.aar`、`jniLibs/{arm64-v8a,x86_64}/libz42_platform_android.so` 和
`assets/stdlib/*.zpkg`。C 依赖（zlib-ng）走 CMake 内建 Android 工具链 + 默认 Unix Makefiles
生成器，**不需要 Ninja**。

宿主侧：

```kotlin
dependencies { implementation(files("path/to/z42vm-release.aar")) }
```

```kotlin
Z42VM(zpkgResolver = AssetZpkgResolver(assets)).use { vm ->
    vm.stdoutHandler = { bytes -> textView.append(String(bytes)) }
    val m = vm.loadZbc(assets.open("hello.zbc").readBytes())
    vm.invoke(vm.resolveEntry(m, "App.Main"))
}
```

与 iOS 不同，Android 的 `zpkgResolver` 是**必填参数**（没有默认值）；resolver 自己吃
`AssetManager`。Kotlin API 见 `src/toolchain/workload/android/README.md`。

| 症状 | 原因 |
|---|---|
| gradle 报 `SDK location not found` / cargo-ndk 链接失败 | 两条 SDK 路径都没配 |
| `error: linker not found for aarch64-linux-android` | NDK 路径错或版本旧 |
| `Could not resolve all dependencies`（Gradle） | JDK < 17 或不在 PATH |
| `UnsatisfiedLinkError: dlopen failed: library "libz42_platform_android.so" not found` | 缺该 ABI 的 `.so`，检查 `jniLibs/<abi>/` 与设备 ABI 是否匹配 |

## 4. desktop

`test platform desktop` 走同一套三段接口，跑的是 Tier-1 C ABI 的 R1–R7：用
`native/include/` 的两个头 + `libz42` 编一个 C 宿主，加载 `.zbc` 执行。它是三个移动/浏览器
平台的**对照组**——同一组契约在没有沙箱限制的 host 上应当全过。

## 5. 跨平台契约

三个 facade 遵循同一份契约（`src/toolchain/workload/platform-contract.md`）：同样的
resolver 抽象、同样的 stdout sink 语义、同样的 R1–R7 用例编号。加一个新平台就是再实现一次
`IPlatformBackend`（`scripts/test/xtask_test_<plat>.z42`）并补齐这三段。

嵌入执行跑在 **16 MB 大栈线程**上（host 侧的 C 符号 `z42_host_run_app`），避免深递归打爆
移动端的小线程栈。哪些用例因平台缺能力被排除、怎么分片跑全覆盖，见
[测试怎么跑 §7](testing.md#7-平台测试与嵌入-corpus)。
