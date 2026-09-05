# split-xtask-cli — `xtask_cli.z42` 按命令族拆分

> 类型：`refactor`（最小化模式，无需 DRAFT 规范）。
> 属 scripts/ 结构优化程序**第 3 批**（第 1 批 = PR #472 文档漂移门；第 2 批 = PR #474 golden 枚举四份合一）。

## 问题

`scripts/xtask_cli.z42` 578 行，一个文件里塞了三层不同的东西：

1. **CLI 核心**：全局选项剥离（`--toolchain` / `--verbosity`）、根命令树、顶层 `_dispatch` 分流；
2. **5 个命令族的 router**：`_buildRouter` / `_packageRouter` / `_testRouter`（+ 嵌套
   `_platformRouter`）/ `_depsRouter` / `_benchRouter`；
3. **5 个命令族的 dispatch**：`_dispatchBuild` / `_dispatchPackage` / `_dispatchTest` /
   `_dispatchDeps` / `_dispatchBench`。

而 ② 与 ③ 是**成对**的（同一族的 router 声明什么选项、dispatch 就读什么选项），且**族间零耦合**
——`build` 的 router 和 `test` 的 dispatch 之间没有任何引用。原布局却把它们按「所有 router
一段、所有 dispatch 一段」摊开，改一个族要在文件里跳两处，两处相隔 250+ 行。

## 方案

按**族**切，不按**种类**切：

| 文件 | 内容 | 行数 |
|---|---|---|
| `scripts/xtask_cli.z42`（留原地） | 核心：`_runCli` / 全局选项剥离 / `_cliRoot` / `_dispatch` / `_sliceFrom` | 578 → **238** |
| `scripts/cli/xtask_cli_build.z42` | `_buildRouter` + `_dispatchBuild` | 55 |
| `scripts/cli/xtask_cli_package.z42` | `_packageRouter` + `_dispatchPackage` | 72 |
| `scripts/cli/xtask_cli_test.z42` | `_testRouter` + `_platformRouter` + `_dispatchTest` | 135 |
| `scripts/cli/xtask_cli_deps.z42` | `_depsRouter` + `_dispatchDeps` | 34 |
| `scripts/cli/xtask_cli_bench.z42` | `_benchRouter` + `_benchE2eParser` + `_benchE2e` + `_dispatchBench` | 93 |

**加一个命令族**从此 = `scripts/cli/` 加一个文件 + `_cliRoot` 加一行 `AddRouter` +
`_dispatch` 加一行。

namespace 扁平（`Z42Xtask`）+ `include = ["**/*.z42"]` 递归收录 → **拆文件不需要改任何
import/引用**，纯代码块搬家。

### 为什么放 `scripts/cli/` 而不是各归各家（`scripts/test/` 等）

考虑过把 `test` 族的 CLI 放进 `scripts/test/`、`build` 族放进 `scripts/build/`（"命名归位"）。
没这么做，因为 **CLI 是一个层，不是一个功能**：把 arg 解析/路由混进实现目录会模糊层边界，
而且 `bench` / `deps` 的实现是顶层单文件（`xtask_bench.z42` / `xtask_deps.z42`）、没有对应
子目录可归。`scripts/cli/` 让「整个命令面长什么样」一眼可见。
（`scripts/` 各子目录均无 README，故本目录也不加，与既有约定一致。）

## 验证（CLI 面等价的硬证据）

纯代码搬家，**外部可见的命令面必须逐字节不变**。沿用第 2 批的对账套路：

1. 写一个 harness（`dump-cli-surface.sh`，不进仓库）dump **整棵命令树的 help 文本**——
   root + 5 个 router + 每个叶子 + `test platform` 子树，共 **49 条命令**，含各自 exit code；
2. 改动**前**跑一次存基线（714 行，49/49 exit=0）；
3. 拆完再跑 —— **`diff` 输出 0 行**。

`xtask test` 全绿 10/10 stage。

> 中途 `z42c` 报了 `E0436: namespace 'Std.IO' is used but not imported`——`Console` /
> `ConsoleError` 在 `Std.IO` 而非 `Std`，拆出去的文件漏了这条 using。这条诊断正是 #471
> 加的，一次就定位到文件与行。

## 状态

🟢 完成
