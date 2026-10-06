# z42c 命令行行为夹具

每个子目录是一个用例：一个最小工程（或几个源文件、几个互相依赖的工程）加一份 `expect.toml`；
用例目录里的 README 说明它为什么这样摆（例如 `path-dep-closure`）。由 `xtask test compiler`
的 e2e 段跑（`scripts/build/xtask_compiler_cli_fixtures.z42`）：整棵目录先暂存到
`artifacts/intermediate/compiler/z42c.driver/tests/fixtures/cli/`（每轮从零开始，源码树零写入），
再在拷贝里逐个用例按 `expect.toml` 执行、判定。

## `expect.toml`

一个用例是**一步或多步**：

- **单步**（简写）：顶层就是一次 `z42c <args>`，可选 `[run]` 段在它通过后用 z42vm 跑一个产物
  （`target` 相对用例目录，其余键同下表）。
- **多步**：`[[step]]` 数组依次执行，任一步不符即停；顶层只放 `desc`、`outside_repo` 和文件类断言，在全部步骤之后判。

顶层 `outside_repo = true`：用例拷到 repo 外的 `/tmp/z42c-e2e-<树名>-cli-<用例>`（每轮重置，按树区分免得并行的
worktree 互相抹掉）再跑。给守「repo 外的消费方」那条路的用例用 —— 在仓库里 z42c 上溯找得到仓库根，走的是另一条判据。

### 每一步的动作（`tool`）

| `tool` | 做什么 | 键 |
|---|---|---|
| `z42c`（默认） | 跑 `z42c <args>`；跑前先删掉 `absent` 里列的文件，不让残留冒充结果 | `args`、`cwd`、`libs`、`env`、`env_remove` |
| `z42b` | 跑 `z42b <args>`（本树建出的 z42b，编译器成员 dist 经 `Z42_PROBING_PATHS` 给它）；同样先删 `absent` | `args`、`cwd`、`libs`、`env`、`env_remove` |
| `run` | 用 z42vm 跑一个产物 | `target`（相对用例目录）、`args`、`cwd`、`libs`、`env`、`env_remove` |
| `copy` | 拷贝；`from` 可带通配（`{stdlib}/*.zpkg`），`to` 以 `/` 结尾 = 拷进该目录 | `from`、`to` |
| `remove` | 删掉文件或目录；文件名可带通配（`shipped/z42c.*`） | `paths` |

`cwd` 默认用例目录；`libs`（相对用例目录）是这一步的 `Z42_LIBS`，默认 stdlib flat；`env` 是额外的环境变量
（内联表，如 `env = { Z42_HOME = "{case}/sdkroot" }`，值可为 `""`），`env_remove` 删掉从 xtask 继承来的变量。
`copy` / `remove` 之后同样判这一步的文件类断言。

### 断言

| 键 | 含义 |
|---|---|
| `desc` | 一句话：这条用例守什么 |
| `exit` | 期望退出码：整数，或 `"nonzero"` |
| `stderr_contains` / `stderr_not_contains` | stderr 必须包含 / 不得包含的子串 |
| `stdout_contains` / `stdout_equals` / `stdout_empty` | stdout 必须包含的子串 / 去掉首尾空白后必须等于 / 必须为空 |
| `stdout_count` | `[["子串", "次数"]]`：子串在 stdout 里恰好出现这么多次（如增量构建命中缓存的成员数） |
| `output_contains` / `output_not_contains` | stdout + stderr 合起来必须包含 / 不得包含（不关心打在哪个流上时用） |
| `exists` / `absent` | 必须存在的文件 / 不得存在的文件或目录 |
| `same_bytes` / `diff_bytes` | `[["a", "b"]]`：两个文件必须逐字节相同 / 必须不同（可复现构建、缓存键对照） |
| `file_contains` / `file_not_contains` | `[["路径", "子串"]]`：文件必须存在且包含 / 不包含子串 |

路径都相对用例目录。字符串里可用占位符：`{stdlib}` = stdlib flat 目录、`{compiler}` = 开发树的编译器目录（driver 的
自包含 dist，发布态 `<sdk>/programs/z42c/` 的对应物）、`{case}` = 用例目录（实际运行处的绝对路径）、`{sep}` = 路径
列表分隔符（`:` / `;`）。

加用例 = 加一个目录。断言写不进上面这些键时，给 harness 加一个通用的键（并补进上面的表），不在 xtask 里写专项检查。
