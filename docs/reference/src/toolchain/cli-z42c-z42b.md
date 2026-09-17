# `z42c` / `z42b` / `z42d`

> 对齐：2026-09-17（change `restructure-docs-three-books`）｜ 代码：
> `src/compiler/z42c.driver/src/Main.z42`（z42c 入口与退出码契约）、
> `src/compiler/z42c.driver/src/BuildCommand.z42`（`build` 的选项解析）、
> `src/compiler/z42c.semantics/src/OptSet.z42`（优化名表）、
> `src/toolchain/builder/core/builder_cli.z42`（z42b 命令树）、
> `src/toolchain/devtools/core/devtools_cli.z42`（z42d 命令树）
>
> 日常用法走 [`z42` 命令面](cli-z42.md)；这几个二进制是它转发的目标，也可以直接敲。

SDK 的 `bin/` 下有三个可以直接敲的可执行文件：

- **`z42c`** — 编译器。对工程或单个源文件做编译，产出 `.zpkg` / `.zbc`。`z42 build`
  原样转发给它。
- **`z42b`** — 构建编排器。把「编译 → 运行 / 发布」串起来。`z42 new` / `test` / `bench`
  / `clean` 以及 `z42 publish` 的后半程转发给它。

- **`z42d`** — 开发者工具。目前可用的是 `install`（装编辑器集成）与 `symbolicate`
  （还原崩溃栈）。

z42c 还有一组 `--dump-*` 编译阶段转储开关，面向改编译器本身的人，不在本书范围。

## 退出码

两个工具共用同一套契约（`src/compiler/z42c.driver/src/Main.z42:25`–`:29`）：

| 码 | 常量 | 含义 |
|---|---|---|
| `0` | `ExitCode.Ok` | 成功 |
| `1` | `ExitCode.BuildError` | 编译 / 构建失败，诊断已输出 |
| `2` | `ExitCode.UsageError` | 命令行或工程文件用法错误（参数缺失、未知选项、找不到清单、清单歧义） |

---

## `z42c build [<manifest>] [options]`

