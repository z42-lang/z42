# Proposal: REPL `.type <expr>` 元指令（迭代三）

## Why
迭代三：REPL 缺经典内省元指令。`.type <expr>`（类 Python `type(x)`）是高频需求——「这东西是什么类型？」
当前只能 `expr.GetType().Name` 手打。设计文档早列了 `.type`（`docs/design/toolchain/repl.md` 指令表），
一直未接。

（迭代三另两项——命名空间名补全、错误行号——因复杂度暂缓：命名空间需含点前缀+替换区间处理；错误行号需
把生成源行号回映到用户输入行。见 Out of Scope。）

## What Changes
- **`.type <expr>` 元指令**（`interactive_main.z42`）：复用 `Script.Eval` 求值 `(<expr>).GetType().Name`
  并打印——零新反射管线，对任意表达式适用（会话变量经 Rewriter 改写）。表达式求值一次（与 Python
  `type(...)` 语义一致，副作用照常）。求值失败 → 打印错误。`.help` 同步列出。
  - 语义：**运行期类型**（User 裁决，2026-08-06）——设计文档描述文字与 `[refl]` 标注曾矛盾，以运行期为准、更新文档。
- **成员 inline ghost 修复**（`corelib/repl.rs`，User 报告）：#122 的 `identifier_hint` 为性能**跳过成员上下文**
  （`recv.<word>` → 不 ghost），导致「输入 `Console.W` 不提示 `WriteLine`」。改：成员上下文也 ghost——
  completer 按需 reconcile 接收者类型（首次命中后缓存），每键成本与 Tab 相当、不比裸词 ghost 更重。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/toolchain/interactive/core/interactive_main.z42` | MODIFY | 派发加 `.type ` 分支；`_showType` 新函数；`_help` 补一行 |
| `src/runtime/src/corelib/repl.rs` | MODIFY | `identifier_hint` 去掉成员上下文跳过 → 成员方法也 inline ghost |
| `docs/design/toolchain/repl.md` | MODIFY | 指令表 / 落地状态标 `.type` 已接（运行期语义）；补全段成员 ghost |

**只读引用**：
- `src/toolchain/scripting/src/Script.z42` — `Script.Eval` / `EvalResult` 契约

## Out of Scope
- 命名空间名补全（`.using Std.` → `Std.IO`）——含点前缀 + 替换区间，留后续。
- 错误行号 / 源片段高亮——REPL 把输入包进生成源，诊断行号指向生成源；需行号回映，留后续。
- `.members <Type>`、`.mode`、`.history`、`.time`/`.counters`——各需新 builtin / diagnostics，留后续。
- 静态类型（不求值）——本 MVP 走 eval + GetType（有副作用，同 Python）；不做纯静态推断版。

## Open Questions
- 无。
