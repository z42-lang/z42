# 发行包夹具

在**打包出的 SDK**（`artifacts/packages/z42-<version>-<rid>-release`，或 env `DIST_DIR`）上验用户能碰到的路径：
新手命令行（`new` / `run` / `build` / `clean` / 帮助 / 错误路径）、单文件运行、`run --bin` / `--set`、`repl`、
桌面发布（apphost、多 exe、预编译 native）、发布态的 analyzer / generator 与 build hooks。开发树的夹具证明不了
这些：发布态的解析路径（`Z42_HOME`、`programs/z42c/`、launcher 由自己的位置找 SDK）和开发树是不同的几档。

每个子目录是一个用例：最小工程 + `expect.toml`；原因不显然的用例目录里另有 README。由 `xtask test package` 跑
（定义在 `scripts/test/xtask_test_dist.z42` 的 `_distFixtureSuite`）：整棵目录暂存到系统临时目录（**仓库外**，
用户工程不会在 z42 仓库里），全部通过后删掉，有失败时保留并打印路径。

`expect.toml` 的格式（单步 / 多步、`copy` / `remove` / `check`、`os`、全部断言键、`tags`、通用占位符 `{case}` /
`{sep}` / `{exe}`）是所有夹具套件共用的，见
[声明式夹具（expect.toml）](../../../../../../docs/internals/src/devinfra/fixture-harness.md)。本页写这个套件自己的
工具、环境与占位符。

## 工具

工具全部取自包内；环境按「用户机器」清理，不继承 xtask / CI 注入的 z42 变量。缺省环境定了每个工具的形态：
**便携**（不给 `Z42_HOME`，程序由自己的位置找到所在的 SDK）或**安装**（`Z42_HOME` = 包根）。用例需要别的形态时
用 `env` / `env_remove` 覆盖（如发布出的 apphost 对着一份拷贝出来的 SDK 跑）。

| `tool` | 程序 | 缺省环境 | 额外的 |
|---|---|---|---|
| `z42`（缺省） | 包根的 `z42`（launcher） | 便携：删 `Z42_HOME` `Z42_PORTABLE_VM` `Z42_PORTABLE_LIBS` `Z42_LIBS` `Z42_CONFIG` `Z42_MODE` | — |
| `z42c` | `bin/z42c` | 安装：`Z42_HOME` = 包根；删 `Z42_LIBS` `Z42_PORTABLE_VM` `Z42_COMPILER_LIBS` | 跑前先删掉 `absent` 里列的文件 |
| `z42b` | `bin/z42b` | 便携：删 `Z42_HOME` `Z42_PORTABLE_VM` `Z42_LIBS` `Z42_COMPILER_LIBS` | 同上 |
| `run` | `bin/z42vm`，跑 `target` | 安装：`Z42_HOME` = 包根；删 `Z42_LIBS` `Z42_PORTABLE_VM` | — |
| `exec` | `target` 本身（如 `z42 publish` 发布出的程序） | 同 `z42` | — |

工具都不接受 `libs`。包内程序在 Windows 上带 `.exe`，工具表已处理；用例里引用发布产物时写 `{exe}`。

## 占位符

| 占位符 | 值 |
|---|---|
| `{sdk}` | 包根（绝对路径） |
| `{apphost_template}` | desktop workload 的 apphost stub（`xtask package workload` 产出）；作 `Z42_APPHOST_TEMPLATE` 传给 `z42 publish`，代替已安装的 desktop workload |
| `{rid}` | 包的 rid（取自包目录名） |
| `{native_prefix}` / `{native_suffix}` | 该 rid 的 native 库文件名前后缀：`lib` + `.so` / `lib` + `.dylib` / `""` + `.dll` |

## tags 与 `DIST_SMOKE_ONLY`

`DIST_SMOKE_ONLY=<tag> xtask test package` 只跑 `tags` 含 `<tag>` 的用例，并跳过 golden 腿。现有的 tag 只有
`launcher`（命令行路径的用例：不需要 apphost stub，只跑它们时 preflight 也不要求 workload）。本地快跑用；
CI 跑全量。

## 加用例

加用例 = 加一个目录。断言写不进现有的键时，给引擎加一个通用的键（并补进格式页），不在 xtask 里写专项检查。
