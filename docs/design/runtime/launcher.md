# z42 launcher (`z42`)

> 长期规范。来源 spec：`docs/spec/changes/add-z42-launcher/`（add-z42-launcher, 2026-06-02）。

## 定位

`z42` 是用户**一次性安装的唯一入口**：给定一个 z42 应用（Exe-mode zpkg），解析它需要的运行时版本，用对应的 `z42vm` 跑起来，并把命令行参数透传给程序；同时管理本机已装的多个运行时版本。类比 `dotnet` muxer + `rustup`。

**设计铁律 —— z42 优先**：bootstrap 约束（没有 VM 就跑不了 z42）决定"找/给 VM"的**最小核**必须原生；除此之外**全部逻辑用 z42 写**。因此 launcher 拆成两层：

```
z42  (apphost, Rust；payload → programs/launcher/launcher.zpkg；2026-06-20 trampoline → apphost)
      │  probe z42vm = {exe_dir}/bin/z42vm；加载 programs/launcher/launcher.zpkg
      ▼
z42vm programs/launcher/launcher.zpkg  --  <用户 argv 原样>   ← 之后全是 z42 代码
      │
      ▼
launcher 核心 (z42 → launcher.zpkg, Exe-mode)
      │  解析 argv / 子命令 / 读 ~/.z42 / 解析版本
      │  run: Std.IO.Process.Spawn
      ▼
$Z42_HOME/runtimes/<ver>/z42vm  <app.zpkg>  --  <app args>
```

- **launcher 运行时**（`$Z42_HOME/bin/ + $Z42_HOME/programs/launcher/`）：随 launcher 一起装的固定 `z42vm + launcher.zpkg + libs`，**只用来跑 launcher 核心自己**，避免"跑 launcher 需要先选运行时"的鸡生蛋。
- **app 运行时**（`$Z42_HOME/runtimes/<ver>/`）：受 launcher 管理，用来跑用户 app。

`z42` apphost（统一前称 trampoline）永远用 colocated 的 launcher 运行时跑核心，**不随 release 变**；所有行为都在 `launcher.zpkg` 里，可单独升级。**统一为单一 apphost stub**（去掉独立 trampoline 实现）：`z42` = payload 指向 `launcher.zpkg` 的 apphost，与 per-app apphost 同一个 stub、同一套 z42vm 探测（见下「运行时解析」），仅 payload 不同。

## 依赖前置：z42vm 透传命令行参数（phase 0）

`z42vm` CLI 末尾接受 `-- <args>`（clap `last = true`），存入 `VmCore.program_args`，由 `__env_args` builtin → `Std.IO.Environment.GetCommandLineArgs()` 返回。**只有 `--` 之后的 token** 是程序参数；VM 自身的 `file/entry/--mode` 不在内；无 `--` 则为空。

> 这是参数透传的**永久归属**：放在 Rust 运行时（自举只重写编译器，不重写 VM），**不放 z42c**（编译器会被 z42 重写）。

## 磁盘布局（`$Z42_HOME`，默认 `~/.z42`）

```
~/.z42/
├── z42                         # apphost（根，在 PATH 上；原 trampoline，2026-06-20）
├── bin/
│   ├── z42c                    # apphost → ../programs/z42c/z42c.driver.zpkg
│   ├── z42vm                   # VM 主进程（也在 PATH 上）
│   └── apphost                 # stub 模板（z42 publish 用）
├── libs/                       # stdlib zpkg
│   └── z42.*.zpkg
├── programs/                   # SDK 内置命令 zpkg 实现（跟 SDK 走，版本无关，2026-06-20）
│   ├── launcher/               #   launcher 核心（muxer）
│   │   └── launcher.zpkg
│   ├── z42c/                   #   z42 build = 编译器
│   │   └── z42c.*.zpkg
│   ├── repl/                   #   z42 repl
│   │   └── z42.repl.zpkg
│   └── <cmd>/                  #   其余 SDK 命令（test / fmt / new …）：<cmd>.zpkg + <cmd>.cmd.toml
├── workloads/<wl>/             # 平台 workload（跟 SDK 走，版本无关；publish/export ios/android/wasm + native 包）
├── runtimes/<ver>/             # app 运行时（受管，只放 z42vm + libs）
│   ├── z42vm
│   ├── libs/
│   └── link.txt                # 可选：重定向到含 z42vm+libs 的本地目录
└── config.toml                 # default = "<ver>"
```

