# 部署模型（Deployment Model）

> 对齐：2026-08-29（change `add-path-dependencies` 探索期确立正交轴总纲）
>
> 本页是 z42 应用**如何被打包、分发、在目标机上运行**的总纲：把部署拆成若干**尽量正交**的轴，
> 说明每条轴的取值、现状与规划、以及轴之间不可避免的耦合点。对标 .NET 的部署模型
> （framework-dependent / self-contained / apphost / single-file / NativeAOT），取其正交经验、
> 避免冗余选项。后续所有 packaging / publish 工作以此为坐标系。

## 核心问题

一个 z42 应用（`kind = "exe"` 的 zpkg）要跑起来，需要三样东西就位：

1. **z42vm 运行时**（解释器 / JIT）——执行 zbc 字节码。
2. **应用自身 + 私有依赖**的 zpkg——业务代码。
3. **标准库**（`z42.core.zpkg` 等）——被应用/依赖引用。

"部署"就是决定这三样**从哪来、怎么打包、如何被应用找到**。不同选择组合出 framework-dependent
目录、自包含单目录、单文件 exe 等形态。

## 正交轴（A–E）

| 轴 | 取值 | 决定什么 | .NET 对应 |
|----|------|---------|-----------|
| **A 运行时供给** | `shared` / `self-contained` | z42vm + stdlib 来自目标机已装的 SDK，还是随 app 打包 | FDD / SCD |
| **B 运行时链接**（仅 A=self-contained） | `static` / `dynamic` | libz42 静态链入 exe，还是旁挂共享库（`libz42.<dyn>`） | （z42 特有细分） |
| **C 宿主形态** | `zpkg` / `apphost` | 裸 zpkg（经 launcher 跑）还是 native 启动器 exe | `app.dll` / apphost |
| **D 文件合并** | `loose` / `single-file` | 目录多文件，还是单个物理文件（exe 内嵌全部 zpkg[+libz42]） | 多文件 / `PublishSingleFile` |
| **E 执行模式** | `interp` / `jit` / `aot` | 字节码怎么执行 / 是否编到原生 | JIT / ReadyToRun / NativeAOT |

### A — 运行时供给：shared vs self-contained

- **`shared`（framework-dependent，默认）**：目标机装了 z42 SDK；apphost **spawn 外部 z42vm**、
  stdlib 从 SDK 的 `libs/` 解析。部署体积小，依赖目标机有 SDK。
- **`self-contained`**：把 z42vm（`libz42`）+ stdlib 随 app 一起打包，目标机**无需装 z42**。
  经 `z42 publish --self-contained`，产物 in-process 运行（`z42_host_run_app`），不 spawn 外部 vm。

### B — 运行时链接（仅 self-contained 有意义）

`[platform.desktop] link`：`static`（libz42 静态链入 exe，单可执行）或 `dynamic`（旁挂
`libz42.<dyn>`，exe 运行时加载）。shared 供给下无此轴（vm 是外部进程）。

### C — 宿主形态：zpkg vs apphost

- **`zpkg`**：产物就是 `app.zpkg`，经 `z42 run app.zpkg` / launcher 执行。无 native 可执行文件。
- **`apphost`（`[platform.desktop] apphost = true`）**：产出 native 启动器 exe，双击即跑、
  无需 `z42` 前缀。exe 内嵌"相对自身的 zpkg 路径"，找到 vm 后加载运行。

### D — 文件合并：loose vs single-file

- **`loose`（默认）**：产物是一个目录（apphost exe + `app.zpkg` + 私有依赖 zpkg + `libs/`）。
  应用运行时按 [entry-dir search](../runtime/) 从自身目录、再从 `libs/` 解析依赖 zpkg。
- **`single-file`（规划）**：把分离的 zpkg（+ self-contained 时的 libz42 / stdlib）**内嵌进
  apphost 二进制**，产出单个物理文件。对齐 .NET `PublishSingleFile`——**内嵌的是分离 zpkg，不是
  合并成一个 zpkg**。需 apphost/vm 支持"从内嵌 bundle 解析 zpkg"（类 .NET host 读附加 bundle），
  故 single-file 是 **runtime 侧特性**，非纯 publish 步骤。

  > **为什么不做「single-zpkg 托管合并」**：把私有依赖的 zbc 源级合编进一个 zpkg，虽也能"减少文件"，
  > 但丢模块边界、改 `internal` 可见性语义、依赖被重编、无独立增量缓存。.NET 正因这些代价**刻意不做
  > assembly 合并**（ILMerge 是第三方、劝退），改用 single-file 内嵌分离 assembly。z42 采同一取舍：
  > **"合成一个文件"归 single-file（D 轴），不引入托管合并原语**，避免冗余与语义代价。
  > **合成范围**：只内嵌**私有（path）依赖**；stdlib 永远作为独立 zpkg（self-contained 时随嵌、
  > shared 时留在 SDK）。

