# Windows 开发支持

> **TL;DR：** 装 **Git for Windows**（自带 Git Bash）+ Rust toolchain + `gh`，跑 `install-z42.bat` 拿到 z42 工具链，然后 z42 build CLI（xtask）在 Git Bash 终端里直接跑。z42 不提供 PowerShell `.ps1` 镜像 —— bash 脚本是单一真相源。

## 为什么 Git Bash 不是 PowerShell

z42 的 build / test / package 工具链通过 z42 build CLI（xtask）驱动，仍依赖 POSIX 工具（`set -euo pipefail` 风格的子进程 + coreutils），有两条 Windows 支持路径：

| 路径 | 优点 | 缺点 |
|------|------|------|
| **Git Bash**（Git for Windows 自带 MSYS2 bash + coreutils）| 零额外装东西；xtask 调用的 POSIX 工具原样跑；与 macOS/Linux 一致 | 是 emulated POSIX，少数 GUI 工具（如 Xcode，本来就 macOS-only）跑不了 |
| WSL2（Windows Subsystem for Linux）| 完整 Linux 环境 | 装好后 dev 在 WSL 里、产物也在 WSL 文件系统里；与 Windows 原生 Rust toolchain 互操作有摩擦 |
| 重写为 PowerShell `.ps1` | "Native" Windows 感 | 维护负担翻倍；每次 fix 要同步两边；与厂商工具链（cargo / gh 都是跨平台 CLI）冗余 |

**z42 选 Git Bash 为推荐路径。** WSL2 也能跑（按 Linux 文档），但产 windows-x64 SDK package 时建议 Git Bash + 原生 `cargo.exe`。

## 一次性装

按需挑（最少 1 + 2 + 3）。

### 1. Git for Windows（含 Git Bash）

- 官网：https://git-scm.com/download/win
- 安装时全默认即可（"Use Git from the command line and also from 3rd-party software" + "Use the OpenSSL library" + "Checkout as-is, commit Unix-style line endings"）
- 装完后开始菜单有 "Git Bash"；右键资源管理器有 "Git Bash Here"

### 2. z42 工具链 primer（`install-z42.bat`）

- 在 Git Bash / CMD 跑 `scripts/install-z42.bat` 下载预编译 `z42` / `z42c` / `z42vm` / stdlib 到 `./.z42/`
- 需要 `gh`（auth'd，下载 SDK 用）；工具链全 z42 自举，无需安装 .NET 或任何其他运行时
- 仅当改了 `src/compiler/**` 才需从源码重建：`./xtask.exe build compiler`

### 3. Rust toolchain (stable)

- 官网：https://rustup.rs → 跑 `rustup-init.exe`
- 选 default host triple = `x86_64-pc-windows-msvc`（z42 windows-x64 用这个）
- 装完后开 **新** Git Bash 窗口，`cargo --version` / `rustc --version` 都能跑

### 4.（可选）xtask 任务跑跑

- xtask 是编译产物（`artifacts/xtask/xtask.zpkg` + 原生 apphost `xtask.exe`），无需额外安装；先 `./xtask.exe build stdlib` 那一套编出来
- 然后 `./xtask.exe build` / `./xtask.exe test` 在 Git Bash 里跑

### 5.（可选）Android SDK + NDK —— 用 Android Studio

如果你要在 Windows 上产 `android-arm64` / `android-x64` SDK 包：

