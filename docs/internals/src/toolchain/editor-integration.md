# 编辑器集成（VSCode 语法高亮）

> 对齐：2026-09-17（change `restructure-docs-three-books`）｜ 代码：
> `src/toolchain/devtools/vscode/`（扩展资产）、
> `scripts/install/xtask_install_vscode.z42`（生成器 + 门禁检查）、
> `src/libraries/z42c.syntax/src/Lexer.z42`（关键字 SoT）、
> `src/compiler/z42c.driver/src/Main.z42:51`（`--dump-keywords` 路由）、
> `scripts/test/xtask_test.z42`（gate 挂接）。

z42 在 VSCode 里的全部语言支持 = **一份声明式 TextMate grammar**，零 node / 零编译依赖，
没有 LSP、没有 `.vsix`。本页写这份 grammar 是**怎么生成出来的**、关键字凭什么不漂移、
装在哪以及为什么。加关键字被 GREEN gate 拦住时、或要把语义级能力接上来时读它。

## 1. 能力面（先划清边界）

| 能力 | 有没有 | 来自 |
|---|:-:|---|
| 语法高亮（注释 / 字符串四形态 / 数字 / 关键字六组 / 属性 / 运算符 / 类型名 / 函数调用） | ✅ | `syntaxes/z42.tmLanguage.json` |
| 括号匹配、自动闭合、包围对、注释切换、折叠标记、缩进规则 | ✅ | `language-configuration.json` |
| 诊断 / 跳转 / hover / 重命名 / 语义着色 | ❌ | 需要 LSP server——不存在 |
| `.vsix` 打包 | ❌ | 需要 vsce/node，仓库不引 node 构建链 |

`src/toolchain/devtools/` 下的 muxer `z42d` 目前只有 `symbolicate` 一个子命令落地，**没有 `lsp` 子命令**。
接 LSP 时的形态是既定的：server 作为 `z42d` 的一个子命令调 z42c 的语法/语义 API（对照 `dbg` 与 DAP 的
「前端 + 协议适配」模式），本扩展升级为 LSP client 宿主，届时才引入 TypeScript 构建链。

扩展本身是**纯声明资产**（`package.json` 无 `main`），所以它不进 `z42d` muxer——它不是 CLI 工具。

## 2. 生成链：关键字只有一个来源

手写 grammar 的通病是关键字表成了第二份真相，加关键字忘了同步就静默少高亮一个词。这里的做法是
**机械生成 + 门禁闭环**：

```
Lexer.z42 _initKeywords()                 ← 关键字唯一 SoT
    │  KeywordCount() / KeywordNameAt(i)
    ▼
z42c --dump-keywords                      ← 每行一个，注册序
    ▼
scripts/install/xtask_install_vscode.z42
    ├─ 分类表（control / declaration / modifier / operator / type / literal）
    ├─ 穷尽校验（漏 / 幽灵 / 重复 → 报错指名）
    └─ 模板渲染：z42.tmLanguage.tpl.json 的 __KW_<GROUP>__ → kw1|kw2|…
    ▼
syntaxes/z42.tmLanguage.json              ← 生成产物，入库
```

生成器 fork in-tree 的 z42c（`--dump-keywords`；编译器没 build 过就先 `build compiler`），
把 dump 切成非空行数组，校验后按**六个** `__KW_<GROUP>__` 占位符注入 alternation。

**确定性**：每组的 alternation 按 **dump 的注册序**输出，不按分类表的声明序——同样的输入必然同样的字节，
入库产物才能拿来做 diff 检查。

### 分类表为什么不算第二个 SoT

分组（控制流 / 声明 / 修饰符 / 表达式运算符 / 基元类型 / 字面量）是**纯表现层概念**，Lexer 里本就不存在
这个维度，没法从 SoT 推导出来。它不成为漂移源，靠的是**穷尽校验的双向闭环**：

- dump 里的每个关键字必须**恰好**落一个分组——落 0 个报「not in any category」并指名该词，
  落 >1 个报「appears in N categories」；
- 分组里的每个词必须在 dump 里存在——否则报「Lexer no longer has」（Lexer 删词后分类表必须跟删，
  防"幽灵"关键字）。

