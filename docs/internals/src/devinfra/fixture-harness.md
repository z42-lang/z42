# 声明式夹具（expect.toml）

> 代码：`scripts/common/xtask_fixture_harness.z42`（引擎）、`scripts/build/xtask_compiler_e2e.z42`（z42c 套件）、
> `scripts/test/xtask_test_dist.z42`（发行包套件）
> 相关：[测试用例组织规范](test-layout.md) · [GREEN gate](test-gate.md) · [产物目录布局](artifacts-layout.md)

## 概述

命令行与构建行为类的测试写成「一个目录 + 一份 `expect.toml`」：目录里是最小工程（或几个互相依赖的工程），
`expect.toml` 写要跑的命令和断言。xtask 里只有一个通用引擎负责暂存、逐步执行、判定；被测的是哪套工具链由**套件**
决定（工具表 + 占位符）。加一条用例 = 加一个目录，不改 xtask。

## 设计目标与约束

- **用例是数据**：断言写在 `expect.toml`，不在 xtask 里写专项检查。现有的键写不出某个断言时，给引擎加一个通用的键
  并补进本页，而不是给某个用例开特例。
- **一个引擎、多个套件**：开发树工具链和打包出的 SDK 的差别只体现在工具表与占位符上，断言键和语义处处一致。
- **源码树零写入、每轮从零开始**：先把整棵夹具目录暂存出去再跑，上一轮的残留冒充不了这一轮的结果。
- **失败自带现场**：判失败时打印第几步、哪条断言、实际的退出码 / stdout / stderr。

## 方案与决策

| 问题 | 决定 | 为什么不选另一个 |
|---|---|---|
| 不同的被测工具链怎么接入 | 套件提供工具表（`FixtureTool`：程序、固定参数、是否带 `target`、缺省环境）与占位符 | 每个套件各写一份 harness：断言键的写法和语义会各自漂移 |
| 同一个工具的便携 / 安装两种形态 | 工具的缺省环境定一种形态，用例需要另一种时用 `env` / `env_remove` 覆盖 | 套件级的形态矩阵：现有用例没有两种形态都要跑的，矩阵只会让每个用例翻倍 |
| 平台差异 | 步骤级 `os = [...]`（只在列出的 OS 上执行该步） | 按平台拆用例：同一条契约分散在几个目录里，改一处漏一处 |
| 进程环境 | 工具缺省与步骤覆盖先合并、再交给子进程 | 两层直接叠到 `Process` 上：`Process` 对同一变量「删」恒胜过「设」，步骤就无法设回工具缺省删掉的变量 |

## 机制

### 1. 套件

| 套件 | 夹具目录 | 命令 | 暂存位置 | 套件 README |
|---|---|---|---|---|
| z42c 命令行与构建行为 | `src/compiler/z42c.driver/tests/fixtures/cli/` | `xtask test compiler` | `artifacts/intermediate/` 下的镜像 | [cli/README.md](../../../../src/compiler/z42c.driver/tests/fixtures/cli/README.md) |
| 发行包（打包出的 SDK） | `src/toolchain/launcher/tests/fixtures/package/` | `xtask package verify` | 系统临时目录（仓库外），全部通过后删除 | [package/README.md](../../../../src/toolchain/launcher/tests/fixtures/package/README.md) |

每个子目录是一个用例（有 `expect.toml` 才算），按目录名排序依次跑。套件 README 写本套件的工具表、占位符和环境约定；
本页写所有套件共用的格式。

### 2. 一个用例：单步或多步

- **单步**（简写）：顶层就是一步，用套件的缺省工具；可选的 `[run]` 段是它通过后的一个 `run` 步（`target` 必填，
  其余键同下）。
- **多步**：`[[step]]` 数组依次执行，任一步不符即停；顶层只放下面「用例级」的键和文件类断言，在全部步骤之后判。

用例级的键：

| 键 | 含义 |
|---|---|
| `desc` | 一句话：这条用例守什么。原因不显然时，用例目录里另放 README 说明 |
| `outside_repo` | `true`：用例拷到仓库外的 `/tmp/z42c-e2e-<树名>-<套件>-<用例>`（每轮重置，按树区分免得并行的 worktree 互相抹掉）再跑。给守「仓库外的消费方」那条路的用例用 —— 在仓库里 z42c 上溯找得到仓库根，走的是另一条判据 |
| `tags` | 字符串数组。套件可以只跑带某个 tag 的用例（发行包套件的 `DIST_SMOKE_ONLY=<tag>`） |

### 3. 每一步的动作（`tool`）

`tool` 不写时用套件的缺省工具。除了套件工具表里的工具，引擎自带三个动作：

| `tool` | 做什么 | 键 |
|---|---|---|
| 套件工具（如 `z42c`、`z42`） | 起一个进程：工具的程序 + 固定参数 +（需要时）`target` + 这一步的 `args`；按工具设定，跑前先删掉 `absent` 里列的文件，不让残留冒充结果 | `args`、`target`、`cwd`、`libs`、`env`、`env_remove` |
| `copy` | 拷贝。`from` 是一个文件、一个目录（整棵拷成 `to`），或 `<目录>/<通配>`（如 `{stdlib}/*.zpkg`）；`to` 以 `/` 结尾 = 拷进该目录 | `from`、`to` |
| `remove` | 删掉文件或目录；文件名可带通配（`shipped/z42c.*`） | `paths` |
| `check` | 不执行任何东西，只判这一步的文件类断言（配合 `os` 写平台相关的判据） | — |

