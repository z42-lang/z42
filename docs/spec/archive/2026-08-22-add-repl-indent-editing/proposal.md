# Proposal: REPL 缩进感知行编辑

## Why

REPL 宿主端行编辑走 rustyline 默认 emacs 键位，对「代码缩进」零定制（`repl.rs` 里无任何自定义 `bind_sequence`）。缩进对齐要逐空格退格、闭合块要手动退回、Tab 只做补全无法插缩进——多行输入手感差。续行缩进已在脚本层算好（`Completeness.ContinuationIndent`，4 空格/级），但编辑器不认它。

## What Changes

给 REPL 加缩进感知键位，**决策逻辑全部落 z42 脚本层**（Rust 只留 policy-free 的重入壳，与既有 `SetCompleter`/`replComplete` 对称）：

- **退格删一级**：光标前全是空格时，一次去掉一级缩进（`indent_size=4`）。
- **Tab 加一级**：光标前无词可补（纯空白）时，加一级缩进；否则走补全。

> **实现期调整（spike 发现，见 design.md D1/D6）**：原计划「4 列网格吸附」+「`}` 回退一级」因 rustyline 的
> redo 语义（对可重复命令强制 `redo(Some(n))`，n=1 覆盖 movement 计数 → 变量计数删除退化成删 1）**无法 redo-免疫地实现**。
> 改用 rustyline 原生 `Cmd::Indent`/`Dedent(WholeLine)`（按 `indent_size` 定量、redo-免疫）→ 退格/Tab 为**定量一级**（非网格吸附）；
> `}` 回退需「删 N + 插入」单命令（`Replace` 计数同样被 redo 覆盖，嵌套层级做不干净）→ **Deferred**。用户核心诉求「退格删缩进 / Tab 插缩进」完全满足。

并**排查（spike）多行粘贴**的现状（bracketed paste 默认已开）：真实 PTY 粘贴刻画行为 + 量测每键回调耗时，出结论；真修若属结构性（真多行编辑）则另开 change。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/toolchain/scripting/src/ReplEditing.z42` | NEW | z42 策略：`KeyEdit(key,line,pos)->action`（返回 indent/dedent/""） + 自由函数 `replKeyEdit` |
| `src/toolchain/scripting/src/Repl.z42` | MODIFY | 加 `SetKeyEditor(fqn)` native 绑定 |
| `src/toolchain/interactive/core/interactive_main.z42` | MODIFY | REPL 启动处 `Repl.SetKeyEditor("Std.Scripting.replKeyEdit")` 注册 |
| `src/toolchain/scripting/tests/repl_editing/driver.z42` | NEW | `KeyEdit` 纯逻辑 golden driver |
| `src/toolchain/scripting/tests/repl_editing/expected_output.txt` | NEW | golden 期望输出 |
| `src/runtime/src/corelib/repl_editing.rs` | NEW | 通用重入 handler + 动作串→`Cmd` 解析 |
| `src/runtime/src/corelib/repl_editing_tests.rs` | NEW | `parse_action` 纯逻辑单测 |
| `src/runtime/src/corelib/repl.rs` | MODIFY | `active_ctx_ptr` 访问器 + `read_one_line` 设 `indent_size(4)` + 绑 Backspace/Tab |
| `src/runtime/src/corelib/mod.rs` | MODIFY | 声明 `repl_editing` 模块 + 注册 `__repl_set_key_editor` builtin |
| `src/toolchain/scripting/README.md` | MODIFY | 功能索引 + 核心文件 |

**只读引用**（理解上下文，不改）：

- `src/toolchain/scripting/src/Completeness.z42` — 复用 4 空格/级缩进约定
- `src/toolchain/scripting/src/Completer.z42` — 参照 `replComplete` 回调注册模式
- `~/.cargo/registry/.../rustyline-14.0.0/src/{binding,keymap,config}.rs` — API 参照

## Out of Scope

- **真·整块多行编辑**（rustyline `Validator` 驱动、方向键跨行改历史行）——独立大 change，本次不做。
- **#5 粘贴的真修**——本次只 spike 出结论；真修按结论另开（依赖上面的多行编辑）。
- **`}` 自动回退一级** + **4 列网格吸附**——rustyline redo 语义限制，Deferred（见 design.md Deferred 段）。
- 补全逻辑本身的改进（fuzzy / 大小写不敏感等）。

## Open Questions

- [x] 网格吸附规则：原定吸附到 4 列制表位；**实现期改为定量一级**（rustyline redo 限制，见 design.md D1/D6）。
- [x] #5 粘贴：本次仅 spike 出结论（User 确认）。**结论**：bracketed paste 默认开，多行粘贴整体入单缓冲、**无双重缩进**，不自动提交；深层「回改任意粘贴行」属多行编辑重构 → Deferred。
- [x] `}` 回退一级：**Deferred**（redo 限制，见下）。
- [x] 定级：`feat(toolchain)` 轻量流程，不走 lang/ir/vm 完整流程（不碰语言/IR/VM 语义，仅 REPL 工具键位）（User 确认）。
