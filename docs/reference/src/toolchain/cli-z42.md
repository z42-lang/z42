# `z42` 命令面

> 对齐：2026-09-17（change `restructure-docs-three-books`）｜ 代码：
> `src/toolchain/launcher/core/launcher_cli.z42`（命令树与路由）、
> `src/toolchain/launcher/core/launcher.z42`（`run` / `version` / SDK 布局）、
> `src/toolchain/launcher/core/launcher_export.z42`（`publish` / `export`）、
> `src/toolchain/launcher/core/launcher_workload.z42`（`workload`）、
> `src/libraries/z42.project/src/ManifestLocator.z42`（工程定位）
>
> 清单字段（`[project]` / `[[exe]]` / `[platform.*]` / `[profile.*]`…）见
> [工程清单 z42.toml](z42-toml.md)；`z42c` / `z42b` 的直接命令面见
> [z42c 与 z42b](cli-z42c-z42b.md)；运行时旋钮见[运行时设置](runtime-settings.md)。

`z42` 是 SDK 的唯一命令入口。本页给出全部子命令、旗标、退出码，以及命令共用的四条约定
（工程定位、产物位置、参数透传、SDK 根定位）。查「某个旗标叫什么、还在不在」时看这里。

## 子命令全表

| 命令 | 作用 | 谁执行 |
|---|---|---|
| `z42 new` | 创建工程 | 转发 z42b |
| `z42 build` | 编译工程 | 转发 z42c |
| `z42 run` | 构建并运行 | launcher |
| `z42 test` | 运行 `[Test]` | 转发 z42b |
| `z42 bench` | 运行 `[Benchmark]` | 转发 z42b |
| `z42 clean` | 删除构建产物 | 转发 z42b |
| `z42 repl` | 交互式 REPL | 转发 z42i |
| `z42 publish` | 产出可部署应用 | launcher → z42b |
| `z42 export` | 生成原生 IDE 工程 | launcher |
| `z42 workload` | 安装 / 列出 / 卸载平台 workload | launcher |
| `z42 version` | 打印版本 | launcher |
| `z42 help` | 显示帮助 | launcher |

命令表就这 12 个。没有 `install` / `uninstall` / `self-update` / `info` / `list` /
`default` / `link` / `which`——SDK 是单版本的，launcher 不管理多个运行时版本，
安装与更新由安装脚本负责。

## 通用约定

### 帮助

每个命令都接受 `-h` / `--help`；`z42 help <命令>` 与 `z42 <命令> --help` 等价
（`z42 help` 无参数时列出命令表）。

### 工程定位

命令不带工程路径时，从当前目录逐级向上找，每一层**按顺序**判定：

1. `z42.toml` → 工程清单；
2. 恰好一个 `*.z42.toml` → 工程清单；**多于一个**报歧义并列出候选，由用户指定；
3. `z42.workspace.toml` → 工作区清单。

最近者优先；到文件系统根仍无：

```console
$ z42 build
error: could not find z42.toml in the current directory or any parent directory
  (create a project with: z42 new <name>)
```

**显式给出目录时只看该目录、不向上。** 起点是相对路径时，报出的清单路径也相对表达
（`./z42.toml`、`../z42.toml`），于是编译诊断里是 `./src/Main.z42` 而不是一长串绝对路径。

规则只有一处实现（`ManifestLocator`），launcher / z42b / z42c 共用。

### 产物位置

按清单 `[build]` 段解析，默认 `dist/`（最终产物）与 `artifacts/<profile>/`（缓存、
生成代码）。z42c 写、`run` 找、`clean` 删是同一套规则。字段与级联默认见
[工程清单 z42.toml](z42-toml.md) 的 `[build]` 一节。

```console
$ z42 build
cached: 0/1 files
wrote -> ./dist/probe.zpkg (indexed, 1 zbc)
cache -> ./artifacts/debug/.cache (1 files)
```

### 程序参数怎么透传

`z42 run` 的程序参数写在 `--` 之后。`--` 之前出现未识别的选项**直接报错**，不会被猜成
路径：

