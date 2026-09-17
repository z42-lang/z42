# 开发环境与工具链自举

> 对齐：2026-09-17（change `restructure-docs-three-books`）｜ 代码：`scripts/install-z42.sh`、
> `scripts/install/install.sh`、`scripts/install/xtask_install.z42`、`versions.toml`、
> `src/runtime/Cargo.toml`、`src/compiler/z42c.driver/src/Main.z42`、`.gitattributes`
>
> 只想**写 z42 程序**不用读这页——装 SDK 见学习手册的[安装 z42](../../../learn/src/getting-started/install.md)。
> `z42` / `z42c` / `z42b` 的用户命令面见[参考手册 · 工具链](../../../reference/src/toolchain/README.md)。

这页是**改 z42 本身**（编译器 / VM / stdlib / xtask）的第一页：一台干净机器上从零到能跑
gate 要敲哪几条命令、每个 host 平台各自要注意什么。命令树与 `--toolchain` 的机制见
[xtask](xtask.md)；构建链怎么编排见[构建编排](build.md)；产物落在哪见[产物目录布局](artifacts-layout.md)。

## 1. 冷启动：从零到能跑 gate

```bash
git clone https://github.com/z42-lang/z42 && cd z42

./scripts/install-z42.sh                  # ① 下载种子 → ./.z42/（Windows: scripts\install-z42.bat）
.z42/z42 publish scripts/xtask.z42.toml   # ② 编 xtask → 仓库根的原生 ./xtask
./xtask build all                         # ③ 编译器 + VM + stdlib（全部从源码）
./xtask test                              # ④ 完整 GREEN gate
```

关于这四步：

- **①** `install-z42.sh` 只是给用户安装器 `scripts/install/install.sh` 套上仓库默认值：版本取
  `versions.toml` 的 `[toolchain.z42].launcher`，目标是 gitignore 掉的 `./.z42/`，不动 PATH。
  重跑即更新；它是整条链上**唯一**的非 z42 环节。没有网络 / 没有 `gh` 就起不了步——工具链没有
  任何非 z42 的逃生编译器。
- **②** `z42 publish` 产出的 `./xtask` 是原生 apphost，自己能定位 `./.z42` 运行时，**不需要
  PATH、也不需要装 desktop workload**：`scripts/xtask.z42.toml` 用 `[build] hooks` 声明了
  `scripts/hooks/`，publish 期间现造 apphost stub。
- **不想要 apphost** 时（CI 走的就是这条）直接跑 zpkg：

  ```bash
  .z42/bin/z42c build scripts/xtask.z42.toml --release   # → artifacts/xtask/xtask.zpkg
  .z42/z42 artifacts/xtask/xtask.zpkg -- test            # 任意 xtask 子命令
  ```

- **完全没有 launcher** 时（只有 cargo 编出的 z42vm），用 z42vm 直跑 zpkg：

  ```bash
  vm="$PWD/artifacts/build/runtime/release/z42vm"
  libs="$PWD/artifacts/build/libraries/dist/release"
  Z42_PORTABLE_VM="$vm" Z42_LIBS="$libs" "$vm" artifacts/xtask/xtask.zpkg -- build stdlib
  ```

## 2. 日常命令

```bash
./xtask build all     # compiler + runtime + stdlib（各子命令与编排见 build.md）
./xtask test          # 完整 GREEN gate（stage 清单见 test-gate.md）
./xtask -h            # 命令树
```

日常**不必**手动 `build stdlib`：`test e2e` 自带重建波（stdlib → golden `.zbc` → cargo VM）。
只想重生 golden 基线是 `build test`——**没有 `build test-assets` 这个命令**。

绕开 xtask 直接调编译器时（例如复现一次 stdlib workspace 构建）：

```bash
( cd src/libraries && Z42_LIBS=... z42c build --workspace --release )
```

增量编译默认开，`--no-incremental` 强制全量。

## 3. VM 的 cargo feature

`src/runtime/Cargo.toml` 的 `[features]`：

| feature | 默认 | 作用 |
|---|:---:|---|
| `jit` | ✅ | Cranelift JIT 后端（桌面 x64 / aarch64） |
| `native-interop` | ✅ | Tier 1 native 扩展 ABI（dlopen + libffi） |
| `mimalloc-alloc` | ✅ | z42vm 二进制的全局分配器走 mimalloc |
| `interp-only` | — | 只表意图（不引额外依赖），平台 preset 用它 |
| `aot` | — | AOT 占位，无实现 |
| `bundled-compression` | — | 静态链接 `z42-compression` 而不是 dlopen（wasm 必须） |
| `dhat-heap` / `profile-contention` | — | 分析用，见[调试与运行时诊断](debugging.md) |
| `z42-test-fixtures` | — | build.rs 现编两个 `.z42` fixture；默认关是因为它会把 build script 挂到 `z42c.driver.zpkg` 上，每轮 `build compiler` 后整图重编 |

平台 preset 是现成的组合：`wasm` = `interp-only` + `bundled-compression`；
`ios` / `android` = `interp-only` + `aot` + `native-interop` + `bundled-compression`。

```bash
cargo build --manifest-path src/runtime/Cargo.toml --release
cargo build --manifest-path src/runtime/Cargo.toml --no-default-features --features interp-only
./xtask feature-matrix        # 逐个编 interp-only / wasm / ios / android，验组合都编得过
```

> 没有名为 `interp` 或 `host` 的 feature；解释器是无条件编进去的。

## 4. 环境变量速查

产物落在哪见[产物目录布局](artifacts-layout.md)；下面这些变量决定「用哪套工具链、往哪写诊断」。

