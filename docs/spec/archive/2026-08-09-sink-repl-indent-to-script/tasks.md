# Tasks: sink-repl-indent-to-script

> 状态：🟢 已完成 | 创建：2026-08-09 | 完成：2026-08-09 | 类型：refactor（最小化模式）

**变更说明：** 把 REPL 续行缩进的括号计数从 native（`repl.rs` 的 `bracket_depth` +
`continuation_indent`，`builtin_repl_readline_indented` 内）下沉到脚本层——用既有 `Lexer` 在
`Std.Scripting.Completeness.ContinuationIndent` 里算缩进，native 的 `ReadLineIndented(prompt, buf)`
退化成通用 `ReadLine(prompt, initial)`（读一行 + 用给定串预填），Rust 侧那份含串/注释状态机的括号
计数器整个删除。

**原因：** 完整性判定已在 #146（add-repl-parser-completeness）用 parser 权威、脚本层驱动；残留的
native 括号计数只用于视觉缩进，是冗余的第二套括号逻辑（脚本层 `_isStatement` 早已用 Lexer 数括号）。
下沉后单一权威、native 无括号状态机，且缩进计算改用 Lexer 天然正确处理注释/字符串（含 raw string）。

**文档影响：** `src/toolchain/scripting/README.md`（功能索引 + 核心文件）、
`docs/book/src/toolchain/repl-input-completeness.md`（机制页 + mermaid，对齐日期）、
`docs/design/toolchain/repl.md`（多行/缩进机制 + 行编辑器 API 例）。

## 变更文件（Scope）

| 文件 | 类型 | 说明 |
|------|------|------|
| `src/runtime/src/corelib/repl.rs` | MODIFY | `builtin_repl_readline` 加 `initial`（arg 1）；删 `builtin_repl_readline_indented` / `continuation_indent` / `bracket_depth`；更新模块 doc |
| `src/runtime/src/corelib/mod.rs` | MODIFY | 删 `__repl_readline_indented` 注册（BuiltinId 按名 load-time 解析，删末尾项不动持久 id） |
| `src/runtime/src/corelib/repl_tests.rs` | MODIFY | 删 `bracket_depth`/`continuation_indent` 单测（函数已移脚本层），保留 `word_start` |
| `src/runtime/src/gc/safepoint.rs` | MODIFY | 注释去掉 `builtin_repl_readline_indented` 引用 |
| `src/runtime/Cargo.toml` | MODIFY | 注释 `ReadLine/ReadBlock` → `ReadLine` |
| `src/toolchain/scripting/src/Repl.z42` | MODIFY | `ReadLine(prompt, initial)`；删 `ReadLineIndented` extern |
| `src/toolchain/scripting/src/Completeness.z42` | MODIFY | 新增 `ContinuationIndent(buf)`（Lexer 数括号 → `层数×4 空格`） |
| `src/toolchain/interactive/core/interactive_main.z42` | MODIFY | 续行改 `Repl.ReadLine("... ", Completeness.ContinuationIndent(buf))`；注释更新 |
| `src/toolchain/scripting/tests/completeness/driver.z42` | MODIFY | 加 `ContinuationIndent` golden 断言（回归覆盖，替代删掉的 Rust 单测） |
| `src/toolchain/scripting/README.md` | MODIFY | 功能索引 + 核心文件 |
| `docs/book/src/toolchain/repl-input-completeness.md` | MODIFY | 机制页 + mermaid + 对齐日期 |
| `docs/design/toolchain/repl.md` | MODIFY | 多行/缩进机制段 + 行编辑器 API 例（顺带清 #146 遗留的 ReadBlock 漂移） |

## 任务

- [x] 1.1 `repl.rs`：`ReadLine` 加 initial；删 indented/continuation_indent/bracket_depth + 更新 doc
- [x] 1.2 `mod.rs`：删 `__repl_readline_indented` 注册
- [x] 1.3 `repl_tests.rs`：删括号/缩进单测，保留 word_start
- [x] 1.4 `safepoint.rs` / `Cargo.toml` 注释同步
- [x] 1.5 `Repl.z42`：`ReadLine(prompt, initial)`，删 `ReadLineIndented`
- [x] 1.6 `Completeness.z42`：加 `ContinuationIndent`
- [x] 1.7 `interactive_main.z42`：续行接线 + 注释
- [x] 1.8 `tests/completeness/driver.z42`：加 `ContinuationIndent` 断言（golden 已扩）
- [x] 1.9 文档同步（README / book / design）
- [x] 2.1 `cargo build --release`（z42vm）✔ + `cargo test --lib repl`（word_start 2/2）✔
- [x] 2.2 `xtask build toolchain` ✔ + 交互验收：piped z42i 多行 class 续读求值 ✔；`ContinuationIndent` golden 在 my z42vm 上 diff 通过（0/4/8/串·注释不计）✔
- [x] 2.3 完整 GREEN（`xtask test`）✔ FINAL_EXIT=0：e2e 233/233、cross-zpkg 9/9、stdlib [Test] 全绿、z42c 21/21 单元 + 自举不动点 5/5 gen1==gen2、vscode-syntax ✔
- [x] 2.4 归档

## 备注

- **折入的 pre-existing 修复（独立 commit）**：`src/compiler/z42c.syntax/tests/parser/incomplete_at_eof_tests.z42`
  缺 `using Z42.Core;`（用 `DiagnosticBag` 但未导入）→ file-scoped usings E0436。**非本次引入**（#146 测试
  文件遗漏、#143 迁移后合并的 under-import），但阻塞 `xtask test compiler` GREEN。按 workflow 阶段 8
  「pre-existing 失败须同迭代修复」补 1 行 using（always-correct），单独 commit `fix(compiler): …`。
  memory file-scoped-usings-migration 记录该失败模式对 seed/cache 敏感（origin/main CI 或因缓存未触发）。

- BuiltinId 解析为 load-time by-name（`corelib::builtin_id_of`，`tokens.rs` BuiltinId doc），
  从 `BUILTINS` 删末尾一项不影响任何已编译 zpkg（名未被引用即可，本次连同 z42 `[Native]` 绑定一并删）。
- 基座 seed 为 zpkg 0.34、origin/main 源为 0.35（sealed-devirt bump，非本次引入）→ 本地建走格式差
  两代自举；GREEN 以能建出 0.35 工具链并跑通 z42i 为准。
