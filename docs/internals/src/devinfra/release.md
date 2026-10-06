# 打包与发版

> 代码：`scripts/package/`、
> `scripts/packages.toml`、`scripts/cli/xtask_cli_package.z42`、`.github/workflows/release.yml`、
> `.github/workflows/ci.yml`（`publish-nightly`）、`scripts/versions.toml`
>
> 打包**引擎**（清单三层结构、组装流水、source-identity 门的实现）见[打包引擎](packaging.md)；
> 这页是操作面：产哪些包、怎么在本地产、怎么验、怎么发出去。

要读这页的场景：在本地打一个发行包来 inspect / 冒烟 / 给别人用；改了 `scripts/package/` 或
`packages.toml` 要验；或者要发一个 tag release。

## 1. 一次发布产出哪些包

| 包 | artifact 名模板 | 覆盖面 |
|---|---|---|
| **SDK**（完整开发包） | `z42-sdk-{version}-{rid}` | 只有 4 个 desktop RID |
| **runtime**（嵌入用） | `z42-runtime-{version}-{rid}` | 全部 9 个 RID |
| **workload** | `z42-workload-{label}-{wl}` | 5 个：`desktop` / `ios` / `android` / `wasm` / `test` |

各包装什么、组件怎么登记见[打包引擎 §3](packaging.md)。两条对操作有影响的边界：

- runtime 包**不含** z42c / z42vm，所以它不能当自举种子——种子在 SDK 包的 `programs/z42c/` 里
  （见[自举与种子](../compiler/self-hosting.md)）。
- `test` workload 平台无关、不进 `packages.toml`，CI 只在**一台 host** 上建一次，
  本地对应 `xtask package workload test [<version>]`。

## 2. 9 个 RID

| 类别 | RID | 能在哪些 host 上 build |
|---|---|---|
| Desktop | `linux-x64` / `linux-arm64` / `macos-arm64` / `windows-x64` | **只能同 RID host**（不跨编） |
| iOS | `ios-arm64` / `iossim-arm64` | 仅 macOS（需 Xcode + xcframework） |
| Android | `android-arm64` / `android-x64` | macOS / Linux（Windows 需 Android Studio 装 SDK+NDK） |
| wasm | `browser-wasm` | macOS / Linux / Windows（需 Node + wasm-pack） |

白名单外的 RID 直接报错。想要全平台覆盖走 CI matrix（§5）。

## 3. 本地打包

```bash
./xtask package sdk                                   # host SDK 包（桌面 RID）
./xtask package sdk --profile debug                   # debug profile
./xtask package runtime --rid ios-arm64               # 平台 RID 的 runtime 包
./xtask package workload --rid linux-x64              # 单 RID 的 desktop workload
./xtask package sdk --archive [--label L]             # 同时出发布归档（sdk / runtime / workload 都可带）
./xtask package finalize <LABEL> [--dir D] [--channel C] [--tag T] [--version V]
                                                      # 合并四个 per-RID desktop workload → SHA256SUMS → release-index.json
```

**本地出发布归档**：打包命令带 `--archive`，本次产出的包目录旁边（`artifacts/packages/`）就多出与 CI 同名、
同格式的归档（label 缺省取 `versions.toml` 的版本；只有 release profile 的包会发布）。归档名自带平台 / 架构，
与包目录并排放不会撞。命名规则（下表）只在 `xtask_release.z42` 里写一次，release.yml、nightly 与本地共用。
`finalize` 默认就在 `artifacts/packages/` 上做，要求 9 个 RID 的归档齐全（单机凑不齐，它是汇总 job 的步骤）。

`--no-build` 让它消费已有的 z42c + stdlib 产物（CI warm 路径用）。
`--variant <suffix>` 给包名加后缀。产物落 `artifacts/packages/z42-<version>-<rid>-<profile>/`。

打包前提：`./xtask build stdlib`（+ 改了编译器才要 `build compiler`）。平台 RID 另需该平台的
工具链，见[平台构建与嵌入](build-platforms.md)；wasm 还要先跑一遍
`./xtask test app wasm build` 产出 `pkg-web/` + `pkg-nodejs/`。

## 4. 包内布局与验证

包是**扁平的**：一个 arch 一个自包含目录，解开即用，没有嵌套的 per-arch 子树。

```
desktop SDK:  z42  bin/  programs/  libs/  native/libz42_compression.<dyn>  manifest.toml
runtime:      libs/  native/{libz42.{a,dylib,so} | z42.{lib,dll,dll.lib}, libz42_compression.<dyn>, include/{z42_abi.h,z42_host.h}}
ios:          libs/  native/{libz42.a, Z42VM.xcframework/}  Sources/{Z42VM,Z42VMC}/  Package.swift  manifest.toml
android:      libs/  native/libz42_platform_android.{a,so}  z42vm/src/main/{java,cpp}/  manifest.toml
wasm:         libs/  native/{libz42.a, z42_wasm_bg.wasm}  pkg-web/  pkg-nodejs/  js/  package.json  manifest.toml
```

