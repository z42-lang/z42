# Proposal: REPL `}` 自动回退一级 + 退格 floor 到前制表位

## Why

REPL 缩进编辑还差最后一块（roadmap Deferred `repl-multiline-future-rbrace-floor`）：

1. **`}` 自动回退一级**——在纯缩进行键入 `}` 时，该行自动 dedent 一级并落 `}`，视觉对齐
   `} else {` / 块闭合，无需手动退格。
2. **退格 floor 到前制表位**——纯缩进行退格时 floor 到前一个 4-制表位（错位一键归正），而非
   固定删一级（`Dedent` 删 4，遇 6→2 这类错位无法一键回正）。

二者当初延后的唯一原因是 rustyline 14 的 `edit_insert_text` 插入后不推进光标：`Replace(WholeLine)`
（唯一 redo-免疫的变量宽度删+插）执行后光标归位行首，破坏 `}` 之后继续输入。**已核实**这是
`edit_insert_text` 的确定性局限（其在 rustyline 全 crate 内唯一调用方就是 `Cmd::Replace` 路径），
可用一行 patch（插入后 `set_pos(cursor + text.len())`）根治，且不影响任何其它行为。User 已裁决
经 `[patch.crates-io]` fork rustyline 承载此 patch（REPL 已是 host-only tier1，fork 一个终端库依赖
合理，不污染任何可移植 stdlib）。

## What Changes

- **rustyline fork（外部，经 `[patch.crates-io]`）**：`edit_insert_text` 插入后推进光标
  （`set_pos(cursor + text.len())`）——修 `Cmd::Replace` 的光标归位 bug；同步向上游提 PR，合并后即可撤 fork。
- **z42 策略层** `ReplEditing.KeyEdit` 新增：
  - `key == "rbrace"`：整行纯空白时 → `"replace:<目标缩进>}"`（dedent 一级 + `}`）。
  - `key == "backspace"`：整行纯空白且光标在行尾时 → 改用 `"replace:<floor 缩进>"`（floor 到前制表位）。
- **Rust 适配壳** `parse_action` 新增 `"replace:<text>"` → `Cmd::Replace(Movement::WholeLine, Some(text))`。
- **键绑定**：cdylib `lib.rs::build_editor` 绑定 `}` 键 → `KeyEditHandler::new("rbrace")`。

> **落点更新（rebase on extract-repl-native-cdylib）**：REPL 行编辑后端已剥离成 host-only cdylib
> `src/runtime/crates/z42-repl/`（PR #325）。故 `parse_action` / `}` 键绑定 / `replace:` 单测均落进
> **cdylib**（`editing.rs` / `lib.rs` / `editing.rs` 内联 `#[cfg(test)]`），不再在 VM 侧
> `corelib/repl*.rs`。`[patch.crates-io]` 仍在 workspace 根 `src/runtime/Cargo.toml`（patch 段只在
> workspace 根生效），但唯一 rustyline 消费者现在是该 cdylib。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/Cargo.toml` | MODIFY | workspace 根加 `[patch.crates-io] rustyline = { git = ... }` 指向 fork |
| `src/runtime/Cargo.lock` | MODIFY | 重新解析锁定 fork commit |
| `src/runtime/crates/z42-repl/Cargo.toml` | MODIFY | 注释更新（rustyline 消费者 + patch 说明）|
| `src/runtime/crates/z42-repl/src/editing.rs` | MODIFY | `parse_action` 加 `replace:` + 头注 + 内联 `replace:` 单测 |
| `src/runtime/crates/z42-repl/src/lib.rs` | MODIFY | `build_editor` 绑定 `}` 键到 `KeyEditHandler::new("rbrace")` |
| `src/toolchain/interactive/repl/src/ReplEditing.z42` | MODIFY | `rbrace` 分支 + backspace-floor + 当前逻辑行辅助 |
| `src/toolchain/interactive/repl/tests/repl_editing/driver.z42` | MODIFY | `}`/floor 的 [Test] 覆盖（KeyEdit 动作串断言）|
| `src/toolchain/interactive/repl/tests/repl_editing/expected_output.txt` | MODIFY | golden 期望 |
| `src/toolchain/interactive/repl/README.md` | MODIFY | 功能索引：键位补 `}`/floor |
| `docs/book/src/toolchain/repl-input-completeness.md` | MODIFY | 机制页：删「坑②延后」段，写 patch + rbrace/floor |
| `docs/roadmap.md` | MODIFY | 关闭 `repl-multiline-future-rbrace-floor` Deferred 行 |
| `src/toolchain/repl/ → src/toolchain/interactive/repl/` | MOVE | 目录搬迁：z42.repl 独立包物理移入 interactive 目录（deps 按名解析，仅动 `xtask_toolchain.z42` 构建路径 + 活文档）|
| `scripts/build/xtask_toolchain.z42` | MODIFY | `_buildReplLib` 构建路径 → `src/toolchain/interactive/repl/` |

**外部交付物（不在本仓库 Scope，但属本变更）**：rustyline fork 仓库 + 其内 `edit_insert_text` patch commit。

**只读引用**：

- `~/.cargo/.../rustyline-14.0.0/src/{edit,line_buffer,keymap,command}.rs` — patch 定位与安全性核实
- `docs/spec/archive/2026-08-23-add-repl-multiline-editing/design.md` — Deferred 来源

## Out of Scope

- 其它缩进键位改动（Tab 网格吸附、Enter 整块判定已落，不动）。
- 非空白行（有词/内容）时的 `}`/退格行为——一律走默认键行为，不介入。

> **Scope 追加（User 授权，2026-08-29）**：REPL 目录搬迁（`src/toolchain/repl/` → `interactive/repl/`）本为独立
> 后续 change，经评估足够简单（deps 按名解析、无自动发现 glob、仅 1 处构建路径 + 少量活文档、archive 冻结不动）
> 且与本变更同触 `repl/` 目录，故并入本 PR 作**独立 commit**（refactor 与 feature 分提交，见 commit-log.md）。

## Open Questions

- [ ] fork 托管位置：`z42-lang/rustyline`（org fork）确认可创建/可推（需 gh 权限）；创建仓库属外部动作，实施时与 User 确认。