每一步都可以带 `os = ["windows" \| "macos" \| "linux", …]`：只在列出的 OS 上执行，其余平台跳过这一步。

进程类步骤的键：

| 键 | 含义 |
|---|---|
| `args` | 参数数组 |
| `target` | 相对用例目录的路径。工具声明要 `target` 时接在固定参数之后（如 z42vm 跑的 zpkg）；工具的程序为空时它本身就是要跑的程序（如发布出的 apphost） |
| `cwd` | 工作目录，相对用例目录；缺省是用例目录 |
| `libs` | 这一步的 `Z42_LIBS`（相对用例目录）；缺省是套件的 stdlib。只有声明了 `Z42_LIBS` 的工具接受它，其余工具写它判失败 |
| `env` | 额外的环境变量（内联表，如 `env = { Z42_HOME = "{case}/sdkroot" }`，值可为 `""`） |
| `env_remove` | 删掉的环境变量（从 xtask 继承来的，或工具缺省设的） |

环境的最终值 = 工具缺省（设 / 删）之上依次叠这一步的 `env`、`env_remove`，同一变量后写的胜。

### 4. 断言

进程类步骤先判退出码与输出，再判文件；`copy` / `remove` / `check` 只判文件类断言。

| 键 | 含义 |
|---|---|
| `exit` | 期望退出码：整数，或 `"nonzero"` |
| `stderr_contains` / `stderr_not_contains` / `stderr_empty` | stderr 必须包含 / 不得包含的子串；去掉首尾空白后必须为空 |
| `stdout_contains` / `stdout_not_contains` | stdout 必须包含 / 不得包含的子串 |
| `stdout_equals` / `stdout_starts_with` / `stdout_empty` | stdout 去掉首尾空白后必须等于 / 原样必须以它开头 / 必须为空 |
| `stdout_count` | `[["子串", "次数"]]`：子串在 stdout 里恰好出现这么多次（如增量构建命中缓存的成员数） |
| `output_contains` / `output_not_contains` | stdout + stderr 合起来必须包含 / 不得包含（不关心打在哪个流上时用） |
| `exists` / `absent` | 必须存在 / 不得存在的文件或目录 |
| `same_bytes` / `diff_bytes` | `[["a", "b"]]`：两个文件必须逐字节相同 / 必须不同（可复现构建、缓存键对照） |
| `file_contains` / `file_not_contains` | `[["路径", "子串"]]`：文件必须存在且包含 / 不包含子串 |
| `dir_entries` | `[["目录", "名字 名字 …"]]`：目录的条目恰好是这些（顺序无关；如「源文件旁不留产物」） |

路径都相对用例目录；占位符展开后已是绝对路径的（如 `{sdk}/cache`）原样使用。

### 5. 占位符

所有字符串值里都可以写占位符。引擎自带三个，其余由套件提供（见各套件 README）：

| 占位符 | 值 |
|---|---|
| `{case}` | 用例目录（实际运行处的绝对路径） |
| `{sep}` | 路径列表分隔符（`:` / Windows 上 `;`） |
| `{exe}` | 可执行文件后缀（Windows 上 `.exe`，其余为空） |

## 实现

引擎按「套件 → 用例 → 步骤 → 断言」四层展开：`_runFixtureSuite` 暂存并遍历用例、`_runFixtureCase` 分单步 / 多步、
`_fxStep` 分派动作、`_fxCheck` / `_fxFileChecks` 判定。工具表的「怎么起进程」只有一处可覆盖：`FixtureTool.Begin`。
基类直接起程序，不碰任何 z42 变量；z42c 套件的 `DevFixtureTool` 覆盖成 `_z42Proc` / `_z42bProc`，让子进程的
`Z42_LIBS` 与编译器 probing 仍只经那两个工厂设置（`xtask check` 的 proc-env 门禁守着这一条）。

| 组件 | 位置 |
|------|------|
| 引擎：`FixtureSuite` / `FixtureTool` / 环境合并 / 判定 | `scripts/common/xtask_fixture_harness.z42` |
| z42c 套件（开发树 z42c / z42b / z42vm） | `scripts/build/xtask_compiler_e2e.z42` 的 `_testCompilerCliFixtures` |
| 发行包套件（包内 z42 / z42c / z42b / z42vm / 发布出的程序） | `scripts/test/xtask_test_dist.z42` 的 `_distFixtureSuite` |
| 暂存（拷整棵夹具树，跳过 `artifacts/` / `dist/`） | `scripts/common/xtask_fs.z42` 的 `_stageFixtureTree` / `_copySourceTree` |

## 边界与限制

- 断言只有子串、相等、前缀、计数和存在性，没有正则。
- 用例与步骤都串行执行。
- `{case}` 是未经 realpath 的路径。macOS 上 `/tmp`、`/var` 是符号链接，被测程序若打印 realpath，「不得含 `{case}`」
  这类判据会漏判。
- 夹具目录里不能有名为 `dist` 或 `artifacts` 的子目录：暂存时跳过它们，`dist/` 还被仓库的 `.gitignore` 忽略。
