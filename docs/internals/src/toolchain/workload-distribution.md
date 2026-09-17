# workload 分发：装什么、从哪来、怎么铺

> 对齐：2026-09-17（change `restructure-docs-three-books`）｜ 代码：`src/toolchain/launcher/core/launcher_workload.z42`、`src/toolchain/launcher/core/launcher_network.z42`、`scripts/package/xtask_release.z42`
>
> 命令与旗标怎么敲 → [工具链参考](../../../reference/src/toolchain/README.md)；
> 谁在消费这些 workload → [平台发布与导出](platform-export.md)。

默认装一个 SDK 就能 `build` / `run` / `test`，**零 workload**。
要导出平台工程、产平台发布件、或在设备上跑测试，才按需 `z42 workload install <名字>`。
这页写 workload 是什么形状、从哪个契约拉下来、装完怎么和已装的运行时拼在一起。
**加平台、改发布资产、查「装完了为什么还是找不到 xcframework」时读这页。**

## 两类 workload

| 类 | 成员 | 内容 |
|---|---|---|
| **平台 workload** | `ios` / `android` / `wasm` / `desktop` | 平台工具链工程（SwiftPM / Gradle / npm 包）+ 模板 + 原生绑定；ios/android/wasm 另带该平台的 **runtime pack** |
| **能力 workload** | `test` | 平台无关的共享载荷（on-device test-agent，一份字节码全平台通用） |

`desktop` 是平台 workload 里的例外：它**不含 runtime pack**（桌面复用宿主已装的运行时），
只带每个桌面 RID 的 apphost stub `apphost-<rid>`。这也是为什么 desktop workload 是
**一个 RID 无关的包**，而 ios/android 的 workload 会拉多份 runtime。

能力 workload 是 **payload-only 形状**：只有一个 zpkg，没有 per-RID runtime pack，
`host: ["*"]`。它复用同一套 install 命令，靠 manifest 里的名字驱动。

## runtime 是 per-RID 的名词，不是第二条安装入口

`runtime` 指某个 RID 的 z42vm 与嵌入件。宿主 RID（macos/linux/windows）和目标 RID
（ios/android/wasm）**都是 runtime**，在供给契约里都是 `runtimes` 段下的条目。

但用户面**只有两个入口**，目标 runtime 不开第二条直装路径：

| 入口 | 装什么 |
|---|---|
| 安装脚本（`install.sh` / `install.ps1`） | 宿主 SDK：z42vm + libs + 各工具 apphost |
| `z42 workload install <名字>` | 能力束 = {目标 runtime pack} + {工具链工程 + 模板 + 原生胶水} |

目标 runtime 是 workload 内部组合进来的**组件**，不作为独立命令暴露。
对标 dotnet：你 `dotnet workload install ios`，ios runtime pack 是 workload 内的 component，
从不 `dotnet install ios-runtime`。

launcher **不管理运行时版本**，也没有 `z42 install` / `update` / `self-update`——
SDK 是单版本的，由安装脚本装与更新。

## 供给契约：`release-index.json`

不裸爬 GitHub API（rate-limit、契约不稳、难离线、难签名）。每个 release 上传一个
`release-index.json` 资产作为稳定契约（对标 rustup channel manifest / dotnet release-index）。

生成方是 `xtask package index`（`_releaseGenIndex`），release 与 nightly 共用同一份实现，
**schema 只有这一处定义**。当前形状：

```json
{
  "schema": 1,
  "version": "0.6.0", "channel": "stable", "tag": "v0.6.0", "published": "…",
  "runtimes": {
    "macos-arm64":   { "sdk": { "archive": "…", "sha256": "…" },
                       "runtime": { "archive": "…", "sha256": "…" } },
    "ios-arm64":     { "runtime": { "archive": "…", "sha256": "…" } }
  },
  "workloads": {
    "ios":     { "archive": "…", "sha256": "…", "host": ["macos-arm64"],
                 "runtimes": ["ios-arm64", "iossim-arm64"] },
    "desktop": { "archive": "…", "sha256": "…", "host": ["*"], "runtimes": [] }
  }
}
```

- **4 个桌面 RID 有 `sdk` + `runtime` 两个键**（linux-x64 / linux-arm64 / macos-arm64 / windows-x64）；
  **5 个平台 RID 只有 `runtime`**（ios-arm64 / iossim-arm64 / android-arm64 / android-x64 / browser-wasm）。
- **5 个 workload 条目**：ios / android / wasm / desktop / test。
- `archive` 自带扩展名（windows 是 `.zip`，其余 `.tar.gz`）→ 解压逻辑按名分派，顺手解决 Windows。
- `sha256` 内置 → 校验不必再单独读 `SHA256SUMS`（后者作为并行资产保留）。
- `workloads.<wl>.runtimes` 列出装它时要一并拉的目标 runtime RID：
  android 是两个 ABI、ios 是真机 + 模拟器——一条命令带齐，对齐 `dotnet workload install`。
- `workloads.<wl>.host` 是**宿主白名单**（`"*"` = 不限）。

生成器在写 JSON 前会**逐条校验 SHA256SUMS 里有对应的 sha**，缺一条就整体失败。
少了这道校验，发出去的 index 会指向不存在的资产，而错误要到用户安装时才暴露。

## 安装：两种来源，同一套铺设

```
z42 workload install <wl> [--from <tooling-dir>] [--runtime <pack-dir>]
                          [--base-url <url>] [--version <ver>] [--rid <rid>]
```

