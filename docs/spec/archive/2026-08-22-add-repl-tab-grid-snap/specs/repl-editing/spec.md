# Spec: REPL Tab 缩进网格吸附

## MODIFIED Requirements

### Requirement: Tab 缩进（grid-snap ceil 升级）

**Before（#253）：** 全空白前缀 Tab 恒发 `indent`（加 `indent_size`=4，`col=2`→6）。
**After：** 全空白前缀 Tab ceil 到下制表位，发 `insert:<(next_stop-col) 个空格>`（`col=2`→4）。
`next_stop = ((col/4)+1)*4`。

#### Scenario: 对齐 Tab（col 为 4 倍数，含 col0）
- **WHEN** `KeyEdit("tab", "", 0)`（col=0）
- **THEN** 返回 `insert:    `（4 空格；→ col 4）
- **WHEN** `KeyEdit("tab", "    ", 4)`（col=4）
- **THEN** 返回 `insert:    `（4 空格；→ col 8）

#### Scenario: 错位 Tab（ceil 到制表位）
- **WHEN** `KeyEdit("tab", "  ", 2)`（col=2）
- **THEN** 返回 `insert:  `（2 空格；ceil 2→4，而非 #253 的 →6）
- **WHEN** `KeyEdit("tab", "      ", 6)`（col=6）
- **THEN** 返回 `insert:  `（2 空格；ceil 6→8）
- **WHEN** `KeyEdit("tab", " ", 1)`（col=1）
- **THEN** 返回 `insert:   `（3 空格；ceil 1→4）

#### Scenario: 有词 Tab（落回补全）
- **WHEN** `KeyEdit("tab", "  foo", 5)` 或 `KeyEdit("tab", "foo", 3)`
- **THEN** 返回 `""`（默认 → Tab 补全）

## UNCHANGED（回归保护）：退格

退格行为不变（#253）：全空白前缀非空 → `dedent`；否则 `""`。

#### Scenario: 退格去一级
- **WHEN** `KeyEdit("backspace", "        ", 8)`（col=8）或 `KeyEdit("backspace", "      ", 6)`（col=6）
- **THEN** 返回 `dedent`（错位也只去一级，不 floor——见 Deferred）

#### Scenario: 退格非空白 / col0
- **WHEN** `KeyEdit("backspace", "  x", 3)` 或 `KeyEdit("backspace", "", 0)`
- **THEN** 返回 `""`（默认删 1 字符）

## Rust 侧行为（parse_action → Cmd）

#### Scenario: 动作串翻译
- **WHEN** `parse_action("dedent")`
- **THEN** `Some(Cmd::Dedent(Movement::WholeLine))`
- **WHEN** `parse_action("insert:  ")`
- **THEN** `Some(Cmd::Insert(1, "  "))`
- **WHEN** `parse_action("")` / `parse_action("indent")` / `parse_action("replace:  ")` / 未知
- **THEN** `None`（`indent` 已删；`replace:` 故意不处理——对应 Deferred 的 `}`/退格 floor）

## Pipeline Steps

不涉及编译器 pipeline（纯 REPL 键位策略 + Rust 适配）。受影响：
- [ ] z42 `ReplEditing.KeyEdit`（Tab 分支）
- [ ] Rust `parse_action`（`insert:` 翻译）
