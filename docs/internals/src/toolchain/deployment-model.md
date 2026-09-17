# 部署模型：把一个 app 交出去的五条轴

> 对齐：2026-09-17（change `restructure-docs-three-books`）｜ 代码：`src/toolchain/builder/core/builder_publish.z42`、`src/toolchain/workload/desktop/`
>
> `[platform.desktop]` 有哪些键 → [`z42.toml` 参考](../../../reference/src/toolchain/z42-toml.md)；
> 谁产出这些形态 → [平台发布与导出](platform-export.md)。

一个 z42 应用要跑起来，需要三样东西就位：**z42vm**（执行 zbc）、**app 自身与私有依赖的 zpkg**、
**stdlib**。部署就是决定这三样从哪来、怎么打包、如何被应用找到。这页把这些选择拆成五条尽量正交的轴，
并说明哪几处物理上必然耦合。**加新的 publish 形态、判断某个组合合不合法时以这页为坐标系。**

## 五条轴

| 轴 | 取值 | 决定什么 |
|---|---|---|
| **A 运行时供给** | `shared` / `self-contained` | z42vm + stdlib 来自目标机已装的 SDK，还是随 app 打包 |
| **B 运行时链接**（仅 A=self-contained） | `static` / `dynamic` | libz42 静态链入 exe，还是旁挂共享库 |
| **C 宿主形态** | `zpkg` / `apphost` | 裸 zpkg（经 launcher 跑）还是原生启动器 exe |
| **D 文件合并** | `loose` / `single-file` | 目录多文件，还是单个物理文件 |
| **E 执行模式** | `interp` / `jit` / `aot` | 字节码怎么执行 / 是否编到原生机器码 |

对标 .NET 的部署模型（framework-dependent / self-contained / apphost / single-file / NativeAOT），
取其正交经验，避免冗余选项。

### A — 运行时供给

