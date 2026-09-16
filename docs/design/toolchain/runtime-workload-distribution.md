# runtime & workload 分发：安装 / 更新 / 运行

> **部分已实施（split-release-runtime-package, 2026-06-14）**：manifest schema（`runtimes.<rid>.sdk` + `.runtime`，平台 RID `.runtime` only）、独立 runtime 包、`_fetchManifest` new/old-format 双格式已落地。workload / channel / `z42 update` / 签名等见 Deferred。

## 现状（起点）

release.yml 发 9 个 RID 的 `z42-<ver>-<rid>.tar.gz` + `SHA256SUMS`，tag `v<ver>` / 滚动 `nightly`。`_cmdInstall` 只装 host RID，**靠约定拼资产名 + 读 SHA256SUMS**，无 manifest、无 channel/latest、无 workload、无 self-update，Windows `.zip` 路径未做。

## runtime / workload：名词 vs 命令 vs 组件（消除"两处都含 runtime"的冗余）

> **现状（simplify-z42-cli）**：SDK 单版本，launcher 不管理 host 运行时版本——`z42 install` / `uninstall` / `default` / `list` / `self-update` 与下文「运行解析顺序（多版本选择）」「channel + 更新」均**未采用、已从命令面移除**；SDK 由安装脚本安装与更新，命令面以 [book · z42 命令参考](../../book/src/toolchain/cli.md) 为准。workload 相关设计仍有效。

**名词 `runtime`（per-RID）**：某 RID 的 z42vm/嵌入件——host（macos/linux/windows）与 target（ios/android/wasm）**都是 runtime**，只是 RID 不同。manifest 里它们都是 `runtimes` 段的**组件（pack）**。

**但用户面只有两个安装入口，target runtime 不开第二条直装路径**：

| 入口 | 装什么 | 说明 |
|---|---|---|
| `z42 install <ver>` | **host runtime**（z42vm + libs + z42c）| 必备、能跑 z42 的前提；**只此一处直装 runtime** |
| `z42 workload install <plat>` | **能力束** = 组合 {target runtime pack} + {打包工具 + 模板 + native glue} | 拿到 ios/android/wasm runtime 的**唯一**用户面路径 |

→ target runtime 是 workload 内部组合的 **component**，不作为独立 install 命令暴露 ⇒ **无冗余、无"两条路装同一个 runtime"的困惑**。对标 dotnet：你 `dotnet workload install ios`，ios runtime pack 是 workload 内的 component，从不 `dotnet install ios-runtime`。

9 RID：4 桌面 RID 的 runtime 经 `z42 install`；ios/android/wasm 的 runtime pack 经对应 workload 拉入。

> **desktop 也是 workload（2026-06-17 consolidate-platform-into-workload 裁决）**：上面"两个安装入口"针对 **runtime**——这条不变（桌面 runtime 仍只经 `z42 install`，不开第二条直装）。但 **publish/export 维度，desktop 与 ios/android/wasm 对称，也是一个 workload**：默认 `z42 build`/`run` 用 host runtime、零 workload；要 **`z42 publish`（产 desktop apphost）或 `z42 export`（导出桌面工程）** 才 `z42 workload install desktop`。即 `workloads.desktop` 装的是"桌面 publish/export 能力束（apphost 模板 + 桌面 native glue），**不含** runtime（runtime 已在 host）"——故 `workloads.desktop` 无 `target runtime pack`，区别于 ios/android/wasm（它们的 workload 含 target runtime pack，因设备侧无 host runtime）。apphost 因此归 desktop workload，不再是 launcher Core 命令（见 [launcher-command-dispatch.md](launcher-command-dispatch.md)）。

## 供给契约：每 release 发 manifest 资产（`release-index.json`）

不裸爬 GitHub API（rate-limit / 契约不稳 / 难离线 / 难签名）。每个 release 上传一个 `release-index.json` 资产作**稳定契约**（对标 rustup channel manifest / dotnet release-index）。channel 解析借 GitHub 现成的稳定 URL：

