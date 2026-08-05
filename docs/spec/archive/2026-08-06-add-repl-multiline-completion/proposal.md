# Proposal: REPL 续行自动缩进 + 补全 inline 提示

## Why
REPL 定义类/函数虽已支持括号平衡多行续读（`Repl.ReadBlock`），但**续行不自动缩进**——
交互模式下每行顶格、要手敲空格，写多层 `class`/`fn` 体验割裂，不像 Python/IPython。
补全虽有 Tab（作用域变量/声明名/`obj.` 成员），但**无 inline 提示**——用户不打 Tab 就看不到候选。
两者都是纯交互 UX 缺口，只在 z42vm 的 rustyline 行编辑层（`corelib/repl.rs`）补齐，不涉及语言/IR/执行语义。

## What Changes
- **续行自动缩进**：`__repl_readblock` 读续行时，按当前括号深度预填 `depth × 4 空格`
  （`readline_with_initial`），光标落缩进后可编辑。纯函数 `continuation_indent` 承载缩进算法。
- **inline ghost 提示**：`ReplHelper` 加 `Hinter`——先试**标识符补全 ghost**（裸标识符，复用现有
  session completer，严格前缀 `starts_with` 过滤 → 提示永不出错，至多缺省），无则回退
  **fish 式历史 ghost**（`HistoryHinter`）。ghost 以 ANSI 灰字（`highlight_hint`）显示。
- Tab 补全（既有 `Completer`）不变。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/corelib/repl.rs` | MODIFY | `read_one_line` 加 `initial` 参数走 `readline_with_initial`；`__repl_readblock` 续行预填缩进；`continuation_indent` 新纯函数；`ReplHelper` 加 `Hinter`（标识符 ghost + 历史 ghost）+ `highlight_hint` 灰字 |
| `src/runtime/src/corelib/repl_tests.rs` | MODIFY | `continuation_indent` 单元测试（缩进深度/嵌套/部分闭合/串·注释） |
| `docs/design/toolchain/repl.md` | MODIFY | 「多行输入」段补自动缩进；新增「inline 补全提示」段 |

**只读引用**：
- `src/toolchain/scripting/src/Completer.z42` — 理解 session completer（`replComplete`）候选来源
- `src/toolchain/interactive/core/interactive_main.z42` — 确认 `ReadBlock` 接线不变

## Out of Scope
- 自动 dedent-on-`}`（续行以闭括号开头时退一级）——v1 靠用户 backspace 一次，列为 follow-up。
- `obj.` 成员的 inline ghost（每键成员反射有性能顾虑）——仅 Tab 支持成员补全，ghost 只覆盖裸标识符。
- 缩进宽度可配置 / Tab 字符缩进——v1 固定 4 空格。

## Open Questions
- 无（缩进宽度、ghost 策略已在实现中定；交互手感由 User 验收后可微调）。