| 环境变量 | 作用 |
|---|---|
| `Z42_LIBS` | stdlib 扁平目录；缺省回落 `artifacts/build/libraries/dist/release` |
| `Z42_PORTABLE_VM` | 指定 z42vm 路径（CI 显式设，本地一般不用） |
| `Z42_HOME` | SDK 根；由 `xtask --toolchain` 写入 |
| `Z42_LOG` / `Z42_CRASH_DIR` / `Z42_SAMPLE_*` | 见[调试与运行时诊断](debugging.md) |
| `Z42_TEST_CHANGED_BASE` | `xtask test changed` 的默认 base ref |
| `Z42_IOS_DEST` | 指定 iOS 模拟器目标，见[平台构建与嵌入](build-platforms.md) |

运行时旋钮的完整清单与五层优先级见[运行时设置的实现](../runtime/runtime-settings.md)。

## 5. z42c 的编译阶段转储

改编译器时用的调试面（`src/compiler/z42c.driver/src/Main.z42`）。需要 stdlib 的调用前缀
`Z42_LIBS="$PWD/.z42/libs"`：

| 命令 | 输出 |
|---|---|
| `z42c --dump-keywords` | 关键字表，一行一个（vscode 语法门拿它对账） |
| `z42c --dump-tokens <f.z42>` | token 流 |
| `z42c --dump-ast <f.z42>` | AST s-表达式 |
| `z42c --dump-bound <f.z42>` | 类型检查后的 Bound 树（含类型注解 + 诊断计数） |
| `z42c --dump-ir <f.z42>` | `.zasm` 风格的 IR 文本 |
| `z42c --emit-zbc <f.z42> <out.zbc> [--opt-all]` | 单文件编到 `.zbc` |

`--emit-zbc` 的默认优化集**关掉了** StackAlloc / Inline / PureCall / DeadBranch / Devirt
（开了会改 golden 字节）；`--opt-all` 按真实 release 全优化编——**写优化类 golden 必须挂
`opt_all` sidecar**，否则那些 pass 一条路都走不到。

`z42c build` 的选项在[参考手册](../../../reference/src/toolchain/cli-z42c-z42b.md)，那里是 SoT。
`clean` / `test` / `bench` 不在 z42c 上，它们由 `z42b` 编排。

## 6. 三个 host 平台的差异

前置工具三处都是 git + Rust stable + `gh`（auth 过，下载种子用）+ 一套 C 工具链
（cargo 要编 zlib-ng / libffi 这些 C 依赖）。Rust 的 MSRV 由 `versions.toml`
`[toolchain.rust].min_version` 钉住。

| | C 工具链 | 取种子 | 能产的平台包 |
|---|---|---|---|
| **macOS**（arm64，主开发平台） | `xcode-select --install` | `./scripts/install-z42.sh` | 全部；**唯一**能产 iOS 包的 host |
| **Linux**（x64 / arm64） | `build-essential` / `Development Tools` | `./scripts/install-z42.sh` | 除 iOS 外全部 |
| **Windows**（x64） | MSVC（rustup 选 `x86_64-pc-windows-msvc`） | `scripts\install-z42.bat` | windows-x64、browser-wasm、Android（需 Android Studio） |

缺 C 工具链的典型症状是 cargo 编 zlib-ng / libffi 时报 `cc: command not found` / `xcrun: error`。
只支持厂商官方维护的架构，`macos-x64` 不在支持面内。

### Windows

开发在 **Git Bash**（Git for Windows 自带 MSYS2 bash + coreutils）里做，不是 PowerShell / CMD。
xtask 调用的 POSIX 子进程在 Git Bash 里原样跑，无需 prefix。仓库不提供 `.ps1` 镜像——维护两套
脚本的负担翻倍，而 cargo / gh 本来就是跨平台 CLI。WSL2 也能跑（按 Linux 那一列），但产
windows-x64 包时用 Git Bash + 原生 `cargo.exe`。

可执行文件带 `.exe`（`./xtask.exe build stdlib`）。产物后缀差异：

| 产物 | macOS | Linux | Windows |
|---|---|---|---|
| z42c / z42vm | 无后缀 | 无后缀 | `.exe` |
| 动态库 | `libz42.dylib` | `libz42.so` | `z42.dll` + `z42.lib` |
| 静态库 | `libz42.a` | `libz42.a` | cargo 不出（Windows 无 `.a` 惯例） |

打包时按哪个后缀存在自动选。sha256 校验按 `shasum → sha256sum` 顺序兜底（Git Bash 两者都有）。

**行尾是 Windows 上唯一会咬人的地方**。`.gitattributes` 强制这些文件为 LF：

- `*.sh` / `Makefile` —— CRLF 的 `.sh` 在 Git Bash 里报 `bad interpreter`
- `*.z42` —— z42c 对源文本算 `SHA256` 写进 `.zpkg` 的 `SourceHash`；CRLF checkout 会让 hash
  相对提交的 LF 基线漂移，`src/tests/{zbc,zpkg}-format/` 的字节基线比对就红了
- `*expected_output.txt` —— golden 是**逐字节**比对，而 z42vm 的输出永远是 LF；不强制 LF 时
  Windows 上 expected 与 actual 打印出来一模一样却全红，差异全在不可见的 `\r`

防御深度：z42c 在算源码哈希前内部做 CRLF → LF 规范化，即使 `.gitattributes` 失效 hash 仍稳定。
clone 完发现 `.sh` 带 `\r\n` 就 `git config --global core.autocrlf input` 后
`git rm --cached -r . && git reset --hard`。

`xtask deps install --os android` / `--os wasm` 的自动下载在 Windows 上**拒绝执行**（走 POSIX
`.tar.gz` 路径），改用 Android Studio 的 SDK Manager 和 Node.js MSI。