- **stable 最新** → `releases/latest/download/release-index.json`（GitHub `/latest/` 重定向到最新非 prerelease）
- **nightly** → `releases/download/nightly/release-index.json`（固定滚动 tag）
- **指定版本** → `releases/download/v<ver>/release-index.json`

### manifest schema（草案）

```json
{
  "schema": 1,
  "version": "0.3.5",
  "channel": "stable",
  "tag": "v0.3.5",
  "published": "2026-06-12T00:00:00Z",
  "runtimes": {
    "macos-arm64": {
      "sdk":     { "archive": "z42-sdk-0.3.5-macos-arm64.tar.gz",     "sha256": "…" },
      "runtime": { "archive": "z42-runtime-0.3.5-macos-arm64.tar.gz", "sha256": "…" }
    },
    "linux-x64":   { "sdk": {…}, "runtime": {…} },
    "linux-arm64": { "sdk": {…}, "runtime": {…} },
    "windows-x64": { "sdk": {…}, "runtime": {…} },
    "ios-arm64":    { "runtime": { "archive": "z42-runtime-0.3.5-ios-arm64.tar.gz",    "sha256": "…" } },
    "iossim-arm64": { "runtime": { "archive": "z42-runtime-0.3.5-iossim-arm64.tar.gz", "sha256": "…" } },
    "android-arm64":{ "runtime": { "archive": "z42-runtime-0.3.5-android-arm64.tar.gz","sha256": "…" } },
    "android-x64":  { "runtime": { "archive": "z42-runtime-0.3.5-android-x64.tar.gz",  "sha256": "…" } },
    "browser-wasm": { "runtime": { "archive": "z42-runtime-0.3.5-browser-wasm.tar.gz", "sha256": "…" } }
  },
  "workloads": {
    "ios":     { "archive": "z42-workload-0.3.5-ios.tar.gz",     "sha256": "…", "host": ["macos-arm64"], "runtimes": ["ios-arm64","iossim-arm64"] },
    "android": { "archive": "z42-workload-0.3.5-android.tar.gz", "sha256": "…", "host": ["macos-arm64","linux-x64","linux-arm64","windows-x64"], "runtimes": ["android-arm64","android-x64"] },
    "wasm":    { "archive": "z42-workload-0.3.5-wasm.tar.gz",    "sha256": "…", "host": ["*"], "runtimes": ["browser-wasm"] },
    "desktop": { "archive": "z42-workload-0.3.5-desktop.tar.gz", "sha256": "…", "host": ["*"], "runtimes": [] }
  }
}
```
> **一个 workload → 多 target RID（apphost-as-config 后续, 2026-06-17）**：`workloads.<wl>.runtimes` 列出该 workload 安装时需一并拉的 target runtime pack RID。`z42 workload install android` 读 `["android-arm64","android-x64"]` → 拉全部 ABI 的 runtime + workload tooling 包（一个 workload 统一多 ABI，对齐 `dotnet workload install android` 带全 ABI）。ios 的 `["ios-arm64","iossim-arm64"]` = 真机 + 模拟器。
> **desktop = 单 RID-agnostic workload + 跨平台输出（unify-platform-deploy-rid, 2026-06-17）**：desktop workload
> 是**一个包** `z42-workload-<v>-desktop`，携带**每个桌面 RID 的** apphost stub `apphost-<rid>`（CI 把 4 RID
> 的 stub 合并成一个包）；**无 target runtime pack**（`runtimes:[]`，复用 host runtime）→ schema 单 `archive` +
> `host:["*"]`（任意 host 可装）。`publish <toml> --rid <rid>` 取目标 RID 的 `apphost-<rid>` → **任意 host 产其它
> 平台 apphost**（patch 跨平台可行）。apphost **经 desktop workload 交付**，不 baked（Decision 9 完全门控）。
> **限制**：macos 目标 apphost 须 ad-hoc codesign（codesign 仅 macos）→ 跨产 macos apphost 仍需 macos host；
> linux/windows 任意 host 可产（见 Deferred）。
> **已实施**：① desktop RID 双键（sdk/runtime）、platform RID 单键（runtime）、`_fetchManifest` new/old 双格式（split-release-runtime-package, 2026-06-14）。② **`workloads` 段（add-workload-manifest-install, 2026-06-17, B2-4）**：CI release.yml emit `workloads.<wl>.{archive, sha256, runtimes}`；launcher `_fetchWorkloadEntry` 解析 → 联网装。