> **命令 + workload 归属（2026-06-20 裁决）**：SDK 命令与平台 workload **都跟当前 SDK 走**，**不进 `runtimes/<ver>/`**（`runtimes/<ver>/` 只放 app 运行时 z42vm + libs）：
> - **SDK 命令**（`build`/`test`/`fmt`/`new`…）落 `programs/<cmd>/`。其中 **`z42 build`（纯编译，src→zpkg）= `programs/z42c/`**。
> - **平台 workload**（`publish`/`export` 的 ios/android/wasm 命令 + 配套 native 包如 xcframework）落 `workloads/<wl>/`。
> - **暂不做 workload↔runtime 版本绑定**：native 嵌入件 ABI 理论上须配 runtime 版本，但当前为简化跟 SDK 走；多 runtime 版本共存且 ABI 真冲突时再引入版本作用域（见 [runtime-workload-distribution.md](../toolchain/runtime-workload-distribution.md) `workload-future-version-scoped`）。
> - 多版本命令/workload 作用域机制 + 发现细节见 [launcher-command-dispatch.md](../toolchain/launcher-command-dispatch.md)；`publish`（编译+裁剪+workload+打包可分发件）/ `export`（IDE 工程）语义见 [platform-export-lifecycle.md](../toolchain/platform-export-lifecycle.md)。

> **PATH 约定（2026-06-20）**：`$Z42_HOME`（访问 `z42`）和 `$Z42_HOME/bin`（访问 `z42c`、`z42vm`）都加入 PATH。install 脚本同步更新。

`runtimes/<ver>/` 既可以是真实运行时目录，也可以只放一个 `link.txt`（内容为一个本地构建目录的绝对路径）——后者用于 dev：`z42 link <dir> --as <ver>`，无需拷贝。

## 命令（P1）

| 命令 | 行为 |
|------|------|
| `z42 run [--runtime V] <app.zpkg> [-- <args>]` | 解析版本 → 用 `runtimes/<ver>/z42vm` 跑 app，继承 stdio，`--` 后参数透传，设 `Z42_LIBS`，回传退出码 |
| `z42 <app.zpkg> [-- <args>]` | 裸 apphost 形式，等价 `run` |
| `z42 link <dir> --as <ver>` | 把含 `z42vm`(+`libs/`) 的本地目录注册为 `<ver>`（写 `link.txt`） |
| `z42 list` | 列已装运行时（标注 default） |
| `z42 default [<ver>]` | 显示 / 设置默认版本（写 `config.toml`） |
| `z42 which [--runtime V] [app]` | 打印解析到的 `z42vm` 路径 |
| `z42 info` | 打印 `Z42_HOME` / runtimes 目录 / default / 已装数量 |

### 版本解析顺序

```
--runtime <ver>  >  app 自带版本声明(P1 暂空)  >  config.toml default  >  唯一已装  >  报错并列候选
```

## 发布打包：便携模式（portable, model A — bundle-launcher-in-release, 2026-06-03）

`./xtask package` 的 desktop 包内置 launcher,**解压即用、无需安装**:

```
z42-<ver>-<rid>-<profile>/
├── z42               # apphost（包入口，根；payload → programs/launcher/launcher.zpkg）
├── bin/
│   ├── z42c          # apphost → ../programs/z42c/z42c.driver.zpkg
│   ├── z42vm
│   └── apphost       # stub 模板
├── libs/             # stdlib zpkg
├── programs/         # SDK 内置命令 zpkg 实现（2026-06-20）
│   ├── launcher/
│   │   └── launcher.zpkg
│   ├── z42c/
│   │   └── z42c.*.zpkg
│   └── repl/
│       └── z42.repl.zpkg
├── native/  manifest.toml
```