嵌入件（libz42 + C 头）**只在 runtime 包**：SDK 里的 z42vm 静态链接 VM，SDK 的 `native/` 只放它启动时 dlopen 的
stdlib 扩展动态库。compression 的静态库哪个包都不发（desktop 没有静态注册路径，iOS / Android / wasm 的
preset 已把 compression 编进 libz42）。发布的动态库都是可重定位的：macOS install name 是 `@rpath/<名>`，
Linux 带 SONAME（= 文件名），Windows 带 import lib `z42.dll.lib`（`z42.lib` 是静态库）。

C ABI 头文件统一落 `native/include/`（`z42_abi.h` + `z42_host.h`，源是
`src/runtime/include/`）；iOS 与 Android 的平台目录下还各有一份同名 include，装的也是这两个
真头文件，源树同名目录里那些 `#include "../../.."` 转发 stub 不进包。

顶层 `manifest.toml` 由 `_pkgEmitManifest` 生成：

```toml
[package]        # name / version / abi-version / rid / profile / build-date / build-host
[contents]       # bin / libs 清单
[contents.native]# static / dynamic / containers / includes
[contents.platform]
[compat]         # host-min-version
```

验证产物的三步（每个 RID 都做一遍）：

```bash
ls artifacts/packages/z42-<version>-<rid>-release/       # ① 目录结构对照上表

file .../native/libz42.dylib          # ② native 库架构（关键 invariant；desktop 看 runtime 包）
#   macos-arm64   → Mach-O 64-bit ... arm64
#   ios-arm64     → current ar archive（内部 arm64 Mach-O .o）
#   android-arm64 → ELF 64-bit LSB shared object, ARM aarch64
#   browser-wasm  → WebAssembly (wasm) binary module

./xtask check packages                # ③ packages.toml 的解析 / 组件安装 / 发布归档自检

# ④ desktop：C 宿主链接打出来的 runtime 包（静态 + 动态各一遍 R1–R7；CI host-package 每个 RID 都跑）
Z42_RUNTIME_DIR=artifacts/packages/z42-runtime-<version>-<rid> ./xtask test app desktop
```

`xtask package` 末尾还自动跑一道 **source-identity 门**：逐字节比对包内每一份从仓库拷进去的
副本与仓库源（stdlib zpkg、ABI 头、Swift / Kotlin / JS facade 源）。任一处不一致、或某条规则
的目标路径存在却没有文件可比，都直接 exit 1。规则表与设计取舍见[打包引擎 §5](packaging.md)。
**跨包 byte-identical 是它的推论**：`libs/` 与 `native/include/` 在每个包里都拷自同一份仓库源，
`A==源 ∧ B==源 ⟹ A==B`——所以不需要（也做不到，各包在独立进程 / 独立 CI job 里打）两两比对。
紧接着是**包结构门**：该有的文件在、不该发的不在、原生库的 install name / SONAME / 架构 / glibc 下限对
（逐类断言见[打包引擎 §5.1](packaging.md)）。两道门本地打包与 CI 跑的是同一份。

装好的包还可以整包验：`./xtask test package [interp|jit]` 在打包出的发行版上跑发行包夹具 + golden。

### 常见失败

| 症状 | 处置 |
|---|---|
| `rid '<x>' not in supported whitelist` | RID 不在 §2 的 9 个里 |
| `cross-compiling to '<x>' from host '<y>' not supported` | 换 host 或走 CI matrix |
| `error: stdlib not built at artifacts/intermediate/libraries/flat/release` | 先 `./xtask build stdlib` |
| `error: z42c not built ...` | 先 `./scripts/install-z42.sh` 或 `./xtask build compiler` |
| `cargo-ndk not found` | `cargo install cargo-ndk --locked` |
| `$ANDROID_NDK_HOME unset and NDK not found locally` | `./xtask setup --os android` |
| iOS `xcframework not created` | Xcode 未装或 `xcode-select -p` 指错 |
| wasm `pkg-web/ or pkg-nodejs missing` | 先 `./xtask test app wasm build` |
| source-identity 门报文件不一致 | 对应的源被改过而包没重打；重建源 + 重打包 |

## 5. 发 tag release

版本号的单一来源是 `scripts/versions.toml` 的 `[project].version`；`src/runtime/Cargo.toml`
`[workspace.package].version` 是它的镜像，`xtask check versions`（GREEN gate 的 `versions` stage）守这条漂移。

