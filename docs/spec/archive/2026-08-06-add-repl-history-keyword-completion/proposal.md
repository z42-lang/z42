# Proposal: REPL 历史跨会话持久 + 关键字补全

## Why
迭代一（REPL 日常体验快赢）的两项 REPL 专属改进：
- **历史不跨会话**：rustyline `DefaultHistory` 仅进程内，退出即丢——上一次会话敲过的命令，下次上箭头
  找不回。主流 REPL（python/node/fish）都持久化。
- **补全不含关键字**：Tab 补全作用域候选（变量/声明名/类型），但不补语言关键字（`while`/`return`/
  `using`/...），少了最基础的一类。

（原迭代一的「结果富展示 ResultFormatter」按 User 意见移出——它是通用 stdlib 的 ToString/反射格式化
能力，非 REPL 专属，单独立项。）

## What Changes
- **历史持久**（`corelib/repl.rs`）：编辑器初始化时 `load_history($HOME/.z42_history)`，每行 `save_history`
  ——best-effort（缺文件/写失败不影响 REPL）。`$HOME`（Windows 回退 `%USERPROFILE%`）都无 → 退化为纯进程内。
- **关键字补全**（`Completer.z42`）：裸标识符补全追加 z42 关键字。关键字表以 z42c.syntax 的 `Lexer`
  （`KeywordCount`/`KeywordNameAt`）为**权威源**——零硬编码、零漂移，新增关键字自动纳入。仅前缀非空时补。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/corelib/repl.rs` | MODIFY | `history_path()` 新函数；编辑器 init 载入 + 每行保存历史 |
| `src/toolchain/scripting/src/Completer.z42` | MODIFY | `using Z42.Syntax`；`_addKeywords` 新函数 + 接入 ③ 裸标识符分支 |
| `docs/design/toolchain/repl.md` | MODIFY | 行编辑器段补「历史持久」+ 补全段补「关键字」 |

**只读引用**：
- `src/compiler/z42c.syntax/src/Lexer.z42` — 关键字表访问器（`KeywordCount`/`KeywordNameAt`）契约

## Out of Scope
- 命名空间名补全（`.using ` 后补 `Std.IO`）——需 `.using` 上下文识别，留迭代二。
- 基元类型会话变量的 `obj.` 成员补全——留迭代二。
- 输入行语法高亮——`Highlighter` 需 per-keystroke tokenizer，单独评估。
- 历史去重 / 大小上限 / `.history` 元指令——MVP 用 rustyline 默认。

## Open Questions
- 无（历史文件路径 `$HOME/.z42_history` 固定；关键字源用 Lexer）。