**便携解析**（launcher-at-package-root 2026-06-04；apphost 化 2026-06-20）：`z42`（apphost）在包根，payload 打包时 patch 为 `programs/launcher/launcher.zpkg`（相对路径）；z42vm probe 走统一顺序（完整定义见下「运行时解析」节）——关键是 `{exe_dir}/bin/z42vm`（colocated）优先于项目本地，launcher 总用自己同包的 vm。installed 与便携模式 **probe 路径相同**（exe 同级的 `bin/z42vm`），无需区分两套逻辑。核心的 `run`/`which` 在未 pin `--runtime` 时直接用这个 portable runtime（不查 `runtimes/<ver>`）。于是 `<pkg>/z42 run app.zpkg` 开箱即用，**不重复 z42vm/libs、不用 symlink**（Windows 友好）。`z42` 在根而非 `bin/`：它是包的统一入口，`bin/` 留给工具（z42c/z42vm/apphost 模板）。

打包步骤见 `./xtask package` 的 desktop 路径 [2c];`./xtask test dist` 有 portable `z42 which` smoke。

## 三包发布结构（split-runtime-launcher-packages, 2026-06-13）

`./xtask package` 对 desktop RID 一次构建三个 artifact，发布时上传到同一 GitHub Release tag：

```
z42-sdk-<ver>-<rid>.tar.gz      # SDK 包（便携/bootstrap 专用，含 z42c）
z42-launcher-<ver>-<rid>.tar.gz # launcher 包（z42 self-update 下载）
z42-runtime-<ver>-<rid>.tar.gz  # runtime 包（z42 install <ver> 下载）
z42-workload-<ver>-<wl>.tar.gz  # workload tooling（z42 workload install <wl> 下载）
```

**Launcher 包布局**（直接映射 `$Z42_HOME/`，解压即覆盖；2026-06-20 去除 launcher/ 中间层）：
```
z42               # apphost（根，payload → programs/launcher/launcher.zpkg）
bin/
  z42c            # apphost → ../programs/z42c/z42c.driver.zpkg
  z42vm           # VM 主进程
  apphost         # stub 模板
libs/             # stdlib zpkg
programs/
  launcher/
    launcher.zpkg
  z42c/
    z42c.*.zpkg
  repl/
    z42.repl.zpkg
```

**Runtime 包布局**（直接映射 `$Z42_HOME/runtimes/<ver>/`）：
```
z42vm            # VM 主进程
libs/            # stdlib zpkg
```

**SDK 包**同上便携模式布局（用于 `install-z42.sh` bootstrap 和便携模式），含 z42c apphost + programs/ + native/（examples 不再随包分发——remove-examples-from-packaging, 2026-06-20）。

`release-index.json` 新格式（CI 待更新，旧格式 `runtimes.<rid>.archive` 保留兼容回退）：
```json
{
  "runtimes": {
    "macos-arm64": {
      "launcher": { "archive": "z42-launcher-<ver>-macos-arm64.tar.gz", "sha256": "…" },
      "runtime":  { "archive": "z42-runtime-<ver>-macos-arm64.tar.gz",  "sha256": "…" }
    }
  }
}
```

`_fetchManifest(baseUrl, rid, packageType)` 先读 `runtimes.<rid>.<packageType>.{archive,sha256}`，找不到则回退旧格式 `runtimes.<rid>.{archive,sha256}`。

## 安装模式（installed, model B — install-z42-to-home, 2026-06-03）

`scripts/install-z42.sh --system`（或 `install-z42.bat --system`）从 GitHub Releases 下载 launcher 包并**原样解压**进 `$Z42_HOME`（默认 `~/.z42`）——与 portable 同一套 `cp -R`，自包含布局：
- `z42`（apphost，根，放 PATH）+ `bin/z42c`（apphost → `programs/z42c/z42c.driver.zpkg`）
- `bin/z42vm + programs/launcher/launcher.zpkg + libs/`（z42 apphost 经 `ensure_portable_vm` 用**同址** `bin/z42vm` 跑 launcher 核心）

> **unify-launcher-apphost（2026-06-21）**：SDK launcher **不做多版本**。managed 即"解压即用"——
> 无独立 `$Z42_HOME/launcher/` 运行时、无 `z42 link`/`runtimes/<ver>` 注册（同址 vm 取代了独立
> launcher 运行时）。**更新**＝重跑 `install-z42.sh` / `z42 self-update`（`.bootstrap-stamp` 戳跳过
> 同版本重装，新 tag 重新解压覆盖）。多版本支持按需后置。

