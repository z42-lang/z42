# 本地打 SDK package

> **面向：** 想在本地产 9 个 per-arch flat SDK package 之一、然后 inspect / smoke test / 给他人使用的开发者。
>
> **不是：** 给 release 自动化看的（CI 走 [`release.md`](release.md) + `.github/workflows/release.yml`）；也不是给 add-ios-tests / add-android-tests in-repo 流程看的（那些走 `platforms/<x>/build.sh` + `test.sh`）。

## 统一入口

```bash
./xtask package release --rid <rid>  # 见下表选 RID
./xtask package debug --rid <rid>    # debug profile（dev 用）
./xtask package release              # 不带 --rid → 自动用 host RID
./xtask --help                       # 完整选项
```

产物落到 `artifacts/packages/z42-<version>-<rid>-<profile>/`。

## RID 矩阵（9 个）

> 严格按 memory `project_supported_platforms` 白名单；不在表中的 RID 直接报错。

| 类别 | RID | 说明 | host 要求 |
|------|-----|------|----------|
| Desktop | `macos-arm64` | Apple silicon Mac | macOS arm64 |
| Desktop | `linux-arm64` | ARM Linux 服务器 / SBC | Linux arm64 |
| Desktop | `linux-x64` | x86_64 Linux | Linux x64 |
| Desktop | `windows-x64` | x86_64 Windows | Windows x64 |
| iOS | `ios-arm64` | iPhone / iPad / Vision Pro 实机 | macOS（任意 arch）|
| iOS | `iossim-arm64` | Apple silicon Mac 上的 iOS 模拟器 | macOS（任意 arch）|
| Android | `android-arm64` | arm64-v8a（主流 Android 设备）| macOS / Linux（任意 arch）|
| Android | `android-x64` | x86_64（emulator + Chromebook）| macOS / Linux |
| wasm | `browser-wasm` | wasm32 + wasm-bindgen + npm | macOS / Linux / Windows |

**Cross-host 限制：**

- Desktop RID 只能在同 RID 的 host 上 build（macos-arm64 不能产 linux-x64）。需要全平台覆盖请走 CI matrix（[release.md](release.md)）。
- iOS RID 只能在 macOS host 上 build（需要 Xcode + xcframework + Apple toolchain）。
- Android / wasm RID 可在 macOS / Linux / Windows host 上 cross-compile（Windows 需走 Android Studio 装 SDK+NDK、Node.js 装 MSI；见 [`building/windows.md`](building/windows.md)）。
- **Windows host 跑这些 `.sh`**：用 Git Bash（Git for Windows 自带）；见 [`building/windows.md`](building/windows.md)。

## 前置工具（一次性）

按你要 build 的 RID 装：

```bash
# 必备（所有 RID）
git --version                 # 拉源 + gh 下载 SDK
cargo --version               # Rust stable；VM
gh --version                  # auth'd；下载预编译 launcher / SDK
./scripts/install-z42.sh      # z42 launcher + z42c + z42vm + stdlib → ./.z42/
./xtask build stdlib          # stdlib zpkg → artifacts/build/libraries/dist/release/
./xtask build compiler    # z42c 自举 → artifacts/build/compiler/z42c.driver/release/dist/z42c.driver.zpkg（多数用户由 install-z42.sh 直接提供）

# iOS RID（macOS only）
xcode-select --install        # Xcode + xcrun
rustup target add aarch64-apple-ios aarch64-apple-ios-sim

# Android RID
rustup target add aarch64-linux-android x86_64-linux-android
cargo install cargo-ndk --locked
# NDK + 构建 SDK：./xtask deps install --os android（装到 artifacts/tools/android-sdk）
# 或 export ANDROID_NDK_HOME=<your-ndk-path>

# wasm RID
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --locked
# 然后跑一遍 in-repo wasm build 产 pkg-web/ + pkg-nodejs/：
./xtask test platform wasm build
```

## 验证产物（每个 RID 都做）

### 1. 目录结构

```bash
ls artifacts/packages/z42-0.1.0-<rid>-release/
# 期望（按 RID 类别）：
# desktop:  bin/  libs/  native/  examples/{hello_c,hello_rust}/  manifest.toml
# ios:      bin/  libs/  native/{libz42.a, Z42VM.xcframework/}  Sources/{Z42VM,Z42VMC}/  Package.swift  examples/hello_c/  manifest.toml
# android:  bin/  libs/  native/{libz42_platform_android.{a,so}}  kotlin/io/z42/vm/  cpp/  examples/hello_c/  manifest.toml
# wasm:     bin/  libs/  native/{libz42.a, z42_wasm_bg.wasm}  pkg-web/  pkg-nodejs/  js/  package.json  examples/hello_c/  manifest.toml
```

### 2. manifest.toml

每个 package 顶层 `manifest.toml` 描述 abi-version、rid、profile、contents 列表、compat 字段（iOS deployment target / Android min-sdk / wasm-bindgen 版本）。