```bash
$EDITOR scripts/versions.toml              # ① 改 [project].version
./xtask check versions                     # ② 应当 fail（两处不一致）
$EDITOR src/runtime/Cargo.toml             # ③ 同步 [workspace.package].version
./xtask check versions                     # ④ 应当通过
cargo metadata --manifest-path src/runtime/Cargo.toml --format-version 1 --no-deps \
  | jq -r '.packages[].version' | sort -u  #    workspace 全部成员同版本

git commit -am "chore(release): bump version X → Y" && git push origin main
git tag vY && git push origin vY           # ⑤ 触发 .github/workflows/release.yml
```

`release.yml` 三段：

| job | 做什么 |
|---|---|
| `verify-version(linux-x64)` | 校验 `tag` 去掉 `v` 后等于 `versions.toml [project].version`，不等就 fail-fast |
| `package-<rid>`（9 个 RID matrix） | 每 RID 一台 runner，先从上一 nightly 种子自举，再打包：desktop RID 跑 `package sdk` + `package runtime` + `package workload`（外加在一台 host 上 `package workload test`），平台 RID 跑 `package runtime --rid <rid>`；各打包命令带 `--archive --label <version>`，在本 runner 上随打包出归档（保留可执行位；Windows 出 zip） |
| `publish-release(linux-x64)` | 汇总归档，`xtask package finalize <version> --dir dist`（合并 desktop workload → `SHA256SUMS` → `release-index.json`），`gh release create v<version>` 上传 |

**artifact 命名**（`<v>` 在 nightly 里是字面量 `nightly`）：

| 内容 | 文件名 |
|---|---|
| SDK（linux-x64 / linux-arm64 / macos-arm64） | `z42-sdk-<v>-<rid>.tar.gz` |
| SDK（windows-x64） | `z42-sdk-<v>-windows-x64.zip` |
| runtime（9 个 RID） | `z42-runtime-<v>-<rid>.tar.gz`（Windows `.zip`） |
| workload（5 个） | `z42-workload-<v>-<wl>.tar.gz` |
| 安装脚本 | `install.sh` / `install.ps1`（同时发到 Pages 站点根） |
| 校验和 / 清单 | `SHA256SUMS`（coreutils 格式）/ `release-index.json` |

`release-index.json` 是 launcher 的供给契约：从 `SHA256SUMS` 反推每个 RID 有哪些包、各自的
sha，以及每个 workload 能装在哪些 host、覆盖哪些目标 RID。

**`--prerelease` 的自动条件**：版本号 < `1.0.0`（pre-1.0 阶段全部），或 tag 带 `-` 后缀
（`v0.2.5-rc1`）。GitHub UI 不把 prerelease 放进 "Latest release" 高亮位。

Actions UI 的 "Run workflow" 可以手动触发（输入的 version 必须与 `versions.toml` 一致），
但它跑完整三段、publish 段会真的建 release —— 手动触发等于一次正式发布。

| 症状 | 原因 |
|---|---|
| `verify` 报 `drift: tag=X versions.toml=Y` | tag 与 versions.toml 不一致，改正后重打 tag |
| 某个 `package-<rid>` fail | 先看 `ci.yml` 里对应的 package job 是否也红，通常是工具链 / 网络 / cache |
| `publish` 报 already exists | tag 重复；pre-1.0 一般不删 release，直接 bump 到下一版 |

## 6. nightly

每次 push 到 `main`，`ci.yml` 的 `publish-nightly` 汇总全部 package artifact，**强制覆盖**
名为 `nightly` 的 GitHub Release（delete + recreate，URL 永远稳定）。它标 `--prerelease`、
不签名、文件名不带版本号（`z42-sdk-nightly-<rid>.tar.gz`）。

nightly 同时是**下一轮自举的种子**（`install-z42.sh` 默认拉它）。因此它的 `needs` 故意
**不**挂在几条下载种子的 job（`test-vm-jit` / `test-stdlib-jit`）上——
格式 bump 那一轮它们会暂时失败，挂上去就死锁且没有逃生口。见 [CI 拓扑](ci.md)。

```bash
curl -LO https://github.com/z42-lang/z42/releases/download/nightly/z42-sdk-nightly-linux-x64.tar.gz
curl -LO https://github.com/z42-lang/z42/releases/download/nightly/SHA256SUMS
shasum -a 256 -c SHA256SUMS --ignore-missing
```

nightly 是 main 的最新快照，不保证稳定；生产 / 集成用 tag release。

用户侧安装脚本是 `scripts/install/install.sh` / `install.ps1`（默认 nightly、装到 `~/.z42`、
重跑即更新）。它随每个 release 上传，由 `deploy-book.yml` 发到 Pages 站点根，
`ci.yml` 的 `package-host` 会用刚打出的包对它跑一遍离线安装冒烟。