装完 `z42` 在 PATH 上（`$Z42_HOME` + `$Z42_HOME/bin`）、`z42 run app.zpkg` 任意目录可用。安装脚本只**打印** PATH 接入指引，不自动改 profile。

> `install-z42.sh` 支持 `--dest <dir>`（指定安装目录）、`--dry-run`（预览不下载）、`--version <ver>`（覆盖版本）、`--verbose`（详细输出）、`--no-path`（抑制 PATH 提示）。联网 `z42 install <ver>` / `z42 self-update`（P2）已实现，见下「P2 命令」。

## 项目本地引导（z42-bootstrap — install-z42, 2026-06-04）

为了用 z42 自己实现的仓库构建工具(`xtask.zpkg` + 迁移后的脚本),仓库需要先有一个可用的 z42 launcher —— 鸡生蛋。`scripts/install-z42.{sh,bat,command}` 是**唯一保留的原生引导脚本**:从 GitHub Releases 下载预编译 launcher 包,装到**项目本地** `<repo>/.z42`(隔离、gitignore、不碰系统 `~/.z42`)。

- **版本**:`versions.toml [toolchain.z42].launcher`,默认 `nightly`(也可 pin `0.1.0`)。
- **按 RID 下载**:`z42-sdk-<ver>-<rid>.{tar.gz|zip}` ← `releases/download/<tag>/`;对 `SHA256SUMS` 校验。
- **版本检查(每次跑)**:nightly 比对 release `published_at`(存于 `.z42/.bootstrap-stamp`),变了才重下;pin 版装一次即跳过。
- **入口**:装完即 `.z42/z42`(launcher-at-package-root 后在根)。
- **`.bat`** 走 PowerShell(下载/解压/Get-FileHash);**`.command`** 是 macOS Finder 双击 → exec `.sh`。

这是 `bootstrap → xtask` 链路的第一环:`install-z42.sh` → `.z42/z42 xtask.zpkg build/test/...`。

## app `runtimeconfig.json`（版本声明 + 动态配置 — add-runtimeconfig-json, 2026-06-03）

app 可在 **`<app>.runtimeconfig.json`** sidecar(.NET 同款,独立于 zpkg,可编辑/动态)声明所需运行时版本 + 运行时旋钮:

```json
{
  "runtime": { "version": "0.3.4", "rollForward": "exact" },
  "configProperties": { "Z42_GC_MODE": "concurrent", "Z42_SAFEPOINT_THROTTLE": 1024 }
}
```

`z42 run <app.zpkg>` 时,launcher 核心(`Std.Json` 解析):
- `runtime.version` 进入版本解析,**优先级**:`--runtime` > runtimeconfig `runtime.version` > `config.toml` default > 唯一已装。
- `configProperties` 的**任意** key/value 作为 env 设到被 spawn 的 z42vm(`Std.IO.Process.Env`)——支持任何 `Z42_*` 旋钮(GC 模式 / safepoint throttle / …),动态生效。
- `rollForward`:P1 只认 `exact`。

> 独立 sidecar 的好处:版本无关的 launcher 读它**不需解析带版本的 zpkg 格式**;可手改、可被工具生成。这也是 P2(下载即用)的前置——"声明需要的版本 → 没装自动拉"(自动拉 = P2)。

## apphost：每-app 原生可执行文件（add-apphost, 2026-06-09）

类比 .NET apphost：`z42c build` 产出的 Exe-zpkg 不必再靠 `z42 app.zpkg` 跑，而是**生成一个以 app 命名、直接就是 app 的原生可执行文件** `./myapp`——双击/直接调用，自己定位 VM + stdlib 运行时并起 app。

### 用法

```bash
z42c build app.z42.toml            # → app.zpkg（如常）
z42 publish app.z42.toml           # release apphost → publish_dir（留存的发布件）
z42 run desktop app.z42.toml       # debug apphost（旁 zpkg）+ 直接 exec（预演部署启动）
./app arg1 arg2                    # publish 出的 apphost 直接运行；参数 + 退出码透传
```