### 3. native lib 架构（关键 invariant）

```bash
# desktop
file artifacts/packages/z42-0.1.0-macos-arm64-release/native/libz42.dylib
# → "Mach-O 64-bit dynamically linked shared library arm64"

# ios-arm64
file artifacts/packages/z42-0.1.0-ios-arm64-release/native/libz42.a
# → "current ar archive"（内部为 arm64 Mach-O .o）

# android-arm64
file artifacts/packages/z42-0.1.0-android-arm64-release/native/libz42_platform_android.so
# → "ELF 64-bit LSB shared object, ARM aarch64"

# browser-wasm
file artifacts/packages/z42-0.1.0-browser-wasm-release/native/z42_wasm_bg.wasm
# → "WebAssembly (wasm) binary module"
```

### 4. SHA-256 invariant（xtask package 末尾自动跑 SHA-256 invariant，原生 byte compare）

`xtask package` 末尾会自动跑 SHA-256 invariant（原生 byte compare），确保跨 9 包 byte-identical：

- `libs/*.zpkg` — stdlib 二进制（平台无关；无 namespace 索引——读 NSPC）
- `native/include/z42_abi.h` + `z42_host.h` — Tier 1 C ABI 头
- `examples/hello_c/main.c` — C 嵌入示例（同一份源码）
- iOS: `Sources/Z42VM/*.swift` 跨 2 slice 一致
- Android: `kotlin/io/z42/vm/*.kt` + `cpp/z42vm_jni.c` + `cpp/CMakeLists.txt` 跨 2 ABI 一致
- wasm: `js/{index.js,index.d.ts,stdlib-resolver.js}` 与 `platforms/wasm/js/` 一致

任一 mismatch → exit 1 + 报告具体哪个文件。

## 平台 smoke 路径（可选）

各 RID 怎么消费 → 见对应平台的 build doc：

| RID | 消费方式 | 见 |
|-----|---------|-----|
| desktop | `cc -lz42` + `./z42c` / `./z42vm` | [building/compiler.md](building/compiler.md) / [building/vm.md](building/vm.md) |
| iOS | SwiftPM `.package(path:)` import | [building/ios.md](building/ios.md) |
| Android | Gradle `implementation(files(...))` + CMake | [building/android.md](building/android.md) |
| wasm | `npm install ./z42-0.1.0-browser-wasm-release` | [building/wasm.md](building/wasm.md) |

每个 RID 的 `examples/hello_c/README.md` 也含手工链接示例。

## 与 in-repo build flow 的关系

| 流程 | 入口 | 用途 |
|------|------|------|
| **per-arch flat package**（本文）| `./xtask package --rid <rid>` | 给开发者 / Tester / CI 一个独立 SDK ZIP |
| **in-repo native build** | `./xtask test platform <p> build` | 跑 in-repo 平台测试（emulator / simulator / wasm-pack / desktop）|

两条流程**共存**：`test platform <p> build` 产物供 in-repo 测试用；`./xtask package` 把那些产物 + 共享资源 cp 进一个 self-contained SDK 包。

## 失败排查

| 症状 | 原因 / fix |
|------|-----------|
| `rid '<x>' not in supported whitelist` | 你给的 RID 不在 9 个白名单内；见 memory `project_supported_platforms` |
| `cross-compiling to '<x>' from host '<y>' not supported` | host RID 不能 cross-compile 到目标 RID；换 host 或走 CI |
| `error: stdlib not built at artifacts/build/libraries/dist/release` | 先 `./xtask build stdlib` |
| `error: z42c not built ...` | 先 `./scripts/install-z42.sh` 或 `./xtask build compiler` |
| `cargo-ndk not found` | `cargo install cargo-ndk --locked` |
| `$ANDROID_NDK_HOME unset and NDK not found locally` | `./xtask deps install --os android` 或 `export ANDROID_NDK_HOME=<path>` |
| iOS `xcframework not created` | Xcode 没装或 `xcode-select -p` 指错 |
| wasm `pkg-web/ or pkg-nodejs missing — run the wasm-pack build first` | 先 `./xtask test platform wasm build` |
| SHA invariant fail | 通常是 stdlib / native include 中途被改；重建对应源 + 重打包 |

## See also

- 平台 build 详细 step：[`building/ios.md`](building/ios.md) / [`building/android.md`](building/android.md) / [`building/wasm.md`](building/wasm.md)
- Release 自动化（CI matrix）：[`release.md`](release.md)
- 9 RID 白名单理由：memory `project_supported_platforms`
- 包结构契约：[`docs/spec/archive/2026-05-13-define-package-layout/`](../spec/archive/2026-05-13-define-package-layout/)
- 设计原理（Tier 1 C ABI / per-arch flat 决策）：[`docs/design/runtime/embedding.md`](../design/runtime/embedding.md) §11.9