- `archive` 自带类型（`.tar.gz`/`.zip`）→ 统一解压逻辑，顺手解决 Windows `.zip`。
- `sha256` 内置 → 校验不再单独读 SHA256SUMS（SHA256SUMS 可作并行 legacy 资产保留）。
- **`workloads.<wl>.host`（已实施，add-workload-host-gate, 2026-06-17）**：哪些 host RID 能装该 workload
  （ios=`["macos-arm64"]`、android=全桌面 RID、wasm=`["*"]` 通配）。**联网装**前 launcher `_hostAllowed` 校验
  `_hostRid()` ∈ host（或 `"*"`），不通过即拒、不下载。`host` 缺失 = 不限制（向后兼容）。本地 `--from` 装不 gate。

## launcher / runtime 拆分 + 首次 bootstrap

**当前两包格式**：SDK 与 runtime 独立发布；launcher 不单独发包（`z42 self-update` 下 SDK 包即可）。
- **SDK package** = 原生 apphost（根 `z42`）+ `programs/launcher/launcher.zpkg` + `bin/z42vm` + z42c + libs；`install-z42.sh` 和 `z42 self-update` 都下这个。
- **runtime package** = z42vm + **native/（嵌入件 libz42.{a,dylib} + libz42_compression.* + C ABI headers）** + libs；`z42 install <ver>` 下这个。host runtime pack 是**完整可嵌入运行时**（与 ios/android/wasm 的 runtime pack 对称，见下第 142 行），桌面嵌入用户只需此包、不必下整个 SDK。其 `libs/` 与 `native/` 由 `_buildRuntimePackage` **从同次构建的 SDK 包逐字节拷贝**（单一字节源 → SDK 与 runtime pack byte-identical；rework-host-runtime-pack, 2026-06-21）。

**第一次执行（`install-z42`，唯一保留的原生 bootstrap；shell/PS + 系统 curl）一次装完即功能完整**：

```
1. 探测 host RID（uname/arch）
2. curl 拉 channel manifest：releases/latest/download/release-index.json（或 nightly tag）
3. 从 manifest 取 sdk archive + sha
4. 下载 → 校验 → 解压：
     sdk → ~/.z42/z42 + ~/.z42/programs/launcher/launcher.zpkg + ~/.z42/bin/z42vm + libs
           config.toml default=<ver>
5. ~/.z42 + ~/.z42/bin 入 PATH（或打印指引）
6. 完成 —— z42 list / run / build 立即可用
   （项目本地变体：装进 <repo>/.z42，隔离/pin，沿用现状）
```

**之后全部 z42 驱动**（跑在已装 runtime 的 vm 上）：`z42 install / update / self update / workload install / …`。

**为何这样**：bootstrap 下载交 shell/系统 curl（install-z42 本就是 shell）→ **对齐 dotnet**（shell 装 SDK → 之后托管命令管理）。launcher 逻辑仍是 z42（满足 z42 优先）。
**边角**：`z42 uninstall` 删光最后一个 runtime → launcher 无 vm 可跑 → 挡一下（拒删最后一个 / 提示重跑 install-z42）。dotnet 同性质。

## `$Z42_HOME` 布局

```
~/.z42/
  z42                      launcher 原生 apphost（薄；2026-06-20 trampoline → apphost）
  bin/z42vm                launcher 运行时 VM
  programs/launcher/launcher.zpkg   launcher 逻辑（z42）
  programs/<cmd>/          SDK 命令（跟 SDK 走，版本无关）
  config.toml              default = "<ver>"  ·  channel = stable|nightly
  cache/                   下过的归档（校验通过再解压；断点/复用）
  runtimes/
    <ver>/                 ① host runtime（z42vm + libs + z42c；不含 launcher）
      workloads/<wl>/      ③ 版本作用域目标产物（target runtime pack + 平台命令；ABI 配套 <ver>）
  tools/                   全局用户工具（类 ~/.cargo/bin，版本无关）
```

