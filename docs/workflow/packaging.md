# 本地打 SDK package

> **面向：** 想在本地产 9 个 per-arch flat SDK package 之一、然后 inspect / smoke test / 给他人使用的开发者。
>
> **不是：** 给 release 自动化看的（CI 走 [`release.md`](release.md) + `.github/workflows/release.yml`）；也不是给 add-ios-tests / add-android-tests in-repo 流程看的（那些走 `platforms/<x>/build.sh` + `test.sh`）。

## 统一入口

```bash
./xtask package runtime --rid <rid>          # 平台 RID（ios-*/android-*/browser-wasm）；见下表
./xtask package runtime --rid <rid> --profile debug   # debug profile（dev 用）
./xtask package sdk                          # 桌面 RID（含 host）→ host SDK 包
./xtask -h                                   # 完整选项
```

> 桌面 RID（`linux-*` / `macos-*` / `windows-*`）走 `xtask package sdk`（打 host SDK 包，
> 只能在同 RID host 上产）；平台 RID（`ios-*` / `android-*` / `browser-wasm`）走
> `xtask package runtime --rid <rid>`。

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

### 4. source-identity 门（`xtask package` 末尾自动跑，逐字节）

`xtask package` 末尾自动比对**包内每一份从仓库拷进去的副本 vs 仓库源**，逐字节
（`_pkgSourceIdentityCheck`，`scripts/package/xtask_package.z42`）。规则表：

| 包内路径 | 仓库源 | 拷贝点 |
|---|---|---|
| `libs/*`（zpkg + zsym） | `_libsDir(root)`（flat stdlib dist） | `_pkgCopyLibs` |
| `native/include/*.h` | `src/runtime/include/` | `_copyAbiHeaders` |
| `Sources/Z42VMC/include/*.h`（ios） | 同上 | `_copyAbiHeaders` |
| `z42vm/src/main/cpp/include/*.h`（android） | 同上 | `_copyAbiHeaders` |
| `Sources/Z42VM/*.swift`、`Sources/Z42VMC/dummy.c` | `src/toolchain/workload/ios/platform/Sources/…` | `_packageIos` |
| `z42vm/src/main/java/**/*.kt`（递归） | `…/android/platform/z42vm/src/main/java/` | `_pkgCopyKtTree` |
| `z42vm/src/main/cpp/{z42vm_jni.c,CMakeLists.txt}` | `…/android/platform/z42vm/src/main/cpp/` | `_pkgEmitAndroidGradleProject` |
| `js/{index.js,index.d.ts,stdlib-resolver.js}` | `src/toolchain/workload/wasm/platform/js/` | `_packageWasm` |

包内不存在的类别整条跳过（一张表服务全部包类别）。任一 mismatch → 报出具体文件
+ exit 1；某条规则的路径存在却 0 个文件可比 → ⚠ 告警（规则与拷贝点脱钩的信号）。
源侧无对应物的包内文件不在本门管辖内（跳过）。

> ⚠️ ios `Sources/Z42VMC/` 与 android `…/cpp/` **不能整棵目录递归比**：它们的 `include/`
> 装的是 `_copyAbiHeaders` 拷进去的**真 runtime 头**，而源树同名目录里是
> `#include "../../.."` 的转发 stub，整棵比会假红。故那两处是显式文件规则 + 单独一条
> 指向 `src/runtime/include` 的 include 规则。

> **不是哈希**：直接读字节比较，比 hash 更强（无碰撞面、不依赖外部工具）。旧名
> `SHA-256 invariant` 源自最初 bash `package.sh` 里真算 sha256sum 的实现，移植到
> z42 时换成了字节比较、名字留到 2026-09-06 才正（fix-package-identity-gate）。
>
> **为什么是「包 vs 源」而不是「包 vs 包」**：上表每一类文件，各包里的副本都拷自
> **同一份仓库源**，故 `A==源 ∧ B==源 ⟹ A==B`——**跨包 byte-identical 是本门的推论**。
> 反过来不成立：两两比对只能证明「大家一样」，证不了「大家都对」（所有包一致地
> 拷了陈旧副本时跨包比对全绿）。且各包在独立 xtask 进程 / 独立 CI job 里打，
> 真做跨包比对得先改 CI 拓扑。

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
| **per-arch flat package**（本文）| 桌面：`./xtask package sdk`；平台：`./xtask package runtime --rid <rid>` | 给开发者 / Tester / CI 一个独立 SDK ZIP |
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