> **"native" 在 z42 有两层含义，勿混**：
> - **native 宿主/exe**（C=apphost、A=self-contained+B）：产物是原生可执行文件，但**应用代码仍是
>   zbc 字节码**，由内嵌/外部 z42vm 解释或 JIT 执行。apphost 是"原生启动器"，不是"原生化的应用"。
> - **native 编译（E=aot）**：把**应用代码本身**提前编译成原生机器码，**不再有 zbc / 不需 vm**。
>   这才是 .NET NativeAOT 意义上的"native"。
>
> 二者正交：可以有"native exe 跑 zbc"（今天的 self-contained apphost），也可以有"aot 原生二进制"
> （规划）。别把"打出了个 exe"当成"AOT 了"。

### E — 执行模式：interp / jit / aot

`[profile].mode`：
- **`interp`**：字节码解释执行（默认、最稳）。
- **`jit`**：热路径 JIT 到原生。
- **`aot`（规划 M9）**：zbc 提前编译到独立 native（cranelift-object）。**aot 终局会塌缩 A/C/D**——
  一个纯 native 二进制天然是 self-contained + single-file + 无 vm/zpkg，等价 .NET NativeAOT。
  故 aot 不与 A–D 并列，而是"编译到另一种产物形态"。

## 轴之间的耦合（承认非全正交）

正交是设计目标，但物理约束下有三处必然耦合：

1. **裸 zpkg（C=zpkg）⟹ shared（A=shared）**：没有 native 宿主，就只能靠外部 vm / launcher。
2. **single-file（D）⟹ apphost（C=apphost）**：要有 native exe 才能把 zpkg 内嵌进去。
3. **aot（E=aot）终局塌缩 A + C + D**：独立 native 二进制自然自包含、单文件、无独立 vm/zpkg。

除此之外，A×B×C×D 的其余组合都是合法且独立可选的（如 shared + apphost + single-file =
一个内嵌 app zpkg、但仍找外部 vm 的单 exe；self-contained + static + apphost + loose =
今天的 `--self-contained --link=static` 目录形态）。

## 现状与规划

| 轴 | 取值 | 状态 | 承载 |
|----|------|------|------|
| A | shared | ✅ | apphost 默认 spawn 外部 z42vm |
| A | self-contained | ✅ | `z42 publish --self-contained`（embed libz42，in-proc） |
| B | static / dynamic | ✅ | `[platform.desktop] link` |
| C | apphost | ✅ | `[platform.desktop] apphost = true` |
| C | zpkg | ✅（隐式） | `z42 run app.zpkg` |
| D | loose | ✅ | publish 目录布局 + 私有依赖 colocate（`_pubBundleProjectDeps`） |
| D | single-file | 🔴 规划 | 需 runtime 支持内嵌 bundle 解析；独立 change |
| E | interp / jit | ✅ | z42vm |
| E | aot | 🔴 规划 M9 | cranelift-AOT（[aot 设计](../runtime/)） |

## 私有依赖 vs 标准库（打包判据）

部署时"哪些 zpkg 随 app 走、哪些留外部"由**是否真属标准库**判定，**不看名字前缀**：

- **私有依赖**（本地 [path 依赖](./)、不在 `src/libraries/`）：随 app colocate（D=loose）或内嵌
  （D=single-file）。名字即便以 `z42.` 开头（如 host-only 的 `z42.repl`）也是私有。
- **标准库**（真属 `src/libraries/` / shipped `libs/`）：shared 供给下留 SDK `libs/` 按名解析；
  self-contained 供给下作为独立 zpkg 随嵌。

> 引入/演进：change `add-path-dependencies`（path 依赖 + 真-stdlib 打包判据 + colocated）。
> single-file 与 aot 为规划轴，各自独立 change。

## 关联文档

- [SDK 与发行包布局](./README.md) —— 目录/包结构（apphost bin/payload、libs 布局）
- [运行时](../runtime/README.md) —— z42vm 执行模式（interp/jit/aot）、zpkg 加载与 entry-dir search
- [xtask 构建编排](../dev/build.md) —— toolchain / apphost 构建流程
