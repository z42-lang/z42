# z42 命令参考

> **页型**: 参考页 ｜ **状态**: ✅ 已实现（0.6.x）｜ **代码**: `src/toolchain/launcher/core/launcher_cli.z42` · `src/toolchain/builder/core/builder_cli.z42` · `src/compiler/z42c.driver/src/BuildCommand.z42` · `src/libraries/z42.project/src/{ManifestLocator,BuildLayout}.z42`
> **相关**: [CLI 与诊断工具（z42c / z42b）](../compiler/tools.md) · [工程模型](../compiler/project-model.md) · [运行时设置](../runtime/runtime-settings.md) ｜ **对齐**: 2026-09-16

## 概述

`z42` 是 SDK 的唯一命令入口。用户面命令共 12 个：`new` / `build` / `run` / `test` / `bench` / `clean` / `repl` / `publish` / `export` / `workload` / `version` / `help`。
其中 `build` 转发给编译器 z42c，`new` / `test` / `bench` / `clean` 转发给构建编排器 z42b，`repl` 转发给 z42i，其余由 launcher 自己实现。
SDK 是单版本的：launcher 不管理多个运行时版本，运行应用一律用 SDK 自带的 `bin/z42vm`；SDK 的安装与更新由安装脚本负责。

## 约定

- **工程定位**：命令不带工程路径时，从当前目录逐级向上找，每一层依次判定 `z42.toml` → 恰好一个 `*.z42.toml`（多于一个报歧义并列出候选）→ `z42.workspace.toml`；最近者优先，到根仍无则报错并提示 `z42 new`。显式给出目录时只看该目录、不向上。规则唯一实现在 `ManifestLocator`，launcher / z42b / z42c 共用。
- **产物位置**：由 `BuildLayout` 按清单 `[build]` 解析，默认 `dist/`（产物）与 `artifacts/<profile>/`（缓存、生成代码）；z42c 写、`run` 找、`clean` 删同一规则。
- **帮助**：每个命令都接受 `-h` / `--help`；`z42 help <命令>` 与之等价。
- **退出码**：`0` 成功；`1` 执行失败（编译错误、测试失败等，转发命令原样透传工具的退出码）；`2` 用法错误（未知命令 / 选项、找不到工程）。
- **程序参数**：`run` 的程序参数写在 `--` 之后；`--` 之前出现未识别的选项直接报错，不会被当成路径。

## 命令

### `z42 new <name> [--lib] [--path <dir>]`

在 `<dir>/<name>`（默认当前目录下的 `<name>`）创建工程：`z42.toml`、`src/Main.z42`（`--lib` 时为 `src/Lib.z42`）、`.gitignore`、`README.md`。
`<name>` 须匹配 `[a-z0-9][a-z0-9._-]*`；目标目录已存在且非空时报错。测试不单设模板——任何工程的 `tests/*.z42` 都会被 `z42 test` 自动发现。

```console
$ z42 new hello
Created executable project `hello` in hello/

  cd hello
  z42 run
```

### `z42 build [<manifest>] [--release] [--workspace] [...]`

编译工程（增量：只重编改动过的文件），产物写入 dist 目录。定位到的是工作区清单时等价 `--workspace`。
其余选项（`--output-dir` / `--no-incremental` / `--opt` / `--jobs` / `--fix` / `--quiet`）见 `z42 build --help` 与 [CLI 与诊断工具](../compiler/tools.md)。

### `z42 run [<目标>] [--bin <name>] [--mode <m>] [--config <file>] [--set <k>=<v>]... [-- <程序参数>]`

目标为空、目录或清单时：先以 `--quiet` 构建（只输出诊断，不输出进度），再运行产物；目标为 `.zpkg` / `.zbc` 时直接运行。
工程声明了多个 `[[exe]]` 时必须用 `--bin` 选一个。`--mode` / `--config` / `--set` 交给 z42vm，语义见[运行时设置](../runtime/runtime-settings.md)。
简写 `z42 <app.zpkg|.zbc|.z42> [-- args]` 等价 `z42 run <目标>`。定位到工作区清单时报错，需进入成员目录运行。

**单文件**：目标为单个 `.z42` 源文件时，在缓存目录
（`$Z42_CACHE_DIR`，缺省 `<SDK 根>/cache`）的 `run/<源文件绝对路径哈希>/` 下合成一份最小清单
（`kind="exe"`，`include` = 源文件绝对路径），此后完全复用工程构建路径——增量缓存、入口自动检测、
runtimeconfig 侧车一致。**源文件目录不产生任何产物**，源文件也不会被复制进缓存。
单文件只能使用标准库（合成清单不含 `[dependencies]`）；需要依赖时用 `z42 new` 建工程。
`--bin` 与单文件同用报错（单文件只有一个入口）。诊断位置按当前工作目录相对化呈现，
即读者敲什么路径就看到什么路径。

### `z42 test [<目标>] [--filter <s>] [--name <n>] [--format pretty|json] [--list]`

构建工程并运行 `[Test]`：工程声明了测试目标（`tests/*.z42`、`[tests]` / `[[test]]`）时逐个构建运行，否则运行工程自身的 `[Test]`。目标也可是已编译的 `.zpkg` / `.zbc`。机制见[测试流水线](test-pipeline.md)。

### `z42 bench [<目标>] [...]`

同 `test`，运行 `[Benchmark]`。

### `z42 clean [<manifest|dir>]`

删除工程的构建产物（debug 与 release 两个 profile）：dist 目录，以及 `artifacts/`（`[build] output_dir` 未配置时整个删除；配置了时只删解析出的 cache / generated 目录，不删可能与其它工程共享的 output_dir 本身）。

### `z42 repl [--mode <m>] [--config <file>] [--set <k>=<v>]... [-c <expr>]`

启动交互式 REPL；`-c <expr>` 求值一次后退出。机制见 [REPL 输入完整性判定](repl-input-completeness.md)。

### `z42 publish [<manifest>] [--rid <rid>] [--output <dir>] [--no-build] [--self-contained]`

产出可部署应用。当前支持桌面 RID（原生 apphost）；`--rid` 默认宿主。需工程声明 `[platform.desktop] apphost = true`。机制见[部署模型](deployment-model.md)。

### `z42 export [<manifest>] --rid <rid> [...]`

为 ios / android / wasm 生成原生 IDE 工程；平台工具与运行时包需先 `z42 workload install <平台>`。

### `z42 workload install|list|uninstall`

安装 / 列出 / 卸载平台 workload（desktop / ios / android / wasm / test），装入 SDK 目录下的 `runtimes/<ver>/workloads/<wl>/`。

### `z42 version`（`--version` / `-V`）

打印 `z42 <version> (<rid>, <build-date>)`，数据来自 SDK 根目录的 `manifest.toml`；从源码树直接组装、没有该文件的 SDK 打印 `z42 (dev build)`。

### `z42 help [<命令>]`

无参数时列出全部命令；带命令名时等价 `z42 <命令> --help`。