```console
$ z42 run --bogus
z42 run: unknown option `--bogus`
  (program arguments go after `--`; see `z42 run --help`)
```

程序读到的数组**既不含程序名，也不含 `--`**：

```console
$ z42 run app.z42 -- a b --flag
```

```z42
Environment.GetCommandLineArgs()   // == ["a", "b", "--flag"]，Length 3
```

细节见[控制台与文件](../stdlib/io-file.md)的 `GetCommandLineArgs()` 一节。

### SDK 根目录

launcher 按这个顺序定位 SDK 根：`Z42_HOME` 环境变量 → apphost 注入的
`Z42_PORTABLE_VM`（`<sdk>/bin/z42vm`）反推两层 → `~/.z42`（Windows 用 `%USERPROFILE%`）。
根目录下的布局：

| 路径 | 内容 |
|---|---|
| `z42` | launcher 自身的 apphost |
| `bin/` | `z42vm` / `z42c` / `z42b` / `z42d` / `z42i` |
| `programs/<tool>/` | 各工具的 zpkg |
| `libs/` | 标准库 zpkg |
| `manifest.toml` | `[package]` 的 `version` / `rid` / `build-date` |
| `cache/` | 单文件运行的合成工程缓存（`Z42_CACHE_DIR` 可覆盖） |
| `runtimes/<ver>/workloads/<wl>/` | 已安装 workload 的工具部分 |
| `runtimes/<rid>/<ver>/` | 已安装 workload 的运行时包 |

## 退出码

| 码 | 含义 | 发射点 |
|---|---|---|
| `0` | 成功 | — |
| `1` | 构建 / 执行失败（诊断已输出） | `src/compiler/z42c.driver/src/Main.z42:27`（`ExitCode.BuildError`）、`src/toolchain/launcher/core/launcher.z42:176`、`:184` |
| `2` | 用法错误：未知命令 / 未知选项 / 找不到工程 / 找不到工具 | `src/toolchain/launcher/core/launcher_cli.z42:35`、`:42`；`launcher.z42:154`、`:160`、`:170`、`:182`、`:193`；`z42c.driver/src/Main.z42:28`（`ExitCode.UsageError`） |

**被运行程序的退出码原样透传**——launcher 把子进程的退出码直接交回
（`src/toolchain/launcher/core/launcher.z42:214`）：

```console
$ z42 run exit7.z42          # 程序里 Environment.Exit(7)
$ echo $?
7
```

转发命令（`new` / `build` / `test` / `bench` / `clean` / `repl` / `publish`）同样原样
透传被转发工具的退出码。

---

## `z42 new [--lib] [--path <dir>] [<name>]`

在 `<dir>/<name>`（`--path` 默认当前目录）创建工程。

| 旗标 | 作用 |
|---|---|
| `--lib` | 库工程（默认可执行工程） |
| `--path <path>` | 创建到哪个目录下（默认当前目录） |

`<name>` 须匹配小写字母 / 数字 / `-` / `_` / `.`，且首字符是字母或数字；不合规直接报错退出 2：

```console
$ z42 new Probe-K
z42 new: invalid project name `Probe-K`
  use lowercase letters, digits, `-`, `_` or `.`, starting with a letter or digit (e.g. `my-app`)
```

生成四个文件——`z42.toml`、`src/Main.z42`（`--lib` 时为 `src/Lib.z42`）、`.gitignore`、
`README.md`：

```console
$ z42 new hello
Created executable project `hello` in hello/

  cd hello
  z42 run
```

```toml
# 生成的 z42.toml
[project]
name = "hello"
version = "0.1.0"
kind = "exe"

[sources]
include = ["src/**/*.z42"]
```

**测试不单设模板**：任何工程的 `tests/*.z42` 都会被 `z42 test` 自动发现。

## `z42 build [<manifest>] [options]`

编译工程（增量：只重编改动过的文件），产物写入 dist 目录。定位到的是工作区清单时等价
`--workspace`。这条命令原样转发给 z42c，帮助里的用法行也写作 `z42c build`。