三类可安装物：**① runtime 版本（side-by-side）· ② launcher 自身（apphost + launcher.zpkg，无 vm）· ③ workload（挂某 runtime 版本下）**。

## install 机制（runtime/workload 分包 + 平台铺设策略 + 联网/本地两源）

`z42 workload install <wl> [--from <tooling>] [--runtime <pack>] [--base-url <url>] [--version <ver>] [--rid <rid>]`
（launcher_workload.z42）。**两种来源、同一铺设**（`_bedRuntimeIntoWorkload`）：

- **本地（B2，`--from`/`--runtime` 指本地产包目录）** —— 见下「本地两步」。
- **联网（B2-4，无 `--from`/`--runtime`）** —— 读 `release-index.json`（GitHub release tag，或 `--base-url`
  指镜像/本地）→ `_fetchWorkloadEntry` 取 `workloads.<wl>.{archive, sha256, host, runtimes}` → **host gate**
  （`_hostAllowed`：`_hostRid()` ∈ `host` 或 `"*"`；不通过即拒、不下载）→ 下载+校验+解压
  tooling 到 `runtimes/<ver>/workloads/<wl>/`（staging `.stage` + 原子 `File.Move`）→ **遍历 `runtimes` 列表**：
  每个 rid 读 `runtimes.<rid>.runtime` → 下载+校验+解压到 `runtimes/<rid>/<ver>/` → 立即跑同一平台铺设。
  即「按需自动拉 runtime」（Decision 10）；android 双 ABI / ios 真机+模拟器都由 `runtimes` 列表逐个拉+铺。
  `--base-url <root>` 时取 `<root>/release-index.json` 与 `<root>/<archive>`（对齐 GitHub 资产布局，故本地
  `python3 -m http.server` 起 `<root>/<tag>/` 即可验证）。校验失败（checksum mismatch）即中止，不留半装态。

runtime pack 与 workload tooling **分两个包**（独立版本管理——runtime ABI 配 z42vm 版本，tooling 是工程模板）：

```
runtimes/<rid>/<ver>/         ← runtime pack（z42-runtime-<ver>-<rid>）：native/ + libs/ + headers
runtimes/<ver>/workloads/<wl>/ ← workload tooling（gradle 工程 / SwiftPM / npm 包）
```

**安装两步**（解耦，支持一 workload 多 RID）：

1. `--from <tooling>` →（重）装 workload tooling（先 `Delete(wlDest)` 再 `_copyTree`）。
2. `--runtime <pack> --rid <rid>` → 把 runtime pack 拷进 `runtimes/<rid>/<ver>/`，再按平台把其内容
   **铺进 tooling**（铺设策略由 runtime pack 内容探测）：

| 平台 | 探测 | 铺设策略 |
|------|------|---------|
| ios | `native/Z42VM.xcframework` | 改写 tooling `Package.swift` 的 `__Z42_RUNTIME_XCFRAMEWORK__` 占位 → **相对路径** `../../../<rid>/<ver>/native/Z42VM.xcframework`（SwiftPM binaryTarget 要求相对 package root） |
| wasm | `pkg-web/` | 把 runtime 的 `pkg-web`/`pkg-nodejs` **symlink** 进 tooling 根（package.json `exports` root-相对原样解析，免改写） |
| android | `native/libz42_platform_android.so` | per-ABI `.so` → `z42vm/src/main/jniLibs/<abi>/`（jniLibs.srcDirs+CMake 拾取）；stdlib zpkgs → `z42vm/src/main/assets/stdlib/`（assets.srcDirs 烘入 AAR） |