- **`shared`（默认）**：目标机装了 z42 SDK。apphost 按[探测顺序](launcher.md#z42vm-探测所有原生入口共用一套)
  找到外部 `z42vm` 并 `exec` 它（同时设 `Z42_LIBS`），stdlib 从 SDK 的 `libs/` 解析。部署体积最小。
- **`self-contained`**：z42 运行时随 app 打包，目标机无需装 z42。经 `z42 publish --self-contained` 产出，
  **in-process 运行**（`z42_host_run_app`），不 spawn 外部 vm。

两者的产物是**两个不同的原生程序**，不是同一个 stub 的两种配置：

| | shared | self-contained |
|---|---|---|
| exe 来自 | apphost **stub** + patch 内嵌路径 | 预编好的 **embed apphost**，直接拷 |
| exe 怎么找 app | 内嵌的「相对自身目录的 zpkg 路径」 | 固定名 `app.zpkg`（与 exe 同目录） |
| stdlib | 目标机 SDK 的 `libs/` | 随包的 `libs/` |
| 怎么跑 app | `exec z42vm <zpkg>` | 进程内调 `z42_host_run_app` |

embed apphost（以及 `dynamic` 时的 `libz42.<dyn>`）由 desktop workload 提供；
开发流也可经工程 `[build] hooks` 现场产出（注册 `embed-apphost-<link>` / `embed-libs` /
`embed-dylib` 三类 Output），或用 `Z42_EMBED_*` 环境变量直接指定。

### B — 运行时链接

`[platform.desktop] link` = `static`（默认，libz42 静态链入 exe）或 `dynamic`
（旁挂 `libz42.<dyn>`，运行时加载）。取值只对 self-contained 有意义——
shared 供给下 vm 是外部进程，这条轴不存在。非法取值直接报错退出。

### C — 宿主形态

- **`zpkg`**：产物就是 `dist/<name>.zpkg`，经 `z42 run` 执行，没有原生可执行文件。
- **`apphost`**：`[platform.desktop] apphost = true` 是 publish 产 apphost 的**门**（gate）——
  缺省或 false 时 publish 直接报「not configured to publish a desktop apphost」退出。
  `publish_dir` / `--output` 只是输出位置，不充当开关。

### D — 文件合并

- **`loose`（当前唯一）**：产物是一个目录。shared 布局由 `[platform.desktop]` 的 `bin` / `payload`
  两个相对路径决定（都不设则平铺：exe 落 `<root>/<name>`、zpkg 留在原地被内嵌）；
  self-contained 固定 `<appDir>/{<name>[.exe], app.zpkg, libs/*.zpkg[, libz42.<dyn>]}`。
- **`single-file`（规划）**：把分离的 zpkg（self-contained 时连 libz42 / stdlib）内嵌进 apphost 二进制。
  对齐 .NET `PublishSingleFile`——**内嵌的是分离 zpkg，不是合并成一个 zpkg**。需要 apphost/vm 支持
  「从内嵌 bundle 解析 zpkg」，所以它是运行时侧特性，不是纯 publish 步骤。

  > **为什么不做「把依赖合编进一个 zpkg」**：那会丢模块边界、改 `internal` 可见性语义、
  > 依赖被重编、失去独立增量缓存。.NET 正因这些代价刻意不做 assembly 合并。z42 采同一取舍：
  > 「合成一个文件」归 D 轴的 single-file，不引入托管合并原语。
  > 合成范围只含**私有依赖**；stdlib 永远是独立 zpkg。

### E — 执行模式

`[profile.<n>.runtime] mode`，或运行时 `--mode`：`interp`（默认、最稳）/ `jit`（热路径编到原生）/
`aot`（规划：zbc 提前编译到独立 native）。

**AOT 终局会塌缩 A/C/D**——一个纯 native 二进制天然自包含、单文件、无 vm/zpkg。
所以它不与 A–D 并列，而是「编译到另一种产物形态」。

> **「native」在 z42 有两层含义，别混**：
> - **native 宿主**（C=apphost、A=self-contained）：产物是原生可执行文件，但**应用代码仍是 zbc**，
>   由内嵌或外部 z42vm 解释 / JIT 执行。apphost 是原生启动器，不是「原生化的应用」。
> - **native 编译**（E=aot）：把**应用代码本身**编成原生机器码，不再有 zbc、不需要 vm。
>
> 二者正交。别把「打出了个 exe」当成「AOT 了」。

## 轴之间的耦合

正交是目标，物理约束下有三处必然耦合：

1. **裸 zpkg（C=zpkg）⟹ shared（A=shared）**：没有原生宿主就只能靠外部 vm / launcher。
2. **single-file（D）⟹ apphost（C=apphost）**：得先有原生 exe 才能把 zpkg 嵌进去。
3. **aot（E）塌缩 A + C + D**：独立 native 二进制自然自包含、单文件、无独立 vm/zpkg。

除此之外 A×B×C×D 的其余组合都合法且独立可选。

## 现状

| 轴 | 取值 | 状态 | 承载 |
|---|---|---|---|
| A | shared | ✅ | apphost 默认 `exec` 外部 z42vm |
| A | self-contained | ✅ | `z42 publish --self-contained`（embed libz42，in-process） |
| B | static / dynamic | ✅ | `[platform.desktop] link` |
| C | apphost | ✅ | `[platform.desktop] apphost = true` |
| C | zpkg | ✅ | `z42 run <app.zpkg>` |
| D | loose | ✅ | publish 目录布局 + 私有依赖 colocate |
| D | single-file | 🔴 规划 | 需运行时支持内嵌 bundle 解析 |
| E | interp / jit | ✅ | z42vm |
| E | aot | 🔴 规划 | [AOT 设计](../runtime/aot.md) |

## apphost 是怎么做出来的

shared 路径的 exe 不是编出来的，是**改字节**改出来的：native stub 内嵌一段 1024 字节的占位区
（32 字节 MAGIC sentinel + 992 字节 payload），patcher 按 MAGIC 在文件里定位，
把 payload 覆写成「app zpkg **相对 exe 自身目录**的路径 + NUL + 零填充」，写出、置可执行位。
于是 `<exe + app.zpkg>` 可以整体搬迁。

**macOS 必须重签名**：patch 字节会让 stub 的 ad-hoc 签名失效，内核直接拒绝运行（表现为 hang）。
patcher 在 macOS 目标上 patch 完就 `codesign -s - -f <out>`；先 patch 再签（codesign 只动签名 blob，
不碰 `__DATA` 里的占位区）。`codesign` 只在 macOS 上有，所以**产 macOS apphost 必须在 macOS host 上做**，
patcher 会显式拒绝在别的 host 上产 macos 目标。Linux 无签名；Windows 跑无签名 exe 无碍。

MAGIC 与 patch 逻辑目前有三处副本——Rust stub 是权威（`platform/apphost/src/main.rs`），
desktop workload 的 `Apphost.PatchBytes`、z42b 的内联 patcher 各一份。
z42b 内联而不复用 workload 的那份，是因为 z42b 兼作 stdlib 的测试运行器，
那个上下文的 `Z42_LIBS` 里只有 stdlib，看不到 workload 包。改 MAGIC 要三处同步。

## 私有依赖 vs 标准库

部署时「哪些 zpkg 随 app 走、哪些留外部」由**是否真属标准库**判定，**不看名字前缀**：

- **私有依赖**（本地 path 依赖、不在 `src/libraries/`）：随 app 走。
  shared 布局下 `_pubBundleProjectDeps` 沿 path 依赖闭包把它们平铺进 dist / payload 目录；
  self-contained 下进 `libs/`。名字即便以 `z42.` 开头（如 host-only 的 `z42.repl`）也算私有。
- **标准库**（真属 `src/libraries/` / 随包 `libs/`）：shared 下留在 SDK 的 `libs/` 按名解析，
  self-contained 下作为独立 zpkg 随嵌。

私有 **native** 依赖走另一条窄路：沿同一条闭包只跑每个依赖的 `ProvideNative` 钩子，
把目标 RID 那份产物平铺到 payload 目录，不盲跑通用的 `BeforeAssets`。

运行配置侧车（`<stem>.runtimeconfig.toml`）跟着 zpkg 走，且**按目标 stem 改名**：
自包含布局把 zpkg 改名成 `app.zpkg`，侧车必须同时变成 `app.runtimeconfig.toml`——
发现约定是「同目录、同 stem」，拷过去还叫原名等于没拷，工程 `[profile.*]` 里写的旋钮
在发布出来的二进制上会完全不生效且无提示。

## 边界与限制

- **apphost 不读 `<app>.runtimeconfig.toml` 以外的东西**，也不经 launcher——
  它只做「找 vm + 跑 app」。要 launcher 那一层的解析就用 `z42 run`。
- **single-file 与 AOT 未实现**，见现状表。
- **跨平台产 apphost 的签名缺口**：patch 本身跨平台可行，
  但 macOS 目标的 ad-hoc 签名只能在 macOS host 上做。