> **publish vs run desktop**：`z42 publish` 产 release apphost 到 `[platform.desktop].publish_dir`（留存件）；`run desktop` 产 debug apphost 于 zpkg 同目录并直接 exec（ephemeral，预演 apphost 自解析启动路径，区别于 `z42 run` 走 launcher 路径）。详见 [platform-export-lifecycle.md](../toolchain/platform-export-lifecycle.md) 命令模型。

### 机制：内嵌占位符 patch（.NET 同款）

native apphost stub（`src/toolchain/workload/desktop/platform/apphost`，z42-apphost crate，运行时解析共享 `z42-hostrun`；**统一后 `z42` 自身也是此 stub —— 原 trampoline 实现已删，unify-launcher-apphost 2026-06-21**）内嵌一段固定占位区：32 字节 MAGIC sentinel + 992 字节 payload（共 1024）。patch 逻辑（z42 写，`src/toolchain/workload/desktop/appbuilder/apphost.z42`）：

1. 在 stub 模板文件里按 MAGIC 定位占位区；
2. 把 payload 覆写为 app 的 **zpkg 路径** + NUL（路径形态见下三种模式）；
3. 写出到目标位置、置可执行位；
4. **macOS：ad-hoc 重签名**（见下）。

**输入：项目 toml**（apphost-as-config；占位符总是相对路径，运行时按 **exe 自身目录**（`current_exe`）解析 ⇒ 整个 `<exe + app.zpkg>` 可整体搬迁）：

`z42 publish <project.z42.toml> --rid <desktop-rid>` 需 `[platform.desktop] apphost = true`（**gate**；缺省 / false → 报 "not configured to publish a desktop apphost" 退出），读 `publish_dir`（**仅输出位置**，缺省 = 项目目录）+ 从 `[build]`/`[project]` 推出已编译 zpkg，把 exe 产到 `publish_dir/<name>`（exe 名 = `[project].name`；`--output` 覆盖 publish_dir）。内嵌的 zpkg 路径**相对 exe 所在目录**。典型：`z42 publish scripts/xtask.z42.toml` 在仓库根产出 `./xtask`，内嵌 `artifacts/xtask/xtask.zpkg` → `./xtask build package` 直跑 xtask，免敲 `z42 artifacts/xtask/xtask.zpkg -- …`。**无 wrapper 脚本**——产 `./xtask` 的步骤就是「编 zpkg → `z42 publish <toml>`」两条命令（见 `docs/workflow/building/` 与下「z42.toml 配置」）。`./xtask` 原生 + 平台相关 + 已 gitignore，**重生不提交**。`z42c` **不消费** `[platform.desktop]`（patcher 才读它；C# 只把它登记为已知 section 以免 WS008）。

运行时 stub 读占位符（未 patch 则报错退出），定位 `z42vm` + `libs`，**直接 `exec z42vm <app.zpkg> -- <argv>`（设 `Z42_LIBS`）**。

### apphost 的两个生产者 + xtask 内联 patcher（unify-launcher-apphost, 2026-06-21）

patch 同一段占位符的逻辑有**两个调用方**：

1. **用户 app**：`z42 publish <project.toml>` —— desktop **workload** 包的 `apphost.z42`（`Z42.Workload.Desktop.Apphost.PatchBytes`）。
2. **SDK 自己的 apphost**（现在的 `z42`，将来的 `bin/z42c`）：在 **`xtask package`** 里产出 —— xtask **内联**了一份精简 patcher（`scripts/package/xtask_package_desktop.z42` 的 `_produceApphost`，复刻 `PatchBytes`）。

**为什么 xtask 内联、不复用 workload 的 patcher**：xtask 的依赖只有 `z42.core/io/...`，**不依赖 workload 包**——而 workload 包**正是同一次 `xtask package` 在产出的东西**（`_buildDesktopWorkload`）。让 xtask 依赖 workload 会构成循环（用还没造好的 workload 去造 SDK），用上一次发布的旧 workload 又会版本/ABI 错位。发布走 from-source（CI `xtask-bootstrap-source`），那条链里根本没有"下载的 workload"可用。故内联打破循环。

