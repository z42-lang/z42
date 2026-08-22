# Proposal: REPL Tab 缩进网格吸附

## Why

`add-repl-indent-editing`（#253）落地了 REPL 缩进感知键位（退格删一级 / Tab 加一级），但把 Tab/退格的
「网格吸附」（对齐到 4 列制表位）与 `}` 自动回退延后，理由是「需 patch rustyline」。

调查（本 change 探索期）发现 #253 的判据不准确：`Cmd::Replace(Movement::WholeLine, Some(text))`
**对 redo 计数覆盖免疫**（`WholeLine` 无 `RepeatCount`），本可用于变量宽度的删+插。但 PTY spike
实测暴露另一处 rustyline 局限：`edit_insert_text` 插入后**不推进光标** → `Replace(WholeLine)` 执行完
光标归位到行首（col 0），破坏 `}` 之后继续输入（`} else {`、`};`）。z42 非缩进敏感，`}` 自动回退/
退格 floor 纯属**视觉美化**，不值得为其引入功能性光标倒退，故二者维持 Deferred（更新理由：redo-免疫
无需 patch，但删+插的光标正确需要 patch `edit_insert_text`）。

**本 change 只落地光标正确的那部分：Tab 网格吸附（ceil 到下制表位）。** Tab 用 `Cmd::Insert(1, spaces)`
补 `(next_stop - col)` 个空格——`Insert` 推进光标（正确）、不污染 kill-ring、redo-免疫（文本在 payload）。
效果：`col=2` 按 Tab → `col=4`（对齐到制表位），而非 #253 的 `col=6`（恒加一级）。

## What Changes

- **扩展 z42→Rust 动作串协议**（`repl_editing.rs::parse_action`）：新增 `insert:<text>` → `Cmd::Insert(1,text)`；
  移除已无人 emit 的 `indent`（Tab 改用 `insert:`）。退格的 `dedent` 不变。
- **`ReplEditing.KeyEdit` 的 Tab 分支**从「定量一级（`indent`）」改为「grid-snap ceil（`insert:<delta 空格>`）」。
  退格分支不变（仍 `dedent`）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/toolchain/scripting/src/ReplEditing.z42` | MODIFY | Tab 分支改 grid-snap ceil（`insert:` + `_spaces`）；头注更新 |
| `src/runtime/src/corelib/repl_editing.rs` | MODIFY | `parse_action` 加 `insert:`、删 `indent`；协议注释更新 |
| `src/runtime/src/corelib/repl_editing_tests.rs` | MODIFY | `insert:` 单测；`indent` 改为 unknown |
| `src/toolchain/scripting/tests/repl_editing/driver.z42` | MODIFY | Tab grid-snap ceil 场景（含错位 col1/2/6）|
| `src/toolchain/scripting/tests/repl_editing/expected_output.txt` | MODIFY | 对应期望输出 |
| `src/runtime/src/corelib/repl.rs` | MODIFY | 键绑定注释更新（Tab 用 Insert；无新绑定）|
| `docs/roadmap.md` | MODIFY | Deferred Backlog Index：更新 grid-snap（退格 floor）/ rbrace 两行的理由为「光标 patch」；新增 tab-ceil 完成登记（如涉及）|
| `docs/book/src/toolchain/repl.md` | MODIFY | 缩进键位机制补 Tab grid-snap-ceil + Replace 光标局限说明（若无对应节则新增）|
| `docs/spec/changes/add-repl-tab-grid-snap/*` | NEW | proposal/design/specs/tasks |

**只读引用**：

- `docs/spec/archive/2026-08-22-add-repl-indent-editing/design.md` — #253 的动作协议与 Deferred 定义
- `src/toolchain/interactive/core/interactive_main.z42` — REPL 逐行累积循环（本 change 不改）

## Out of Scope（维持 Deferred，见 design Deferred 段）

- **退格 floor 到前制表位**（深错位 grid-snap）：需 `Replace(WholeLine)`，光标归位行首。
- **`}` 自动回退一级**：同上，且破坏 `} else {`。
- ③ 多行整块编辑（PR-B）；validate-while-typing。

## Open Questions

- 无（Tab-ceil 光标正确已 PTY spike 实测：`col=2` + Tab → 4 空格、光标停在末尾）。