**一 workload 多 RID（android 真机要全 ABI）**：tooling 装一次（带 `--from`），之后每个 RID 的 runtime
**只带 `--runtime`/`--rid`（不带 `--from`）增量叠加**——避免 tooling 重拷 `Delete(wlDest)` 抹掉前一个
RID 已铺的 jniLibs slice。android `build.gradle abiFilters=[arm64-v8a, x86_64]` → AAR 需两 ABI 的 `.so`
同时在场 → 必须先装 android-arm64 再增量装 android-x64，`gradlew :z42vm:assembleDebug` 才出全 ABI 的 AAR。

**自包含头约束**：ios `Z42VMC` / android JNI bridge 的 C 头必须是**真实 runtime 头的拷贝**（不是源码树
`#include "../../../runtime/include/.."` 转发桩——桩的相对路径只在 repo 内解析，打包后失效）。`xtask package`
落包时直接拷 `src/runtime/include/{z42_abi,z42_host}.h` 进 tooling 的 include 目录。

## 运行解析顺序（多版本选择）

```
--runtime <ver>                                显式最高
 > <app>.runtimeconfig.json  runtime.version   单 app pin
 > 项目 pin：z42.toml [project].runtime         rust-toolchain.toml 式（复用已有字段）
 > config.toml default
 > 唯一已装
 > 报错（提示 z42 install）
```

- 项目 pin **复用** 已有的 `z42.toml [project].runtime`（平台导出设计已用同字段），不引入新文件。值可为具体版本 `0.3.5`、范围 `0.3.x`、或 channel 名 `stable`/`nightly`。
- 现状只有 `--runtime > default > 唯一`；补 runtimeconfig pin + 项目 pin。

## channel + 更新

- **两个 channel**：`stable`（→ 最新 `v<ver>`）/ `nightly`（滚动）；存 `config.toml channel`。具体版号 pin 仍可用。
- `z42 install <ver|latest|nightly>` → 读对应 manifest 解析版本 → 下 host RID archive → 校验 sha → 解到 `runtimes/<ver>/`。
- `z42 update [--channel …]` → manifest 取 channel 最新 → 比已装新则装 +（可选）重指 default；nightly 用 version / `published` 比对（沿用 bootstrap 已有 published_at 逻辑）。
- `z42 self update` → 换 **launcher bundle**（根 `z42` apphost + `programs/launcher/launcher.zpkg` + `bin/z42vm`）：下 launcher archive → 原子替换（apphost 替换是唯一须碰 native 自替换处）。launcher 复用自带 `bin/z42vm` 跑核心。
- **workload ↔ runtime 版本联动**：workload 嵌入件 ABI 必须配 runtime 版本。升级 runtime → 提示/重装匹配 workload；`z42 workload update` 拉配套版本。

## 命令面（在现有 install/link/list/default/which/info 上扩）

```
z42 install <ver|latest|nightly>        # host runtime
z42 update [--channel stable|nightly]   # (planned) 升级 host runtime — 需 channel→latest 解析（未建）
z42 self update                         # 升级 launcher apphost 自身（已有）
z42 default <ver>                       # 全局默认（已有）
z42 use <ver|stable|nightly>            # (planned) 项目 pin（写 + 解析 z42.toml [project].runtime；解析侧未建）
z42 list [--workloads]                  # 已装版本（+ --workloads 列 workload）（已有）
z42 uninstall <ver>                     # 已有
z42 workload install <wl> [--from <dir>] [--runtime <dir>] [--base-url <url>] [--version <ver>]   # 已有
z42 workload list | uninstall <wl>      # 已有（uninstall 与 runtime 的 z42 uninstall 一致；was remove）
z42 workload update <wl>                # (planned) 重拉已装 workload 至新版 — 需版本解析（未建）
z42 publish <toml> --rid <rid>                # 平台部署件（desktop=apphost；rid 的 category 选平台）
z42 export  <toml> --rid <rid>                # 原生 IDE 工程（ios/android/wasm）
z42 run     <toml> --rid <rid>                # 部署形态预览（vs `z42 run <app.zpkg>` 跑字节码）
z42 run / z42 <app.zpkg>                # 解析版本 → 跑（已有）
```