> **xtask 自身的「编译 + 运行」与「xtask 产出的 SDK」是两套东西，后者不参与前者**：xtask 由一个**独立工具链**（CI `xtask-bootstrap` 下载的 nightly z42，或 `xtask-bootstrap-source` 从源码现 build 的 z42c/z42vm）编译 + 运行；`./xtask` apphost 也是用该工具链的 stub 直接 patch 得到。`xtask package` 输出的 SDK（含其中的 `z42` apphost）纯属 artifact，**不回头跑 xtask**。所以内联 patcher 只服务"产出端"，与 xtask 怎么编/跑无关。

**这样设计更加灵活，也避免以后循环依赖带来更多问题**（xtask 自包含、不被 workload 反向拖住）。代价是 MAGIC 字符串现有三处副本（Rust stub 嵌入端＝权威 / workload `apphost.z42` / xtask 内联），改 MAGIC 须三处同步；后续若嫌重复，可把 `PatchBytes` 抽成**编译期共享库**（z42.io 或新建 z42.apphost）让 workload 与 xtask 共用一份——但仍是共享库、不是"下载 workload"。

> **直跑模型（simplify-apphost-direct-run, 2026-06-10）**：apphost **不经** `launcher.zpkg` / muxer，单个 VM 进程直接跑 app —— 与 .NET apphost 一致（published apphost 不走 `dotnet` muxer）。stub 只做"找 VM + 跑 app"（允许的最小原生核，符合"z42 优先"：它不实现任何 z42 逻辑，只是少做）。**部署一个 app 只需：apphost exe + app.zpkg + 可解析的运行时（z42vm+libs），不需要 launcher.zpkg。** 代价：apphost **不读 `<app>.runtimeconfig.json`**（版本 pin + `configProperties` 旋钮）—— 那套逻辑在 `launcher.zpkg` 里，只有走 `z42 run` 才生效；需要版本选择/GC 旋钮的 app 用 `z42 run`，或后续给 stub 加最小版本检查（Deferred）。`launcher.zpkg` 仍在 SDK 里供 `z42` muxer（run/list/install/publish）用，只是 apphost 不路由经它。

### 运行时解析：z42vm 探测（统一 apphost 唯一真相，2026-06-21）

统一 trampoline → apphost 后，**所有原生入口共用同一个 stub、同一套 `z42-hostrun`**：`z42`（launcher，包根）、`bin/` 内工具 apphost（`z42c` 自举后及后续）、per-app apphost（`./xtask` 等）。stub 不捆绑 VM/libs（framework-dependent），定位 **`z42vm`**（设 `Z42_LIBS`；**不需 `launcher.zpkg`**）。

**SDK 同址引导（启动第一步，`ensure_portable_vm`）**：apphost 启动时,若 `$Z42_PORTABLE_VM` **未设**且 exe 同址有 z42vm —— `{exe_dir}/z42vm`（apphost 在 `bin/` 内、vm 同级，如 `bin/z42c`）或 `{exe_dir}/bin/z42vm`（apphost 在包根，如 `z42`）—— 就把 `$Z42_PORTABLE_VM` 设成它。

> **为什么经 `$Z42_PORTABLE_VM` 而非单独探测档**：① 让 SDK 包内的 app 用**自己那个包**的 ABI-匹配 vm（payload 与同包 `bin/z42vm` 同版本，zbc 必配）；② 设为进程环境变量后**自动传给它 spawn 的子进程** —— launcher `z42` 跑 `launcher.zpkg`、后者再调 `z42c`/其它 SDK app，子进程继承同一个 vm，全链一致。**exe 同址查找只此一处**（不在 `resolve_app_runtime` 做探测档），范围收窄到 SDK-internal app。直接跑 `bin/z42c`（不经 launcher、无继承）也靠这一步命中同址 vm。

**`resolve_app_runtime` 探测顺序**（`src/runtime/crates/z42-hostrun`）:

**most-local-wins 顺序**（2026-06-21）：
```
① $Z42_PORTABLE_VM   显式指向 z42vm（文件或其所在目录）；同时是 SDK 同址 vm 的载体（上面 ensure 设的）。
                      libs 推导：vm 在 bin/ 内 → <bin父>/libs，否则 → <vm父>/libs。
② 本地（最具体）      exe 逐级上行 <d>/.z42/launcher、<d>/.z42 —— apphost 同级/项目 venv（xtask 这类 framework-dependent）。
③ 用户目录            $HOME/.z42/launcher
④ 系统目录            $Z42_HOME/launcher（最低 —— 显式覆写请走 ①）
   都无 → None（apphost 报错列已查路径，非零退出）。
```

