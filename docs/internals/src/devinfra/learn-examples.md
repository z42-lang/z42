# 学习手册示例与会话脚本

> 代码：`scripts/test/xtask_test_examples.z42`、`scripts/test/xtask_examples_book.z42`、
> `scripts/test/xtask_examples_transcript.z42`
> 相关：[怎么跑测试](testing.md) · [GREEN gate](test-gate.md)

学习手册里的每段 z42 代码、每条命令及其输出都 include 自仓库根 `examples/`，由 `xtask test docs examples`
用真实 SDK 重放校验。这页讲会话脚本的格式、沙箱语义和本地怎么校验。

## 会话脚本（`*.console`）

文件内容就是书上渲染的终端画面：

```text
$ z42 run -- 小明
Hello, 小明!
$ z42 run
z42c build: 1 error(s) in ./src/Main.z42
  ./src/Main.z42(6,5): E0401: undefined: Consle
[exit: 1]
```

| 写法 | 含义 |
|------|------|
| `$ <命令>` | 一步。只允许 `z42 …` 与内建 `cd` / `cat` / `ls`；参数按空白切分，支持引号；不经过 shell |
| 其后的行 | 期望输出（stdout 在前、stderr 在后） |
| `[exit: N]` | 期望退出码（块末独占一行；省略 = 0） |
| `[..]` | 匹配一行内任意字符（版本号、耗时等会变的内容） |
| `...`（独占一行） | 匹配零到多行 |
| `[ROOT]` | 沙箱根目录（输出里出现绝对路径时） |
| `# ANCHOR: name` / `# ANCHOR_END: name` | 把一个脚本拆成几段分别 include 到页面不同位置 |

- 沙箱：临时目录里放脚本所在目录的副本，环境变量清空（只留 PATH / HOME 等），每步默认超时 120 秒。
- `ls` 按字典序列出、隐藏以 `.` 开头的条目、目录带 `/`；`cat` 原样输出文件。

## 校验与更新

```bash
xtask build toolchain && xtask package dev-sdk          # 先有一个当前源码的 SDK（artifacts/.z42）
xtask test docs examples                                # 书↔示例引用校验 + 重放全部会话脚本
xtask test docs examples getting-started/hello-world    # 只跑一章
xtask test docs examples --book-only                    # 只校验引用（不需要 SDK）
xtask test docs examples <path> --bless                 # 输出确实该变时：用实际输出改写期望，再人工审阅 diff
```

- 失败时报告脚本位置、命令、期望 / 实际逐行对照，并保留沙箱目录供排查。
- `--bless` 整块替换期望输出，不保留 `[..]`——改写后检查 diff，把会变的部分改回通配符。
- 本地预览：`cd docs/learn && mdbook serve --open`。发布：合入 main 后 `deploy-book.yml` 构建到 `https://z42-lang.github.io/z42/learn/`，代码 include 在构建时展开，发布出去的就是 main 上已通过校验的代码。

## playground（预留）

代码块信息串可写 ` ```z42,example=<examples 下相对路径> `（mdBook 保留为 CSS class），将来 playground
按钮据此跳转并拉取**完整工程**运行。门禁已校验该路径存在；按钮本身待 playground 就绪后再加。
