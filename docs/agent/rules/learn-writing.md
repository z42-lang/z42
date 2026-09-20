# 学习手册写作规范（docs/learn + examples）

> `docs/learn/`（《z42 学习手册》）与仓库根 `examples/` 的编写规范。文档体系定位见
> [doc-system.md](doc-system.md) 「三本书的角色」「学习手册」与决策 D10；知识库的写法见 [book-writing.md](book-writing.md)。
> 章节规划与写作进度见 [`docs/learn/OUTLINE.md`](../../learn/OUTLINE.md)。

---

## 一、手册与参考的分工

**手册讲「怎么用」**（按学习顺序、从安装到发布），**参考讲「规则是什么」**（按主题、可跳读、规则完整）。
手册讲到某条规则时只讲够用的部分，完整规则**链接** `docs/reference/` 对应页，不复制。
**该链就链**——手册是介绍性的，把系统性的全表（清单字段、CLI 旗标、API、错误码）交给参考手册，
比自己抄一份短表要好。

> 三本书各自的判据与「明确不写什么」见 [doc-system.md 三本书的角色](doc-system.md)——本节不复述。

### 🔴 手册只链参考，不链实现内幕

**手册里不出现任何指向 `docs/internals/` 的链接**（绝对站点 URL 也算）——
这是 [doc-system.md §2.1](doc-system.md) 的硬规则，读者一旦被带进实现细节，
「按学习顺序读一遍就会用」这条判据就塌了。

需要交代「这背后还有别的东西」时，用**不带链接的散文**：

```markdown
✅ 具体怎么回收属于实现细节，本手册不展开。
❌ 具体怎么回收见[垃圾回收](https://z42-lang.github.io/z42/internals/runtime/gc.html)。
```

### 跨书链接用绝对站点 URL

`learn` 与 `reference` 是**两个独立的 mdBook**，分别构建到 `/z42/learn/` 与 `/z42/reference/`。
相对路径在磁盘上能解析、在渲染出来的站点上却走不通，所以跨书一律写完整 URL：

```markdown
完整字段表见[工程清单 z42.toml](https://z42-lang.github.io/z42/reference/toolchain/z42-toml.html)。
```

对应关系：`docs/reference/src/<路径>.md` → `https://z42-lang.github.io/z42/reference/<路径>.html`。

⚠️ 绝对 URL **不在 `xtask test docs` 的死链检查范围内**（它只查相对链接）——
参考手册改动页面路径时，手册这边不会自动报红，需要人工留意。

## 二、页面

- **一章一页**，路径 `docs/learn/src/<part>/<chapter>.md`，英文 kebab-case；写完才挂进 `SUMMARY.md`（未写的章节只留在 `OUTLINE.md`）。
- **开头一段说清本章做什么**；结尾「小结」列要点，需要时加「下一步」。
- **用读者的语言**：第一次出现的术语加粗并当场解释；不说实现细节（编译器内部、VM 结构）——
  需要交代时用不带链接的散文，见 §一。
- **不写历史**（「以前是这样」「某版本起」）；手册跟随最新版（nightly）。
- 与 C# 不同的地方用引用块单独点出（`> 熟悉 C# 的读者请注意：…`）。

### 🔴 读者必须知道「这段代码放哪、怎么跑」

**手册是让人跟着做的，不是代码陈列。** 读者在任何一页上都不该猜「这段片段该存成什么文件、
要不要 `using`、要不要包进 `void Main()`」。

这是一条**结果要求**，不是格式要求——达成它有两种写法，按章节性质选：

**写法甲：散文点名（手把手的章节用）。** 起步部分那种一步步带着建文件的章节，直接在正文里
说「新建一个名为 `hello.z42` 的文件」「把它挪到 `src/Greeting.z42`」，再**整文件** include。
读者要建的文件、完整内容、怎么跑，一条线下来全有了，不需要额外标注。

**写法乙：章首引用块 + 每块标文件名（片段式的章节用）。** 语言基础往后，一章要展示十几个互不
相关的片段，不可能每个都手把手。这时：

1. **章首一个引用块**，交代本章示例目录（带 GitHub 链接）+ 书上是片段、完整可跑的程序就是
   那个文件本身：

   ```markdown
   > **本章代码在 [`examples/basics/operators/`](https://github.com/z42-lang/z42/tree/main/examples/basics/operators)**，
   > 每段代码上方标出它在该目录下的文件。书上是文件里的片段；完整可跑的程序
   > （含 `using Std.IO;` 与 `void Main() { … }`）就是那个文件本身。
   ```

