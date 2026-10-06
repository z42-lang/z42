# z42c 命令行行为夹具

每个子目录是一个用例：一个最小工程（或几个源文件、几个互相依赖的工程）加一份 `expect.toml`；
用例目录里的 README 说明它为什么这样摆（例如 `path-dep-closure`）。由 `xtask test compiler`
的 e2e 段跑：整棵目录先暂存到 `artifacts/intermediate/compiler/z42c.driver/tests/fixtures/cli/`
（每轮从零开始，源码树零写入），再在拷贝里逐个用例按 `expect.toml` 执行、判定。

`expect.toml` 的格式（单步 / 多步、`copy` / `remove` / `check`、全部断言键、`outside_repo`、通用占位符）是所有夹具套件
共用的，见 [声明式夹具（expect.toml）](../../../../../../docs/internals/src/devinfra/fixture-harness.md)。本页只写这个套件
自己的工具与占位符（定义在 `scripts/build/xtask_compiler_e2e.z42` 的 `_testCompilerCliFixtures`）。

## 工具

工具都跑**开发树**的工具链，由 z42vm 起；`Z42_LIBS` 是这一步的 `libs`（相对用例目录），缺省是 stdlib flat。

| `tool` | 做什么 | 额外的 |
|---|---|---|
| `z42c`（缺省） | `z42c <args>`（本树的 driver） | 跑前先删掉 `absent` 里列的文件 |
| `z42b` | `z42b <args>`（本树建出的 z42b，编译器成员 dist 经 `Z42_PROBING_PATHS` 给它） | 同上；z42b 没建出来时这一步判失败 |
| `run` | 用 z42vm 跑 `target`（相对用例目录） | — |

## 占位符

| 占位符 | 值 |
|---|---|
| `{stdlib}` | stdlib flat 目录 |
| `{compiler}` | 开发树的编译器目录（driver 的自包含 dist，发布态 `<sdk>/programs/z42c/` 的对应物） |

`outside_repo = true` 的用例拷到 `/tmp/z42c-e2e-<树名>-cli-<用例>` 再跑。

加用例 = 加一个目录。断言写不进现有的键时，给引擎加一个通用的键（并补进格式页），不在 xtask 里写专项检查。