**本地**（给了 `--from` 或 `--runtime`）：直接从 `xtask package` 产出的目录拷。
`--from` 会**重装**工具链工程（先删后拷）；只给 `--runtime` / `--rid` 则是**增量**往已装的
工具链里再叠一个 RID 切片。版本取 `--version`，缺省从 `--from` 目录的 `manifest.toml` 读。

**联网**（两者都没给）：`--version` 必填。按版本拼 tag（`nightly` 或 `v<ver>`），
默认 base URL 是 GitHub release 的下载目录，`--base-url` 可指向镜像或本地
（取 `<root>/release-index.json` 与 `<root>/<archive>`，对齐 GitHub 的资产布局，
所以本地起一个静态 HTTP 服务器就能验证整条链路）。流程：

1. 读 index，取 `workloads.<wl>` 条目；
2. **host gate 在任何下载之前**：宿主 RID 不在 `host` 里（且不是 `"*"`）就拒绝、不下载。
   本地 `--from` 装不做这道 gate；
3. 下载工具链归档 → 校验 sha → 解到 `.stage` 暂存目录 → 原子 `File.Move` 就位；
4. 遍历 `runtimes` 列表，每个 RID 同样「下载 → 校验 → 暂存 → 移动」，
   **装完一个就立即铺进工具链**。

校验失败即中止，不留半装状态。暂存 + 原子移动是为了让「装到一半被打断」不会产生一个
看起来装好了、其实内容残缺的目录。

## 磁盘布局与铺设

runtime pack 和工具链是**两个包**，落在两处：

```
<SDK 根>/runtimes/<rid>/<ver>/          ← runtime pack：native/ + libs/ + 头文件
<SDK 根>/runtimes/<ver>/workloads/<wl>/ ← workload 工具链：SwiftPM / Gradle 工程 / npm 包
```

分包是为了独立版本管理：runtime 的 ABI 配 z42vm 版本，工具链是工程模板，两者节奏不同。

「铺设」（`_bedRuntimeIntoWorkload`）把前者接进后者。**策略由 runtime pack 的内容探测出来**，
不看名字：

| 探测到 | 平台 | 铺设 |
|---|---|---|
| `native/Z42VM.xcframework` | ios | 把工具链 `Package.swift` 里的 `__Z42_RUNTIME_XCFRAMEWORK__` 占位改写成**相对路径** `../../../<rid>/<ver>/native/Z42VM.xcframework` |
| `pkg-web/` | wasm | 把 runtime 的 `pkg-web` / `pkg-nodejs` **symlink** 进工具链根（`package.json` 的 exports 按根相对解析，无需改写） |
| `native/libz42_platform_android.so` | android | per-ABI `.so` → `z42vm/src/main/jniLibs/<abi>/`；stdlib zpkg → `z42vm/src/main/assets/stdlib/`（Gradle 把两者都烘进 AAR） |

iOS 用相对路径不是风格问题：SwiftPM 的 `binaryTarget` **要求**路径相对 package root。
`workloads/<wl>/` 在 `runtimes/` 下三层，所以是 `../../../`。

**一个 workload 多个 RID**（android 真机要全 ABI）：工具链装一次（带 `--from`），
之后每个 RID 只带 `--runtime` / `--rid` **增量叠加**。这条顺序是强制的——
带 `--from` 会先删掉整个工具链目录，把前一个 RID 已铺好的 jniLibs 切片一起抹掉。
Gradle 的 `abiFilters` 要求两个 ABI 的 `.so` 同时在场才出全 ABI 的 AAR。

stdlib zpkg 跟着 **runtime pack** 走而不是工具链：它们与 runtime 版本锁死。

**自包含头约束**：ios 的 `Z42VMC` 与 android 的 JNI bridge 用的 C 头必须是真实运行时头的**拷贝**，
不能是 `#include "../../../runtime/include/…"` 这样的转发桩——桩的相对路径只在仓库里解析得开，
打包之后就断了。`xtask package` 落包时直接把 `src/runtime/include/{z42_abi,z42_host}.h` 拷进
工具链的 include 目录。

## 代码地图

| 组件 | 位置 |
|---|---|
| `workload install/list/uninstall`、铺设策略 | `src/toolchain/launcher/core/launcher_workload.z42` |
| index 读取、下载校验、tar.gz / zip 解包 | `src/toolchain/launcher/core/launcher_network.z42` |
| `release-index.json` 生成与校验 | `scripts/package/xtask_release.z42` |
| workload 包的组装 | `scripts/package/xtask_package_desktop.z42` |
| 各 workload 的内容 | `src/toolchain/workload/<plat>/`、`src/toolchain/workload/test/` |

## 边界与限制

- **workload 仍挂在 `runtimes/<ver>/` 下**，即版本作用域的旧布局；
  「跟 SDK 走、落 `<SDK 根>/workloads/`」的裁决还没落到代码里。
  workload 与 runtime 的 ABI 版本联动也没做——单 SDK 期不会冲突，多版本共存且 ABI 真冲突时才需要。
- **没有 `z42 workload update`**：升级 = 重新 `install`。
- **命令不是发现出来的**：`export` / `publish` / `workload` 全部 baked 在 launcher 的命令树里，
  平台工程生成器也是 launcher 的编译期依赖（见 [export](export.md#谁拥有生成器)）。
  「从已装 workload 动态发现命令」是设想，不是现状；加一个平台仍需重编 launcher。
- **信任链只到 sha256**：manifest 与归档本身没有签名。
- **卸载只删工具链目录**，铺进去的 runtime pack（`runtimes/<rid>/<ver>/`）留在原地。
