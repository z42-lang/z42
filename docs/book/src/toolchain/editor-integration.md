# 编辑器集成（VSCode）

> **页型**: 机制页 ｜ **状态**: ✅ A 期已实现（声明式高亮）｜ **代码**: `src/toolchain/devtools/vscode/` + `scripts/install/xtask_install_vscode.z42`
> **相关**: [xtask](../dev/xtask.md) · [测试门禁](../dev/test-gate.md) ｜ **对齐**: 2026-07-07

## 概述

VSCode 打开 `.z42` 的语言支持分两期：**A 期（本页，已实现）**——声明式 TextMate
grammar 提供语法高亮、括号匹配、注释切换，零 node/编译依赖；**B 期（规划）**——
`z42d lsp`（LSP server 调 z42c.syntax/semantics API）提供诊断/跳转/语义着色，
本扩展届时升级为 LSP client 宿主。

## 设计目标与约束

- **关键字单一 SoT**：grammar 的关键字必须机械地来自编译器词法器，禁止手工双份
- **零外部构建依赖**：扩展纯 JSON 资产，不引入 node/vsce 到仓库构建链
- **防漂移入 GREEN gate**：Lexer 加关键字而 grammar 未再生成 → 裸 `xtask test` 失败

## 机制

### SoT 生成链

```
Lexer.z42 _initKeywords()           ← 关键字唯一 SoT
    │ KeywordCount()/KeywordNameAt(i)
    ▼
z42c --dump-keywords                ← 每行一个，注册序（DumpTool.DumpKeywords）
    ▼
xtask 生成器（xtask_install_vscode.z42）
    ├─ 分类表（control/declaration/modifier/operator/type/literal，住生成器）
    ├─ 穷尽校验：dump 的每个关键字必须恰好落一个分组；漏/幽灵/重复 → 报错指名
    └─ 模板渲染：z42.tmLanguage.tpl.json 的 __KW_<GROUP>__ → kw1|kw2|…
    ▼
syntaxes/z42.tmLanguage.json        ← 生成产物，入库（GENERATED 标记）
```

为什么分类表不算第二个 SoT：分组（控制流/声明/修饰符…）是纯表现层概念，Lexer 里
本就不存在；防漂移靠穷尽校验闭环——新关键字进 Lexer → `test vscode-syntax` 红 →
开发者被迫补分类并重新生成。

### 命令面与 gate

- `xtask deps install vscode`：重新生成 grammar → symlink `~/.vscode/extensions/z42.z42-lang`
  → 指向 repo 内目录（grammar 改动重载窗口即生效）。deps 依赖模型中的第三类：
  **主机集成，用户显式触发**（编辑器集成无法"用到自动装"）。
- `xtask test vscode-syntax`：in-process 调生成器检查函数——重新生成到内存与入库文件
  字节 diff + 分类穷尽校验；挂 `_testAll` 链尾（守跨子系统 SoT 一致性，性质同自举不动点）。

### grammar 覆盖面（对照 Lexer 现状）

注释 `//` `/* */`（不嵌套）；字符串四形态：`"…"`（转义）、raw `"""…"""`、插值
`$"…{expr}…"`（洞内嵌套高亮，`$self` 递归）、字符 `'…'`；数字：十进制/`0x`/`0b`、
`_` 分隔、小数、指数、后缀 `L u f d m`；属性 `#[…]`；运算符全集（`?.` `??` `=>`
`::` `..` 等）；关键字六组注入 + PascalCase 类型 / `ident(` 函数调用的约定式着色。
scope 命名循 TextMate 惯例（`keyword.control.z42` 等），主流主题开箱着色。

## 边界与限制

- Windows 无自动安装（symlink 需特权）：手动 copy 到 `%USERPROFILE%\.vscode\extensions\`
- 无 `.vsix` 打包（需 vsce/node；发行期再议）
- 语义级能力（诊断/跳转/hover/语义着色）不在 A 期——B 期 `z42d lsp`

## Deferred

- **B 期 LSP**：`z42d lsp` server + 本扩展加 LSP client（TypeScript 构建链届时引入），
  0.4.x+ 与 dbg/DAP 同期评估；见 `src/toolchain/devtools/README.md`
