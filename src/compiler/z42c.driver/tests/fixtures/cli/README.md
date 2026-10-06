# z42c 命令行行为夹具

每个子目录是一个用例：一个最小工程（或几个源文件）加一份 `expect.toml`。由 `xtask test compiler`
的 e2e 段跑（`scripts/build/xtask_compiler_cli_fixtures.z42`）：整棵目录先暂存到
`artifacts/intermediate/compiler/z42c.driver/tests/fixtures/cli/`，再在拷贝里逐个用例以用例目录为 cwd
执行 `z42c <args>`，按 `expect.toml` 判定。

## `expect.toml`

| 键 | 含义 |
|---|---|
| `desc` | 一句话：这条用例守什么 |
| `args` | 传给 z42c 的参数（相对路径相对用例目录） |
| `exit` | 期望退出码：整数，或 `"nonzero"` |
| `stderr_contains` / `stderr_not_contains` | stderr 必须包含 / 不得包含的子串 |
| `stdout_contains` / `stdout_empty` | stdout 必须包含的子串 / 必须为空 |
| `exists` / `absent` | 跑完后必须存在 / 不得存在的文件（相对用例目录） |

加用例 = 加一个目录。断言写不进上表（字节比对、多次构建对照……）的，留在 xtask 的专项检查里。