> **为什么 $Z42_HOME 垫底而非置顶**：显式覆写已由 `$Z42_PORTABLE_VM`（①）承担，`$Z42_HOME` 退化成单纯"系统安装根位置"。优先级遵循 most-local-wins：本地项目 `.z42`（②）> 用户 `$HOME/.z42`（③）> 系统 `$Z42_HOME`（④），与"同级目录优先"一致。

**角色分流**（全靠 ① 是否被 `ensure_portable_vm` 填上）：
- **launcher `z42`**（包根，子 `bin/z42vm`）→ ensure 命中 `{exe}/bin/z42vm` → ① ✓，跑 `programs/launcher/launcher.zpkg`。
- **`bin/z42c` 及后续 bin/ apphost**（与 z42vm 同在 `bin/`）→ ensure 命中 `{exe}/z42vm` → ① ✓，**直接 `exec z42vm <自身zpkg>`、不经 launcher**。payload 指向自身 zpkg（如 `bin/z42c` → `../programs/z42c/z42c.driver.zpkg`），其包依赖经**同址依赖搜索**（payload 目录入 `search_dirs`，见 vm-architecture.md）从 `programs/z42c/` 解析、stdlib 从 `libs/`。
- **per-app `./xtask`（framework-dependent）**（exe 近邻无同址 vm）→ ensure 不触发 → 落 ② 项目 `.z42`，venv 语义不变。

**两个旋钮**：`$Z42_PORTABLE_VM`（①）显式钉死任意 vm（CI/镜像/特殊部署）或承载 SDK 同址 vm，最高优先；`$Z42_HOME`（④）改"系统安装根"位置（最低优先级）。

### macOS 代码签名（必须）

macOS（尤其 Apple Silicon）强制代码签名：**patch 二进制字节会使 stub 签名失效，内核拒绝运行**（表现为 hang）。故 patcher 在 macOS patch 后 ad-hoc 重签名 `codesign -s - -f <out>`（.NET apphost 同款）。先 patch payload、再签名（codesign 只改签名 blob，不动 `__DATA` 的占位符）。Linux 无签名；Windows 跑无签名 exe 无碍（PE checksum/Authenticode = Deferred）。

### 分发：模板随包/安装

native apphost 模板按 host 编译，随 desktop 包分发：
- `./xtask package` → `<pkg>/bin/apphost`（patcher 便携模式从 `dirname(Z42_PORTABLE_VM)/apphost` 取）。
- `install-z42.sh --system` → `$Z42_HOME/bin/apphost`（installed 模式从此处取）。
- `./xtask test dist` 有 apphost smoke（build → `z42 publish` → 跑产出 exe → 断言）。

### z42.toml 配置：`[platform.desktop]` publish（apphost-as-config, 2026-06-17）

项目用 `[platform.desktop]` 段声明桌面平台输出；`z42 publish <toml>` 读它产出原生 apphost exe（对称 ios/android/wasm export；**无独立 `z42 apphost` 命令**）：

```toml
[platform.desktop]
publish_dir = ".."   # exe 落在哪（project-dir 相对，同 output_dir）
                     # xtask.z42.toml 在 scripts/ → ".." = 仓库根 → ./xtask
```

- **`publish_dir`**：apphost exe 的输出目录，**相对 toml 所在目录**（与 `[build].output_dir` 同基准）。exe 名 = `[project].name`。
- 已编译 zpkg 的位置由 patcher 从 `[build]`（`output_dir` / `dist_dir`，支持单层 `${output_dir}` 展开；缺省回退 `<output_dir>/dist`）+ `[project].name` 推出——与 `z42c build` 的产物布局一致。
- **`z42c` 不消费 `[platform.desktop]`**：消费方是 `z42 publish <toml>` patcher（逻辑全留 z42，符合"z42 优先"）。C# `ProjectManifest` 仅把 `platform.desktop` + `publish_dir` 登记为已知 schema，避免 WS008 unknown-key（仍对 `[platform.desktop]` 内的陌生 key 报 WS008）。