所以新增一个关键字的路径是强制的：进 Lexer → `xtask test vscode-syntax` 变红 → 被迫补分类并重新生成。

## 3. 两个命令面

| 命令 | 做什么 |
|---|---|
| `xtask deps install vscode` | 重新生成 grammar 写回入库路径，然后建 symlink `<repo>/.vscode/extensions/z42.z42-lang` → `src/toolchain/devtools/vscode` |
| `xtask test vscode-syntax` | **in-process** 调同一个生成函数渲染到内存，与入库文件做字节 diff（分类穷尽校验顺带跑了）；产物缺失 → 提示去跑 install |

检查是 in-process 调用生成器的检查函数，不是给 install 留一个 `--check` 旗标——deps 收敛后一个动词一个语义。

`vscode` 在 `xtask deps install` 里是**组件位置参数**（缺省值 = 装平台必备依赖，不碰编辑器资产）。
它属于 deps 依赖模型的第三类：**主机集成，用户显式触发**——编辑器集成没法「用到时自动装」。

`vscode-syntax` 挂在全量 gate 的链尾（`scripts/test/xtask_test.z42`，`_gateStageNames()` 里有它），
守的是跨子系统的 SoT 一致性，性质同自举字节不动点。成本≈一次 z42c fork，可忽略。
CI 的分腿 job 用 `xtask test --skip vscode` 把它挪到别的腿上。

## 4. 装在项目目录，不装用户目录

symlink 落 `<repo>/.vscode/extensions/z42.z42-lang`（**工作区本地扩展**），不是 `~/.vscode/extensions/`：

- 随仓库走——clone 下来跑一条命令就有高亮，不污染 `~`；
- symlink 用**相对路径**（`ln -sfn ../../src/toolchain/devtools/vscode`），仓库整体移动不破链；
- grammar 改动重载窗口即生效（源码目录就是扩展目录，没有拷贝步）；
- 链接路径已在 `.gitignore`（`.vscode/extensions/z42.z42-lang`）。

代价：VSCode 从 1.89 起才支持从工作区加载本地扩展，且首次打开仓库会弹信任/启用提示。
`package.json` 的 `engines.vscode` 声明的是 grammar 贡献点本身所需的下限（`^1.75.0`）——
**工作区本地扩展这条安装路径的实际下限是 1.89**，两个数字不是一回事。

Windows 不做自动安装（symlink 需特权）：生成器直接报错并打印手动指引——把该目录复制到
`<repo>\.vscode\extensions\z42.z42-lang`。

## 5. grammar 覆盖面

模板（`z42.tmLanguage.tpl.json`）里手写九个 repository 块：`comments` / `strings` / `chars` /
`numbers` / `attributes` / `keywords` / `types` / `functions` / `operators`。关键字之外的规则全部手写，
改高亮规则改**模板**，不改生成产物。

- 注释 `//`、`/* */`（不嵌套）
- 字符串四形态：`"…"`（含转义）、raw `"""…"""`、插值 `$"…{expr}…"`（洞内以 `$self` 递归嵌套高亮，
  scope `meta.embedded.line.z42`）、字符 `'…'`
- 数字：十进制 / `0x` / `0b`，`_` 分隔、小数、指数、后缀
- 属性 `#[…]`（含起止标点单独成 scope）
- 运算符全集，按语义分 scope（arithmetic / comparison / logical / bitwise / assignment / increment /
  ternary / range / scope / arrow / null-coalescing / expression）
- 约定式着色：PascalCase 标识符 → `entity.name.type.z42`，`ident(` → `entity.name.function.z42`

scope 命名循 TextMate 惯例（`keyword.control.z42`、`storage.modifier.z42`、`storage.type.primitive.z42` 等），
主流主题开箱就能着色，不需要扩展自带配色。

验收没有自动化的视觉检查——装完打开任意 `.z42`，对着上面这张覆盖表逐项核对。

## 相关

- [REPL 实现](repl.md)——另一个面向源码的交互前端；它的 Tab 补全与关键字表用的是同一个 `Lexer` 权威源
- [包划分与依赖层级](../stdlib/organization.md)——`z42c.syntax` 为什么是 stdlib（前端下沉后才有这条 dump 链）