- 装 [Android Studio](https://developer.android.com/studio)（一键带 SDK Manager）
- SDK Manager → Tools 装：Android SDK Platform 34 + Build-Tools 34.x + NDK (Side by side) 26.x
- 设环境变量（PowerShell `[System.Environment]::SetEnvironmentVariable(...)` 或 Settings GUI）：

  ```
  ANDROID_HOME       = %LOCALAPPDATA%\Android\Sdk
  ANDROID_NDK_HOME   = %LOCALAPPDATA%\Android\Sdk\ndk\26.1.10909125
  ```

  （NDK 版本号按你装的实际填）

- `rustup target add aarch64-linux-android x86_64-linux-android`
- `cargo install cargo-ndk --locked`

⚠️ `./xtask.exe deps install --os android` 在 Windows 上**会拒绝执行**（只支持 macOS/Linux 自动下载；emulator tier 的自动安装同理）；走 Android Studio 这条路。

### 6.（可选）Node.js —— 用官方 MSI

如果你要打 `browser-wasm` SDK 包：

- 官网：https://nodejs.org/en/download → Windows Installer (.msi) LTS
- `cargo install wasm-pack --locked`
- `rustup target add wasm32-unknown-unknown`

⚠️ node 的自动安装（`deps install --os wasm` 内含；wasm 测试的用到才装兜底同理）在 Windows 上也会**拒绝执行**（POSIX .tar.gz 路径）；走 MSI 这条路。

## 日常工作流

打开 **Git Bash** 终端（不是 PowerShell / CMD），cd 到 repo：

```bash
# 编译（z42c 由 install-z42.bat 提供；改了编译器才需重建）
./xtask.exe build compiler         # z42c 自举（可选）
cargo build --manifest-path src/runtime/Cargo.toml --release   # z42vm + libz42

# stdlib
./xtask.exe build stdlib

# 全套测试
./xtask.exe test

# 打 host (windows-x64) SDK package
./xtask.exe package sdk
```

xtask 调用的 POSIX 子进程都在 Git Bash 直接跑（shebang `#!/usr/bin/env bash` 生效），无需额外 prefix。

## Windows 特定的坑

### 路径分隔

Git Bash 内部把 `C:\foo\bar` 映射为 `/c/foo/bar`。但 Cargo / z42vm 这种**原生 Windows** 工具看 `cargo build --manifest-path src/runtime/Cargo.toml` 时仍按 Windows 的相对路径解析。z42 脚本全用相对路径 + `cd` 切到 repo root，不易踩。

### 文件名后缀

| 产物 | macOS | Linux | Windows |
|------|-------|-------|---------|
| z42c | `z42c` | `z42c` | `z42c.exe` |
| z42vm | `z42vm` | `z42vm` | `z42vm.exe` |
| 动态库 | `libz42.dylib` | `libz42.so` | `z42.dll` (+ `z42.lib` import lib) |
| 静态库 | `libz42.a` | `libz42.a` | （cargo 不出，因为 Windows 没有惯例的 `.a`）|

`xtask package` 的 desktop 打包步骤（原 `scripts/_lib/package_desktop.sh`，已移植进 xtask）自动检测哪个后缀存在并 cp 进 `bin/`。

### Line endings

仓库带 [`.gitattributes`](../../../.gitattributes)：

- `*.sh` / `Makefile` 强制 LF（Git Bash 跑 CRLF 的 .sh 会报 `bad interpreter`）
- `*.rs` / `*.md` 等用 git autocrlf 默认（Windows 用户编辑器看 CRLF，git 存 LF）
- `*.z42` 强制 LF：z42c 对源文件做 `SHA256(text)` 作为 `SourceHash` 写入 `.zpkg`；如果 Windows checkout 是 CRLF，hash 会和提交的 LF 字节基线（`src/tests/zbc-format/` 的 committed baselines）飘移，导致 zbc-format golden 比对失败
- `*.zpkg` / `*.zbc` / `*.wasm` / `*.dll` 等 binary（永不转换）

防御深度：z42c 在源码哈希时内部做 `CRLF → LF` 规范化，即使 `.gitattributes` 失效，hash 也保持跨平台稳定。

如果 clone 完发现 `.sh` 含 `\r\n`，跑 `git config --global core.autocrlf input` 然后 `git rm --cached -r . && git reset --hard`。

### `sha256sum` / `shasum`

- macOS 只有 `shasum`
- Linux 通常有 `sha256sum`
- Git Bash 通常**两者都有**

xtask 打包的 sha256 校验（原 `scripts/_lib/package_helpers.sh` 的 `pkg_sha256_check`，已移植进 xtask）与 `xtask_install.z42` 的 `_sha256File` 已经按 "shasum → sha256sum → 错" 的顺序兜底。

### `file` / `ar` 工具

Git Bash 自带 `file`、`ar`、`grep`、`awk`、`sed`、`tr` —— `xtask package` 用到的全部 POSIX 工具都有。`xxd` / `hexdump` 也在。

### `xcrun` / `xcodebuild` —— iOS RID

iOS slice package 需要 Xcode，**永远只在 macOS host 上能跑**。Windows host 跑 `./xtask.exe package --rid ios-arm64` 直接被 `validate_rid_supported_on_host` 拒绝。

## 跑得通的 RID matrix（Windows host）

| RID | 是否能在 Windows host build | 备注 |
|-----|----------------------------|------|
| `windows-x64` | ✅ | 主要支持目标 |
| `macos-arm64` / `linux-x64` / `linux-arm64` | ❌ | desktop RID 只支持同 host build |
| `ios-arm64` / `iossim-arm64` | ❌ | 需要 macOS + Xcode |
| `android-arm64` / `android-x64` | ⚠️ 可以但要装 Android Studio | 见上文 §5 |
| `browser-wasm` | ✅ | 需要装 Node.js MSI + wasm-pack（见 §6）|

完整 9 RID 矩阵 + cross-host 规则见 [`packaging.md`](../packaging.md)。

## 测试 Windows 包

```bash
# 1. 装好 z42 工具链 + Rust（§2 + §3）
# 2. 编 stdlib（z42c 由 install-z42.bat 提供；改了编译器才 ./xtask.exe build compiler）
./xtask.exe build stdlib

# 3. 打 windows-x64 package
./xtask.exe package sdk

# 4. 验证产物
ls artifacts/packages/z42-0.1.0-windows-x64-release/
#  bin/z42c.exe bin/z42vm.exe
#  native/z42.dll native/z42.lib native/include/{z42_abi.h,z42_host.h}
#  libs/*.zpkg
#  examples/{hello_c, hello_rust}/
#  manifest.toml
```

跑 `./xtask.exe test` 应该全绿；如果有失败按本文档下方"See also"链表里的 testing/ 文件排查。

## See also

- 9 RID per-arch SDK 包：[`packaging.md`](../packaging.md)
- VM build feature flags：[`vm.md`](vm.md)
- 测试入口：[`testing/`](../testing/)
- 9 RID 白名单（含 `windows-x64`）：memory `project_supported_platforms`