**两条命令产 `./xtask`**（无 wrapper 脚本）：

```bash
# 1. 编 xtask.zpkg（冷树先 build stdlib，见 building/stdlib.md）
dotnet run --project src/compiler/z42.Driver -- build scripts/xtask.z42.toml --release
# 2. 从 toml 产 ./xtask（读 [platform.desktop].publish_dir）
z42 publish scripts/xtask.z42.toml
```

> **历史**：`[project].apphost = true` 布尔（Deferred `apphost-future-build-flag`）→ `[apphost]` 段 + 独立 `z42 apphost build <toml>` 命令（2026-06-10）→ **apphost-as-config（2026-06-17）** 统一为 `[platform.desktop]` 段 + `z42 publish desktop`，取消独立 `z42 apphost` 命令（apphost 与 ios `.ipa`/android `.aab`/wasm bundle 同层，是 desktop 平台发布产物）。消费逻辑始终全留 z42 patcher，compiler 仅登记 schema。

## Deferred / Future Work

### ~~launcher-future-install~~ ✅ 已实现（add-launcher-install, 2026-06-13）

`z42 install <ver|nightly>` 和 `z42 self-update [--channel <ver>]` 已在 `add-launcher-install` 中实现：manifest-first（`release-index.json`）下载、SHA256 验证、流式 tgz/zip 解压、staged 原子替换。见 `src/toolchain/launcher/core/launcher_network.z42`。`split-runtime-launcher-packages`（2026-06-13）进一步细化：各命令通过 `packageType` 参数请求各自的专属小包而非全量 SDK。

P2 命令：

| 命令 | 行为 |
|------|------|
| `z42 install <version\|nightly>` | 请求 `packageType="runtime"` → 下载 `z42-runtime-<ver>-<rid>.tar.gz` 到 `$Z42_HOME/runtimes/<ver>/` |
| `z42 self-update [--channel <ver>]` | 请求 `packageType="launcher"` → 下载 `z42-launcher-<ver>-<rid>.tar.gz` 替换 `$Z42_HOME/{z42, bin/, programs/launcher/, libs/}`（portable 模式拒绝）|

### launcher-future-self-update-windows: Windows 上 `z42 self-update` 替换失败

- **来源**：add-launcher-install 实施期延后
- **触发原因**：Windows 上 `z42.exe` 是父进程（等待 `z42vm`），替换 `$Z42_HOME/programs/launcher/` + `bin/z42vm` 会因文件被占用而失败
- **前置依赖**：进程退出后延迟替换策略（rename-then-copy）或 PowerShell 辅助
- **触发条件**：Windows 用户需要 `z42 self-update` 时
- **当前 workaround**：Windows 用户重新运行 `install-z42.bat --system` 替换 launcher

### launcher-future-version-declaration: app 自带运行时版本声明

- **来源**：add-z42-launcher design（决策 D4）
- **触发原因**：是否把"需要哪个运行时版本"写进 zpkg `META.toolchain_version` 还是独立 `runtimeconfig.json` sidecar 未定；strict-pin 下 P1 用 `link`+`default` 本地指定即可
- **触发条件**：进入分发场景（P2 下载）时必须定
- **当前 workaround**：版本解析第 2 步（读 app 自带声明）留空 hook

### launcher-future-single-file-exe-zpkg: z42c 从裸 `.z42` 脚本直接产 Exe-zpkg

- **来源**：add-z42-launcher（原 phase 0.5）
- **触发原因**：launcher 核心与 dev 脚本都可作为**普通 `kind="exe"` 项目**（带 `z42.toml`）经现有 `z42c build` 产 Exe-zpkg；单独实现"裸脚本 → Exe-zpkg"需在 SingleFileCompiler 重新装配 zpkg（sourceHash/namespace/deps），与已测项目路径重复，ROI 低
- **触发条件**：若大量一次性脚本需免 `z42.toml` 的极简体验再做
- **当前 workaround**：脚本写成 5 行 `z42.toml`（`kind="exe"`）的 mini-project，`z42c build` 即得 Exe-zpkg