编译一个包，产出 packed `.zpkg` 到 dist 目录。不给清单时按[工程定位](cli-z42.md#工程定位)
规则从当前目录向上找；定位到工作区清单时等价 `--workspace`。

```console
$ z42c build
cached: 0/1 files
wrote -> ./dist/hello.zpkg (indexed, 1 zbc)
cache -> ./artifacts/debug/.cache (1 files)
```

| 选项 | 作用 |
|---|---|
| `--release` | release profile（默认 debug） |
| `--workspace` | 按拓扑序编译工作区全部成员 |
| `--output-dir <dir>` | 所有产物写到 `<dir>` |
| `--no-incremental` | 全量重建，忽略构建缓存 |
| `--opt <name>` | 开启一项优化（可重复） |
| `--no-opt <name>` | 关闭一项优化（可重复） |
| `--jobs <n>` | 包内文件级并行编译度，默认 `1`（串行） |
| `--fix` | 把 analyzer 携带的代码修复就地写回源文件 |
| `-q` / `--quiet` | 不输出进度行，诊断照常打印 |
| `-h` / `--help` | 显示帮助 |

未识别的选项报错退出 2。

### 优化开关

优化是**可独立勾选的集合**，不是「高档含低档」的档位。`--opt` / `--no-opt` 各自可重复，
先加后减：

```console
$ z42c build --opt inline --opt dce --no-opt copy-prop
```

| 名字 | 作用 |
|---|---|
| `const-fold` | 常量折叠 + 代数恒等式 |
| `copy-prop` | 拷回冗余消除 + use-site 级联 |
| `dce` | 纯指令死代码删除 |
| `inline` | 函数内联 |
| `cse` | 公共子表达式消除 |
| `licm` | 循环不变量外提 |
| `stack-alloc` | 逃逸分析栈上分配 |
| `loop-alloc-reuse` | 循环内分配外提 + 对象复用 |
| `readonly-load` | `readonly` 字段读消重与外提 |
| `pure-call` | 纯函数调用消重与外提 |
| `dead-branch` | 常量条件死分支消除 |
| `devirt` | 基于 `sealed` 的去虚化 |
| `all` | 上面全部 |
| `none` | 一个都不开 |

未知名字报错退出 2（错误行里只列出了其中几个名字，全集以本表为准）。

**profile 默认**：`--release` = `all`，debug = `none`（`-O0`，忠实可调试）。一旦命令行
出现任何 `--opt` / `--no-opt`，本次构建的优化集就以 profile 默认为基、按命令行加减得出。

**命令行是唯一的覆盖入口**——清单里写 `[optimize]` 段不会改变本次构建开了哪些优化。

### `--fix`

analyzer 在报诊断的同时可以携带一个「代码修复」（一组文本编辑）。`--fix` 让编译时把这些
修复**就地重写**回源文件；不加 `--fix`、或 analyzer 没带修复时源文件不动。被 `[lints]`
或 `#suppress` / `[Suppress]` 抑制的诊断不会应用修复。

修复由 analyzer 自身产出，包括第三方 `[analyzers]` zpkg——「谁报诊断谁产修复」。
**只对单包 `build` 生效**，`--workspace` 下不应用。

## `z42c --emit-zbc <file.z42> <out.zbc> [--opt-all]`

把**单个**源文件编译为 `.zbc`。有编译错误时逐条打印诊断、退出 1、**不写产物**；
无错但有 warning 时照样把 warning 打出来。

```console
$ z42c --emit-zbc bad.z42 bad.zbc
z42c: 1 error(s) in bad.z42
  bad.z42(2,15): E0443: undefined type: NoSuchTypeAtAll
$ echo $?
1
```

`--opt-all` 按 release 全优化编。不给它时走的是 emit-zbc 自己的默认优化集，比 release
少若干 pass。

设了 `Z42_LIBS` 时会扫该目录做跨包依赖解析，`using` 到的 stdlib 符号才能 emit 出正确的
全限定函数名；没设则回落为「无依赖单文件」编译。

产出的裸 `.zbc` **没有烤入口**：直接 `z42 run x.zbc` 会报 `no entry point`，因为
`z42 run` 没有传入口函数名的旗标。要得到能跑的产物用 `z42 build` / `z42c build`
（它把 `Main()` 烤进 zpkg）。

## `z42b`

z42b 编译为 `z42.builder.zpkg`，用户通过 launcher 到达它的命令：

| z42b 命令 | 用户入口 | 说明 |
|---|---|---|
| `new` | `z42 new` | 旗标见 [`z42` 命令面](cli-z42.md) |
| `test` | `z42 test` | 同上 |
| `bench` | `z42 bench` | 同上 |
| `clean` | `z42 clean` | 同上 |
| `publish` | `z42 publish` | launcher 先解析已安装 workload 的 apphost，再转发 |
| `build` | — | 供编排方使用；用户面的 `z42 build` 走 z42c |
| `export` | — | 供编排方使用；用户面的 `z42 export` 由 launcher 实现 |

直接敲 `z42b <verb>` 时旗标与 `z42 <verb>` 一致，只有 `publish` / `export` 两处差别：
`z42b publish` 的 `--rid` 不会默认到宿主（要默认得走 `z42 publish`），`z42b export`
只认 `--rid` 与 `--release`、不接受 `--bundle-id` / `--app-id` / `--entry` 那组旗标。

## `z42d install <target>`

把**这个 SDK 自带的**编辑器集成装进你的编辑器。

```sh
z42d install vscode
```

| 目标 | 装到哪 |
|---|---|
| `vscode` | `~/.vscode/extensions/z42.z42-lang/` |

装完**重启编辑器**才生效。重复执行即更新（覆盖同一目录），不会留下多份。

资产来自 SDK 自己的 `editors/` 目录，所以**装了 z42 就能装编辑器集成，不需要 clone 仓库**。

退出码：

| 码 | 含义 |
|---|---|
| `0` | 装好了 |
| `1` | 这个 SDK 打包时没带编辑器资产，或找不到 home 目录 |
| `2` | 目标名不认识（会打印已知目标列表） |

> 当前只提供 VSCode 的**语法高亮与括号/注释配置**（声明式 TextMate grammar）。
> 没有语言服务器，因此没有跳转定义、补全、实时诊断。

## `z42d symbolicate`

见 `z42d symbolicate --help`：把发布档的崩溃栈（`at <fn> +0x<off>`）配合归档的 `.zsym`
还原成 `file:line:col`。