平台部署命令统一为 `publish/export/run <toml> --rid <rid>`（rid 的 category 选平台，全平台一条命令，不增子命令）；
所依赖的 workload/SDK 由 `workload install` 提供；`install/update/self/default/use/list/uninstall` 属 launcher core（baked-in）。

## 完整性 / 信任

- 现：`sha256`（内置 manifest）+ SHA256SUMS（legacy 并行）。
- 延后：manifest + archive 签名（GPG / sigstore）——与 roadmap `binary-package-signing` 延后一致。

## Decisions

| # | 决定 | 理由 |
|---|------|------|
| 1 | host runtime（`install`）与目标平台产物 workload（`workload install`）二分 | "不同平台"干净落位；对齐 dotnet SDK + workload |
| 2 | 每 release 发 `release-index.json` manifest 作供给契约 | 稳定/可缓存/可离线/可签名；避开裸 API 的 rate-limit + 契约不稳（rustup/dotnet 同选）|
| 3 | channel 解析借 GitHub `/latest/` 重定向 + 固定 `nightly` tag | 无需 API 枚举，stable URL 即解析入口 |
| 4 | 两 channel（stable/nightly）+ 项目 pin 复用 `z42.toml [project].runtime` | 最少概念；不引入新 version 文件 |
| 5 | ~~workload 版本作用域（挂 `runtimes/<ver>/`）+ 与 runtime 版本联动~~ **（2026-06-20 改向：workload 跟 SDK 走，落 `$Z42_HOME/workloads/`，暂不绑 runtime 版本）** | 原由：嵌入件 ABI 须配 runtime。**改向理由**：简化——单 SDK 期无多版本 ABI 冲突；版本作用域延后到真冲突时（`workload-future-version-scoped`）。**注**：当前 install 代码仍铺到 `runtimes/<ver>/workloads/`，迁移到 `$Z42_HOME/workloads/` 为后续 change |
| 6 | launcher 与 runtime 拆分；**launcher 不带 vm，复用已装 runtime 的 vm** | 不必为 launcher 造/最小化 vm；NativeAOT 后转原生 |
| 7 | 首次 `install-z42` 一次装 launcher + host runtime（manifest 驱动），之后全 z42 驱动 | "第一次执行完成安装"；下载交 shell（无需带 TLS 的 launcher vm），对齐 dotnet |
| 8 | `runtime` 为 per-RID 名词/组件，但 `install` 只装 host；target runtime 仅经 workload 组合进 | 命名诚实 + 用户面无冗余直装路径 |
| 9 | desktop 亦 workload（仅 publish/export 维度，不含 runtime）；apphost = desktop workload 的 publish 产物 | 四平台 publish/export 对称门控；"默认 build/run 零 workload"立柱；apphost 与 .ipa/.aab/wasm 同层（consolidate-platform-into-workload, 2026-06-17）|
| 10 | workload 按需自动拉取其依赖的 target runtime：用到某平台而对应 runtime pack 未装 → 自动下载（manifest 驱动 + sha 校验），无需用户先手动 `install` | 对齐 dotnet workload（装 workload 自动带 runtime pack）；用户心智 = "要哪个平台就声明，工具补齐依赖"。desktop workload 例外：不含 runtime（复用已装 host runtime）（apphost-as-config, 2026-06-17）|

## Deferred / 待 spec 细化

> **workload install 现状（2026-06-17）**：①三平台 LOCAL produce + `--from`/`--runtime` 装（impl-workload-install
> B2 已归档）。②**联网装 + CI workloads 段（add-workload-manifest-install B2-4 已落地）**：`release.yml` emit
> `workloads.<wl>` + 平台 runtime pack 双归档；`workload install <wl>`（无 `--from`，可 `--base-url`）联网拉。
> 机制见上「install 机制」节。下列为后续 change：

- **B1 命令发现（discovery-based dispatch）**：现 `workload`/`export`/`publish` 命令 baked 进
  launcher_cli.z42；B1 改为从已装 workload 动态发现命令（对齐 dotnet workload）。属 CLI dispatch
  机制变更，需独立 spec。
