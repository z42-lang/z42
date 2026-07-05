# Android facade — 嵌入 z42 到 Android

> 🟢 已落地 · facade [`android/platform/`](../../../src/toolchain/workload/android/platform/) · spec [`2026-05-12-add-platform-android/`](../../spec/archive/2026-05-12-add-platform-android/)

把 z42 VM 编进 `z42vm.aar`，让 Kotlin / Compose app 引入后 `import io.z42.vm.Z42VM` 跑 `.zbc`。统一三段：**① Host 环境准备 → ② 编译（facade + 嵌入 app）→ ③ 运行测试用例**。

## 1. Host 环境准备

### 1.1 平台工具链（一次性）

```bash
# JDK 17+
brew install --cask temurin              # macOS；其他平台用 SDKMAN / apt
java -version                            # 验证

# Rust android targets + cargo-ndk
# 32-bit ABI (armv7 / x86) 已退场；见 memory project_supported_platforms。
rustup target add aarch64-linux-android x86_64-linux-android
cargo install cargo-ndk
```

SDK + NDK（含 cmdline-tools / build-tools / platform-34）二选一：

**(推荐) 仓库内一键装** —— 版本由 [`versions.toml`](../../../versions.toml) `[build.android]` pin，落到 `artifacts/tools/android-sdk`，**不污染系统**：

```bash
./xtask deps install --os android        # SDK + NDK（build tier；加 --emulator 连模拟器一并装）
```

直接下载的产物（cmdline-tools / gradle）下载后按 versions.toml 的 `sha256` pin 校验，不符即中止安装；NDK + system-image 走 sdkmanager（对 Google repository manifest 自校验）。

装好后**无需任何环境变量**：`test platform android build` 自动从 `artifacts/tools/android-sdk` 解析 SDK + NDK（`AndroidBackend._resolveSdk` / `_resolveNdk` 给 cargo-ndk + gradle 注入 `ANDROID_HOME` / `ANDROID_SDK_ROOT` / `ANDROID_NDK_HOME` / `ANDROID_NDK` / `ANDROID_NDK_ROOT`）。

**(替代) 用现成的 Android Studio SDK** —— 显式指向你的安装（这些 env 优先于仓库内的）：

```bash
export ANDROID_HOME="$HOME/Library/Android/sdk"             # macOS 默认；含 SDK Platform 34 + Build-Tools 34
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/26.3.11579264"   # 替换为实际 r26+ 版本
export PATH="$PATH:$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools"
```

❗ 两种都没配 → gradle 报 `SDK location not found`、cargo-ndk 链接失败。
❗ NDK 版本与 cargo-ndk 不匹配 → 升级 NDK 到 r26+。
ℹ️ cargo-ndk 的 C 依赖（zlib-ng 经 z42.compression）走 CMake 内建 Android 工具链 + 默认 Unix Makefiles 生成器（`make`，Xcode CLT / build-essential 自带）——**不需要 Ninja**。关键是 cmake 靠 `ANDROID_NDK` / `ANDROID_NDK_ROOT`（非 `ANDROID_NDK_HOME`）定位 NDK，backend 已一并注入。

### 1.2 z42 工具链（编译器 + stdlib，一次性 / 改 stdlib 后重跑）

facade 会把 stdlib zpkg 打进 assets，故先备好 z42 工具链：

```bash
./xtask build compiler      # z42c 自举（或由 ./scripts/install-z42.sh 直接提供）
./xtask build stdlib
```

✅ 产出 `artifacts/build/compiler/z42c.driver/release/dist/z42c.driver.zpkg` + stdlib zpkg 到 `artifacts/build/libraries/dist/release/*.zpkg`。
❗ `error: z42c not built` → 先 `./scripts/install-z42.sh` 或 `./xtask build compiler`。

## 2. 编译

### 2.1 编 facade

```bash
./xtask test platform android build
```

`test platform android build`（AndroidBackend）内部串接：`cargo ndk -t arm64-v8a -t x86_64 build --release` + `./gradlew :z42vm:assembleRelease`。

✅ 产物：
- `z42vm/build/outputs/aar/z42vm-release.aar`
- `z42vm/src/main/jniLibs/{arm64-v8a,x86_64}/libz42_platform_android.so`
- `z42vm/src/main/assets/stdlib/*.zpkg`（22 个，从 `artifacts/build/libraries/dist/release/` 拷入）

❗ `error: linker not found for aarch64-linux-android` → `ANDROID_NDK_HOME` 错或 NDK 版本旧。
❗ `Could not resolve all dependencies` (Gradle) → JDK < 17 或不在 PATH。

### 2.2 嵌入到 app

`app/build.gradle.kts`：

```kotlin
dependencies {
    implementation(files("path/to/z42vm-release.aar"))
    // 或 release 后 Maven：
    // implementation("io.z42:z42vm:0.1.0")
}
```

Kotlin 用法：

```kotlin
import io.z42.vm.Z42VM
import io.z42.vm.AssetZpkgResolver

Z42VM(zpkgResolver = AssetZpkgResolver(assets)).use { vm ->
    vm.stdoutHandler = { bytes -> textView.append(String(bytes)) }
    val m = vm.loadZbc(assets.open("hello.zbc").readBytes())
    val e = vm.resolveEntry(m, "App.Main")
    vm.invoke(e)
}
```

❗ `UnsatisfiedLinkError: dlopen failed: library "libz42_platform_android.so" not found` → 缺 ABI 的 `.so`；检查 `jniLibs/<abi>/` 与设备/模拟器 ABI 是否匹配。

## 3. 运行测试用例

R1–R7 嵌入契约测试，跑在**真 Android emulator** 上：

```bash
eval "$(./xtask deps install --os android --print-env)"   # 设 ANDROID_NDK_HOME / ANDROID_HOME 等
./xtask test platform android
# ① cargo-ndk×ABIs + gradle AAR → ② fixtures+stdlib 进 assets
# → ③ test.sh：起 emulator + gradlew :z42vm:connectedAndroidTest（R1–R7）
```

完整三阶段（build / assets / run）+ emulator 前置见 [`../testing/platform-tests.md`](../testing/platform-tests.md)。

> 🚧 **占位**：app 级 demo（独立可跑的示例 app）+ JUnit 报告 + CI 接线推迟到独立 spec（`add-platform-android-demo` / `-tests` / `-ci`），后续补。

## See also

- **本地打 per-ABI SDK package**（自包含 `kotlin/` + `cpp/` + `native/libz42_platform_android.{a,so}`）：[`../packaging.md`](../packaging.md) — `./xtask package release --rid android-arm64 / android-x64`
- Kotlin API + 错误码（spec 落地后补）：`android/README.md`
- 跨平台契约：[`platform-contract.md`](../../../src/toolchain/workload/platform-contract.md)
- 设计 + 决策：[spec](../../spec/archive/2026-05-12-add-platform-android/)
