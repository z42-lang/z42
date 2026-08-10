# Proposal: REPL 命名空间名补全 + 错误行号回映

## Why
User 点名两项 REPL 改进：
- **命名空间名补全**：`.using Std.` + Tab 不补 `Std.IO`/`Std.Collections`。补全器现只补类型/成员，不补 ns 本身。
- **错误行号回映**：REPL 把用户输入包进生成源（prelude + wrapper），编译错误报的是生成源坐标——
  如 `repl_r5.z42(7,41): E0401: ...`，`repl_r5.z42`（内部）+ 行 7（生成行）对用户毫无意义。

## What Changes
- **命名空间补全**（`Completer.z42`）：`replComplete` 加 `.using ` 上下文分支——按 `DepScanResult.NsNames`
  （全量 ns）前缀匹配，返回「下一段」候选（`Std.C`→`Collections`）。替换区间是 `_wordStart` 给的最后一段，
  段候选正好替换它 → `Std.C`→`Std.Collections`。零 Rust 改（复用现有替换区间）。
- **错误行号回映**（`Script.z42`）：编译错误里每条诊断 `<file>(<L>,<C>): <rest>` 回映——**用户行 = L −
  prelude换行数**（生成源 = prelude + 用户输入，两者统一）。丢内部文件名 + **不可靠的列号**（Rewriter
  改写用户输入会移动列位置）。单行输入（用户行≤1）→ 只留 `<rest>`（不加"第1行"噪声）；多行块 → `第 N 行: <rest>`。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/toolchain/scripting/src/Completer.z42` | MODIFY | `replComplete` 加 `.using ` 分支；`_namespaceComplete` 新函数 |
| `src/toolchain/scripting/src/Script.z42` | MODIFY | `_countNewlines` + `_remapDiag` 新函数；Eval / `_evalDecl` 两处错误路径回映 |
| `docs/design/toolchain/repl.md` | MODIFY | 补全段补命名空间；错误呈现补行号回映 |

**只读引用**：
- `src/compiler/z42c.pipeline/src/DepScan.z42` — `DepScanResult.NsNames`/`NsCount` 契约
- `src/compiler/z42c.semantics/src/IrDump.z42` — 诊断串格式 `<file>(L,C): code: msg`（`:183`）

## Out of Scope
- **列号回映**：Rewriter 改写用户输入 + 各轮 wrapper 前缀不同 → 列号无法可靠映回原始输入，本轮丢弃（只映行）。
- var 声明轮 init 表达式错误落在 prelude（Vars 类）→ 用户行≤1、退化为只给消息（可接受）。
- `.mode`、`.history`、结果富展示——留后续。

## Open Questions
- 无（列号丢弃是 Rewriter 决定的、已在 Out of Scope 说明）。