- **B4 测试经 workload**：`z42 test` 的平台侧（on-device / 模拟器）测试改走已装 workload tooling。
- **B5 mobile publish/run 生命周期**：`z42 publish <toml> --rid ios-arm64` 产 .ipa/.aab + `z42 run … --rid` 设备侧部署（现 `publish/run --rid` 对 mobile 报 B5 未实装）。
- **跨产 macos apphost 的 codesign**：`publish --rid macos-*` 在非 macos host 上 patch 成功但无法 ad-hoc codesign（macos-only）→ 现报清晰错误。待远程签名 / 在 linux 上签 Mach-O（参 apphost cross-sign 延后）。
- **真实 iOS 多-slice xcframework**：B2 用 macos 单 slice 验机制；device+sim 多 slice 合并归 CI。
- **launcher 最小 vm → 由 NativeAOT 原生 launcher 取代**：本设计 launcher 已不带 vm（复用 runtime vm），故现在无需最小化 vm；NativeAOT 落地后把 launcher AOT 成原生二进制（rustup 式），vm 问题永久消失。届时 `install-z42` 可只下原生 launcher，bootstrap 更轻。
- **`workload-future-version-scoped`（workload↔runtime ABI 版本作用域，2026-06-20）**：现 workload 跟 SDK 走（`$Z42_HOME/workloads/`，版本无关，简化）。当多 runtime 版本共存且 native 嵌入件（xcframework/AAR）ABI 与某 runtime 版本真冲突时，回到版本作用域（挂 `runtimes/<ver>/workloads/`）+ workload↔runtime 联动。**前置依赖**：多 runtime 版本共存 + 跨版本 ABI 实际不兼容。**当前 workaround**：单 SDK 期跟 SDK 走；install 代码现仍铺 `runtimes/<ver>/workloads/`（待迁移到 `$Z42_HOME/workloads/`）。
- **`runtime-future-jit-cdylib-split`（z42vm 动态瘦身：JIT 拆 cdylib，2026-06-21）**：把 cranelift JIT 从 z42vm 拆成可 dlopen 的 `libz42_jit.dylib`，`--mode jit` 时加载；z42vm 6M→~3.5M + 新增 ~2.5–3M dylib。**触发原因（ROI 低，2026-06-21 User 决策暂不做）**：整包 ~70M（`libz42_compression.a` 20M + `libz42.a` 10M 等静态归档才是大头），拆 JIT 顶多省 ~3M 且多一个 dylib，包级收益是噪声；JIT↔interp 有共享代码（`exec_vcall` 等）+ 需设计符号导出协议，中高工作量。**前置依赖**：dlopen 框架已有（`native/ext.rs`）；需把 JIT 提取为独立 crate + 符号导出协议 + 拆 interp↔jit 共享代码。**触发条件**：z42vm 单二进制体积真正成为某分发渠道约束（如某平台对二进制大小敏感、或要 interp/jit 按需下载）。**当前 workaround**：mobile/wasm 已 `--no-default-features --features interp-only`（~3M、无 JIT）；桌面默认 full（6M）JIT 静态链接。**目标架构**：完整组件化设计（libz42 基座 + interp/jit/aot/gc/debug 组件 + static/dynlink/dlopen 三粒度 + 切换语义）见 [componentized-runtime.md](../../internals/src/runtime/componentized-runtime.md)；本条是该架构的第一个落地切口（拆 JIT）。
- manifest schema 定稿（多 archive 类型、可选 z42c-less 精简 runtime、依赖/兼容区间）。
- self update 的原子替换 + 回滚（Windows 文件占用、权限）。
- workload↔runtime 版本兼容矩阵与升级时的自动重装策略。
- 离线/镜像源（企业内网 mirror base URL）。
- 签名/校验链（manifest 签名 + archive 校验）。
- `z42 use` 范围 pin（`0.3.x`）的解析与自动安装策略。