| 旗标 | 作用 |
|---|---|
| `--release` | release profile（默认 debug） |
| `--workspace` | 按拓扑序构建工作区全部成员 |
| `--output-dir <dir>` | 所有产物写到 `<dir>` |
| `--no-incremental` | 全量重建（忽略构建缓存） |
| `--opt <name>` | 开启一项优化（可重复） |
| `--no-opt <name>` | 关闭一项优化（可重复） |
| `--jobs <n>` | 包内文件级并行编译度（默认 1 = 串行） |
| `--fix` | 把 analyzer 携带的代码修复**就地写回**源文件 |
| `-q` / `--quiet` | 不输出进度（诊断照常打印） |

优化名与 profile 默认见 [z42c 与 z42b](cli-z42c-z42b.md)。未识别的选项报错退出 2。

## `z42 run [options] [<目标>] [-- <程序参数>]`

目标可以是单个 `.z42` 源文件、工程目录、清单文件，或已编译的 `.zpkg`。
不给目标时按[工程定位](#工程定位)向上找，先构建（`--quiet`：只留诊断，进度行不混进程序
输出）再运行。

`.zbc` 也在接受之列，但裸 `.zbc` 没有烤入口，而 `z42 run` 没有传入口函数名的旗标，
于是会停在 `Error: no entry point`——要跑的产物用 `z42 build` 产的 `.zpkg`。

简写 `z42 <app.zpkg|app.zbc|file.z42> [-- args]` 等价于 `z42 run <目标>`。

| 旗标 | 作用 |
|---|---|
| `--bin <name>` | 工程声明了多个 `[[exe]]` 时选一个（**必须选**，否则报错并列出目标） |
| `--mode <interp\|jit>` | 本次运行的执行模式 |
| `--config <file>` | 用户运行时配置文件（TOML，`[runtime]` 表） |
| `--set <key>=<value>` | 设一个运行时旋钮，可重复 |

`--mode` / `--config` / `--set` 直接交给 z42vm；旋钮名与取值见[运行时设置](runtime-settings.md)。
`--mode` 的取值只有 `interp` 与 `jit`。`z42 run --help` / `z42 repl --help` 的用法行里
还写着第三个 `aot`，但 z42vm 不接受它：

```console
$ z42 run --mode aot
error: invalid value 'aot' for '--mode <MODE>'
  [possible values: interp, jit]
```

launcher 只透传，旋钮名校验归 z42vm：

```console
$ z42 run --set gc-mdoe=1
z42: unknown runtime knob `gc-mdoe` in --set; did you mean `gc-mode`?
     Run `z42vm --list-knobs` (or `--list-knobs --all`) to see every knob.
```

定位到工作区清单时报错，需进成员目录运行。

### 单文件运行

目标是单个 `.z42` 源文件时，在缓存目录（`Z42_CACHE_DIR`，缺省 `<SDK 根>/cache`）的
`run/<源文件绝对路径的 SHA-256>/` 下合成一份最小清单（`kind = "exe"`，`include` = 源文件
**绝对路径**），此后完全复用工程构建路径——增量缓存、入口自动检测、runtimeconfig 侧车
一致。

三条边界：

- **源文件目录不产生任何产物**，源文件也不会被复制进缓存；
- 单文件只能使用标准库（合成清单不含 `[dependencies]`），需要依赖时用 `z42 new` 建工程；
- `--bin` 与单文件同用报错（单文件只有一个入口）。

诊断位置按当前工作目录相对化呈现——在当前目录之下的绝对路径显示为相对路径，之外的保持
绝对（不生成 `../../..`）。

## `z42 test [options] [<目标>]`

构建工程并运行 `[Test]`。工程声明了测试目标（约定发现的 `tests/*.z42`，或 `[tests]` /
`[[test]]`）时逐个构建运行，否则运行工程自身的 `[Test]`。目标也可以是已编译的
`.zpkg` / `.zbc`，或 device 部署用的 `bundle.json`。

| 旗标 | 作用 |
|---|---|
| `--list` | 只列出 test 目标名（一行一个），不编译不运行 |
| `--name <n>` | 只构建运行**名字精确等于** `<n>` 的目标；点不中报错 |
| `--filter <s>` | 只保留**目标名**含子串 `<s>` 的目标；一个都不匹配时打印提示并退出 0 |
| `--format <pretty\|json>` | 输出格式（默认 `pretty`） |
| `--release` | release profile（默认 debug） |
| `--reuse-parent` | 父包 dist 已存在则复用、不重建 |
| `--rid <rid>` | `host`（默认，在本进程内跑）或 `device`（组装可部署件） |
| `--out <dir>` | device：组装出的 `{app,libs,bundle}` 输出目录 |
| `--stage-only` | device：只组装可部署件，不构建也不运行 |
| `--build` | device：跑原生平台构建（wasm-pack / xcframework / cargo-ndk） |
| `--run` | device：部署 + 在设备 / 模拟器上运行并收报告 |
| `--build-root <dir>` | device `--build` / `--run`：定位原生平台 crate 的仓库根 |
| `--node-bin <dir>` | device `--run`：前置到 PATH 的 node bin 目录 |

`--name` 与 `--filter` 是**两种语义**，都作用在**目标名**上：`--name` 是「就这一个，
点不中退出 2」，`--filter` 是「筛一批，筛空了只是没事可做」。两者都给时取交集。
`--filter` **不筛单个测试方法名**——用它匹配方法名会得到「一个目标都没匹配上」。

失败（有 `FAIL` 的用例，或编译不过）退出 1；工程完全没有测试目标时打印「无事可做」并
退出 0。

```console
$ z42 test
── test target: Basic ──
  PASS ProbeKProj.Tests.BasicTests.Passes$0
  FAIL ProbeKProj.Tests.BasicTests.Fails$0: Std.TestFailure: values not equal

  Result: 1 passed, 1 failed, 0 skipped
```

`--format json` 输出单行 JSON：

```json
{"tool":"z42b","module":"…/probe.test.Basic.zpkg","summary":{"total":2,"passed":1,"failed":1,"skipped":0},"results":[{"name":"…Passes$0","status":"passed","is_benchmark":false},{"name":"…Fails$0","status":"failed","is_benchmark":false,"reason":"Std.TestFailure: values not equal"}]}
```

## `z42 bench [options] [<目标>]`

同 `test`，运行 `[Benchmark]`。旗标是 `test` 的子集：`--list` / `--name` / `--filter` /
`--format`（`json` 时额外带 `bench_stats`）/ `--release` / `--reuse-parent` / `--rid`。
没有 device 那组旗标。

## `z42 clean [<manifest|dir>]`

删除工程的构建产物（debug 与 release 两个 profile）：dist 目录，以及 `artifacts/`——
`[build] output_dir` 未配置时整个删除；配置了时只删解析出的 cache / generated 目录，
不删可能与其它工程共享的 `output_dir` 本身。

```console
$ z42 clean
removed ./dist
removed ./artifacts
```

## `z42 repl [options]`

启动交互式 REPL。

| 旗标 | 作用 |
|---|---|
| `--mode <interp\|jit>` | 执行模式 |
| `--config <file>` | 用户运行时配置文件 |
| `--set <key>=<value>` | 本次会话的运行时旋钮，可重复 |
| `-c <expr>` | 求值 `<expr>`、打印结果、退出 |

会话内的元指令以 `.` 开头：

| 元指令 | 作用 |
|---|---|
| `.help` | 列出元指令 |
| `.vars` | 显示已绑定变量 |
| `.types` | 显示已声明类型 |
| `.usings` | 显示生效的 using |
| `.using <ns>` | 增加一条 using |
| `.version` | 显示版本 |
| `.clear` | 清屏 |
| `.reset` | 重置会话状态 |
| `.exit` / `.quit` | 退出（或 Ctrl-D） |

`z42 repl -c "-h"` 求值字符串 `-h`，而不是显示帮助——`-c` 的值不参与帮助探测。

## `z42 publish [options] [<manifest>]`

产出可部署应用。`--rid` 默认宿主 RID。

| 旗标 | 作用 |
|---|---|
| `--rid <rid>` | 目标 RID（默认宿主） |
| `--output <dir>` | 输出目录（覆盖 `[platform.*].publish_dir`） |
| `--no-build` | 直接部署已构建的 zpkg（默认先编译，只重建改动过的） |
| `--self-contained` | 嵌入 z42 运行时（自包含 app，不依赖外部 z42vm）；静态 / 动态链接由 `[platform.desktop] link` 决定 |

**当前只支持桌面 RID**（`macos-*` / `linux-*` / `windows-*`）；其它平台类别报错退出 2。
工程须声明 `[platform.desktop] apphost = true`，apphost 本身随 desktop workload 安装：

```console
$ z42 publish
z42b publish: ./z42.toml is not configured to publish a desktop apphost — add `apphost = true` under [platform.desktop]

$ z42 publish            # 声明了 apphost，但没装 workload
z42b publish: desktop apphost not available (the apphost ships with the desktop workload).
       run: z42 workload install desktop
```

`--self-contained` 的输出是一个 app 目录：可执行文件 + `app.zpkg` + `libs/`（侧车随之
改名为 `app.runtimeconfig.toml`）。默认输出根：`--output` → `[platform.desktop].publish_dir`
→ `<output_dir>/publish` → `<工程目录>/publish`。

## `z42 export --rid <rid> [options] [<manifest>]`

为 ios / android / wasm 生成原生 IDE 工程。`--rid` **必给**。

| 旗标 | 作用 |
|---|---|
| `--rid <rid>` | `ios-*` / `iossim-*` / `android-*` / `browser-wasm` |
| `--output <dir>` | 输出目录（默认清单所在目录下的 `<工程名>-ios` / `-android` / `-wasm`） |
| `--sdk-ver <ver>` | 使用哪个平台 SDK 版本 |
| `--bundle-id <id>` | iOS `CFBundleIdentifier`（仅 ios） |
| `--app-id <id>` | Android application ID（仅 android） |
| `--entry <Type.Method>` | wasm 入口点，如 `Hello.Main`（仅 wasm） |

CLI 旗标覆盖清单里 `[platform.ios]` / `[platform.android]` / `[platform.wasm]` 的同名字段；
必填项两处都没有就报错：

```console
$ z42 export --rid ios-arm64
z42 export ios: bundle_id required — add [platform.ios] bundle_id in ./z42.toml or pass --bundle-id
```

桌面 RID 没有 IDE 工程，会被指回 `z42 publish`。平台工具与运行时包需先
`z42 workload install <平台>`。

## `z42 workload install|list|uninstall`

安装 / 列出 / 卸载平台 workload。workload 名：`ios` / `android` / `wasm` / `desktop` /
`test`。

`z42 workload install <workload>`：

| 旗标 | 作用 |
|---|---|
| `--from <dir>` | 本地工具包目录（省略则走网络安装） |
| `--runtime <dir>` | 本地运行时包目录 |
| `--base-url <url>` | 网络安装的 manifest 根（默认 GitHub releases） |
| `--version <ver>` | 版本（默认取自包 manifest；网络安装**必给**） |
| `--rid <rid>` | 运行时 RID（默认取自运行时目录名） |

`z42 workload uninstall <workload> [--version <ver>]`——`--version` 省略时卸载全部版本。

`z42 workload list` 每行一个 `<wl>  (<ver>)`；一个都没装时：

```console
$ z42 workload list
(no workloads installed)
```

安装落位：工具部分进 `<SDK 根>/runtimes/<ver>/workloads/<wl>/`，平台运行时包进
`<SDK 根>/runtimes/<rid>/<ver>/`（两者分开，同一份工具可配多个 RID 的运行时）。

## `z42 version`

亦可写作 `z42 --version` / `z42 -V`。打印 `z42 <version> (<rid>, <build-date>)`：

```console
$ z42 version
z42 0.6.0 (macos-arm64, 2026-09-17)
```

数据来自 SDK 根目录的 `manifest.toml`；从源码树直接组装、没有该文件的 SDK 打印
`z42 (dev build)`。

## `z42 help [<命令>]`

无参数时列出命令表；带命令名时等价 `z42 <命令> --help`。
