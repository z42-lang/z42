# Proposal: VSCode 语法高亮扩展（IDE 语法支持 A 期）

## Why

在 VSCode 打开 `.z42` 文件没有任何语法显示（无高亮、无括号匹配、无注释切换），
开发体验缺失。IDE 语法支持分两期：A 期（本变更）用声明式 TextMate grammar 解决
高亮痛点，零 node/编译依赖；B 期（后续独立变更）走 `z42d lsp`（LSP server 调
z42c 编译器 API）提供诊断/跳转/语义着色，届时本扩展升级为 LSP client 宿主。

## What Changes

- 新增 VSCode 扩展资产包 `src/toolchain/devtools/vscode/`（语言贡献点 + language-configuration + TextMate grammar）
- grammar 关键字段**生成器驱动**，SoT 链：`Lexer.z42` 关键字表 → `z42c --dump-keywords` 导出 → xtask 生成器（模板 + 分类表，穷尽性机械校验）→ `z42.tmLanguage.json`（生成产物入库）
- z42c.driver 新增 `--dump-keywords` 动词（顺既有 `--dump-tokens/--dump-ast` 模式）；Lexer 公开关键字只读访问器
- 新增 `xtask deps install vscode`（User 裁决 2026-07-07 二次修订：并入收敛后的 deps install 命令面，作 optional positional component；**依赖 `simplify-xtask-deps` 先落地**）：**VSCode 相关资产的统一安装项**——后续 LSP client、snippets 等 VSCode 依赖扩展都收进此 component 一起安装。本变更装第一项——语法扩展：重新生成 grammar + symlink 安装到 `~/.vscode/extensions/`
- 新增轻量 GREEN stage `xtask test vscode-syntax`（in-process 调生成器检查函数：穷尽校验 + 与入库文件字节 diff），挂入裸 `xtask test` 链——Lexer 加关键字而未重新生成 grammar 时 gate 失败；该 stage 亦是手动漂移检查入口（不在 install 上留 `--check` mode flag，遵循 deps 收敛后的"一个动词一个语义"）

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/toolchain/devtools/vscode/package.json` | NEW | 扩展清单：语言 id `z42`、`.z42` 后缀、grammar/config 挂接 |
| `src/toolchain/devtools/vscode/language-configuration.json` | NEW | 注释 `//` `/* */`、括号对、自动闭合、包围对、缩进 |
| `src/toolchain/devtools/vscode/syntaxes/z42.tmLanguage.tpl.json` | NEW | grammar 模板（非关键字规则手写 + `__KW_*__` 占位符）|
| `src/toolchain/devtools/vscode/syntaxes/z42.tmLanguage.json` | NEW | 生成产物，入库（供手动安装/审阅；`--check` 防漂移）|
| `src/toolchain/devtools/vscode/README.md` | NEW | 六段制 README |
| `src/compiler/z42c.syntax/src/Lexer.z42` | MODIFY | 公开只读访问器 `KeywordCount()` / `KeywordNameAt(i)` |
| `src/compiler/z42c.syntax/src/DumpTool.z42` | MODIFY | 新增 `DumpKeywords()`（每行一个，源顺序）|
| `src/compiler/z42c.driver/src/Main.z42` | MODIFY | `--dump-keywords` 分发 + usage（注意 500 行硬限，超则停）|
| `src/compiler/z42c.syntax/tests/lexer/lexer_tests.z42` | MODIFY | 关键字导出单测（非空、计数一致、含哨兵关键字）|
| `scripts/install/xtask_install_vscode.z42` | NEW | 生成器（分类表 + 穷尽校验 + 模板渲染）+ 安装/检查逻辑 |
| `scripts/xtask_cli.z42` | MODIFY | deps `install` 加 optional positional component `vscode` + `test vscode-syntax` 注册与分发 |
| `scripts/test/xtask_test.z42` | MODIFY | `_testAll` 链尾挂 `vscode-syntax` stage |
| `src/toolchain/devtools/README.md` | MODIFY | 新增「编辑器集成」节 + B 期 lsp 展望 |
| `docs/book/src/toolchain/editor-integration.md` | NEW | 机制页：SoT 生成链、分类表、安装原理 |
| `docs/book/src/SUMMARY.md` | MODIFY | 挂新页到第五部分·工具链 |
| `docs/book/src/dev/test-gate.md` | MODIFY | gate 链新增 vscode-syntax stage |
| `docs/workflow/testing/README.md` | MODIFY | stage 说明同步 |
| `.claude/rules/workflow.md` | MODIFY | 阶段 8 GREEN stage 列表 +1 |
| `README.md` | MODIFY | 根 README：IDE 支持一句 + 安装命令 |

**只读引用**（理解上下文必须读，但不修改；不计入并行冲突）：

- `src/compiler/z42c.syntax/src/TokenKind.z42` — 词法元素全集对照
- `scripts/install/xtask_install.z42` — install 命令族既有模式
- `scripts/common/xtask_common.z42` — `_root`/`_exec` 等公共设施
- `examples/*.z42` — 高亮视觉验收样例

## Out of Scope

- **LSP / 语义级支持**（诊断、跳转、hover、semantic tokens）→ B 期独立变更（`z42d lsp`，0.4.x+ 与 dbg/DAP 同期评估）
- `.vsix` 打包发行（需 vsce/node 构建链；发行期再议）
- Windows 安装自动化（symlink 权限问题；MVP 打印手动 copy 指引）
- 文件图标主题、snippet、`.z42.toml` 的 TOML 高亮增强
- 其他编辑器（Vim/JetBrains 等）

## Open Questions

（无——安装平台、gate 挂接、分类表归属均已在 design.md 决策）
