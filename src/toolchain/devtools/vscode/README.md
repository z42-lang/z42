# toolchain/devtools/vscode — VSCode z42 语言扩展

## 职责

VSCode 的 z42 编辑器集成资产包（声明式，零 node 构建依赖）：语法高亮（TextMate
grammar）+ 括号匹配/注释切换/自动缩进（language-configuration）。**不做**语义级
支持（诊断/跳转/hover = B 期 `z42d lsp` + 本扩展升级为 LSP client，独立变更）。

## 功能索引

| 功能 | 入口 / 文件 |
|------|-----------|
| 语言注册（id `z42`、`.z42` 后缀） | `package.json` |
| 语法高亮 grammar（生成产物，勿手改） | `syntaxes/z42.tmLanguage.json` |
| grammar 模板（高亮规则的编辑处） | `syntaxes/z42.tmLanguage.tpl.json` |
| 注释/括号/缩进/包围对 | `language-configuration.json` |
| 生成 + 安装 | `xtask deps install vscode`（`scripts/install/xtask_install_vscode.z42`） |
| 防漂移检查（GREEN gate） | `xtask test vscode-syntax` |

## 基础用法

```bash
xtask deps install vscode   # 重新生成 grammar + symlink 到 <repo>/.vscode/extensions/z42.z42-lang
# 重载 VSCode 窗口即生效；首次会提示信任/启用 workspace 扩展（需 VSCode ≥1.89）
```

装在**项目目录**而非用户目录（User 裁决 2026-07-08：工作区本地扩展）：随仓库走、
不污染 `~`；symlink 用相对路径，仓库整体移动不破链（已 gitignore）。
Windows：不支持自动 symlink，把本目录复制到 `<repo>\.vscode\extensions\z42.z42-lang`。

## 如何测试验证

```bash
xtask test vscode-syntax    # 生成器一致性：关键字分类穷尽 + 入库 grammar 与重新生成字节一致
```

视觉验收：安装后打开 `examples/*.z42`，核对注释/字符串（普通、raw `"""`、插值 `$"{}"`、
字符）/数字（hex/bin/`_` 分隔/指数/后缀）/关键字五组/属性 `#[...]`/运算符着色。

## 关联文档

- 机制（SoT 生成链、分类表、安装原理）：[book 编辑器集成](../../../../docs/book/src/toolchain/editor-integration.md)
- 引入：change `add-vscode-syntax-ext`（`docs/spec/archive/`）

## 核心文件

| 文件 | 职责 |
|------|------|
| `package.json` | 扩展清单：语言贡献点 + grammar/config 挂接（纯声明，无 main） |
| `language-configuration.json` | 注释 `//` `/* */`、括号对、自动闭合、包围对、缩进规则 |
| `syntaxes/z42.tmLanguage.tpl.json` | grammar 模板：非关键字规则手写 + `__KW_*__` 占位符 |
| `syntaxes/z42.tmLanguage.json` | **生成产物**（`xtask deps install vscode`），入库；勿手改 |