2. **每段 z42 代码上方标出它来自哪个文件**，只写章节目录下的相对尾巴（§三 B5 已保证前缀
   在一章之内恒定，写全路径只会吵）：

   ````markdown
   **`arith/basic.z42`**

   ```z42
   {{#include ../../../../examples/basics/operators/arith/basic.z42:code}}
   ```
   ````

   `console` 块不标——它自己的 `$ z42 run <文件>` 行已经说明了跑的是谁。

两种写法都要在**章尾给出本章示例目录的链接**（写法乙的章首块已含链接则不必重复）。
没有任何示例代码的章节（如安装章）不适用本条。

**报错示例必须同时给出错的源码**，不能只贴 transcript：读者看见 `narrowing.z42(5,17)` 却没有
第 5 行，那条诊断对他毫无意义。

⚠️ **一个源文件里的多个锚点，各自要有对得上的输出。** 一条 transcript 整段 include 到第一个
片段下面，打出的会是整个程序的输出——读者看到的行数与眼前的片段对不上。会话脚本里用
`# ANCHOR:` 把输出按小节切开（锚点行在脚本任意位置都被忽略，可以插在输出中间）。

## 三、代码与终端输出只来自 examples/（由门禁强制）

**书里的每一段 z42 代码、每一条命令及其输出，都 include 自 `examples/`，由 `xtask test examples` 用真实 SDK 重放校验。**

- ` ```z42 ` / ` ```console ` 代码块的正文**只能是一条** `{{#include …}}`（B6）。
- 需要展示**不可运行**的示意代码时，用 ` ```text `，或显式标 ` ```z42,ignore `（少用，门禁会计数）。
- 与 z42 无关、无法在沙箱里运行的命令（安装脚本、`curl`、编辑器操作）用 ` ```sh ` / ` ```powershell `，示例输出用 ` ```text `。
- 片段用**锚点**，不用行号：源文件里写 `// ANCHOR: name` … `// ANCHOR_END: name`，页面写 `{{#include 路径:name}}`（B2、B3）。
  含锚点的文件不能整文件 include（锚点行会被渲染出来，B4）。
- 页面 `<part>/<chapter>.md` **只能 include `examples/<part>/<chapter>/` 下的文件**（B5）；章节之间不共享示例，需要时复制。

## 四、examples/ 的结构

```
examples/<part>/<chapter>/          ← 与页面路径一一对应（B9）
  <project>/                        ← 完整、可直接 `z42 run` 的工程
    z42.toml
    src/Main.z42
    run.console                     ← 会话脚本：沙箱 = 本目录的副本
  <scene>/<scene>.console           ← 不需要现成工程的会话（如演示 `z42 new`），放在空目录里
```

- 每个工程至少被一个会话脚本覆盖（脚本在工程目录或其上级目录，B8）；每个会话脚本至少被一页 include（B7）。
- `examples/` 只放手册配套内容。**语言 / 库特性的覆盖写成测试**（`src/tests/`、库的 `tests/`），不要往这里加演示文件。
- 源文件不写「本文件演示了…」之类的头注释——说明写在页面里；代码里的注释按正常程序的标准写。

## 五、会话脚本（`*.console`）

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
- **尽量少用通配符**：书上展示的是读者真正会看到的内容，`[..]` 只用于确实会变的部分。

## 六、校验与更新

```bash
xtask build sdk                                    # 先有一个当前源码的 SDK（artifacts/.z42）
xtask test examples                                # 书↔示例引用校验 + 重放全部会话脚本
xtask test examples getting-started/hello-world    # 只跑一章
xtask test examples --book-only                    # 只校验引用（不需要 SDK）
xtask test examples <path> --bless                 # 输出确实该变时：用实际输出改写期望，再人工审阅 diff
```

- 失败时报告脚本位置、命令、期望 / 实际逐行对照，并保留沙箱目录供排查。
- `--bless` 整块替换期望输出，不保留 `[..]`——改写后检查 diff，把会变的部分改回通配符。
- 本地预览：`cd docs/learn && mdbook serve --open`。发布：合入 main 后 `deploy-book.yml` 构建到 `https://z42-lang.github.io/z42/learn/`，代码 include 在构建时展开，发布出去的就是 main 上已通过校验的代码。

## 七、playground（预留）

代码块信息串可写 ` ```z42,example=<examples 下相对路径> `（mdBook 保留为 CSS class），将来 playground
按钮据此跳转并拉取**完整工程**运行。门禁已校验该路径存在；按钮本身待 playground 就绪后再加。
