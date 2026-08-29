# Spec: REPL `}` 自动回退 + 退格 floor

## ADDED Requirements

### Requirement: `}` 在纯缩进行自动回退一级

#### Scenario: 已对齐缩进键入 `}`
- **WHEN** 当前逻辑行整行纯空白、光标前 `col=8`（indent=4），键入 `}`
- **THEN** `ReplEditing.KeyEdit("rbrace", line, pos)` 返回 `"replace:    }"`（4 空格 + `}`）
- **AND** Rust 译成 `Cmd::Replace(WholeLine, Some("    }"))`，patch 后光标落在 `}` 之后（可继续输入 ` else {`）

#### Scenario: 已在一级缩进键入 `}`
- **WHEN** 整行纯空白、`col=4`，键入 `}`
- **THEN** 返回 `"replace:}"`（0 缩进 + `}`）

#### Scenario: 顶格键入 `}`
- **WHEN** 整行纯空白、`col=0`，键入 `}`
- **THEN** 返回 `"replace:}"`（不回退到负缩进）

#### Scenario: 行内已有内容时键入 `}`（不介入）
- **WHEN** 当前逻辑行非纯空白（含词或符号），键入 `}`
- **THEN** 返回 `""` → Rust `None` → 默认插入 `}`（不 dedent、不动光标）

### Requirement: 退格在纯缩进行 floor 到前制表位

#### Scenario: 错位缩进退格归正
- **WHEN** 整行纯空白、光标在行尾、`col=6`，按退格
- **THEN** 返回 `"replace:    "`（4 空格；删 2，floor 到前制表位）
- **AND** 光标落在 4 空格之后

#### Scenario: 对齐缩进退格（与旧 Dedent 等效）
- **WHEN** 整行纯空白、`col=8`，按退格
- **THEN** 返回 `"replace:    "`（删 4 到一级）

#### Scenario: 一级缩进退格到顶格
- **WHEN** 整行纯空白、`col=3`，按退格
- **THEN** 返回 `"replace:"`（floor 到 0）

#### Scenario: 前缀空白但光标后有内容（保留旧行为）
- **WHEN** 光标前纯空白，但同逻辑行光标后仍有内容（非整行空白）
- **THEN** 返回 `"dedent"`（`Dedent(WholeLine)` 删一级、不碰光标后内容）

### Requirement: `replace:` 动作与 rustyline 光标推进

#### Scenario: parse_action 翻译 replace
- **WHEN** `parse_action("replace:    }")`
- **THEN** 得 `Cmd::Replace(Movement::WholeLine, Some("    }"))`

#### Scenario: 空替换文本
- **WHEN** `parse_action("replace:")`
- **THEN** 得 `Cmd::Replace(Movement::WholeLine, Some(""))`

#### Scenario: edit_insert_text 推进光标（fork patch）
- **WHEN** 执行 `Cmd::Replace(WholeLine, Some(text))`，`edit_kill(WholeLine)` 后光标在逻辑行首
- **THEN** `edit_insert_text(text)` 插入后光标推进到 `行首 + text.len()`（文本末尾），而非停在行首

## IR Mapping

无新 IR 指令 / zbc·zpkg 格式变更（纯 VM 行编辑行为 + 策略）。

## Pipeline Steps

- [ ] Lexer — 无
- [ ] Parser / AST — 无
- [ ] TypeChecker — 无
- [ ] IR Codegen — 无
- [x] VM interp — rustyline 适配壳 `parse_action` + `}` 键绑定 + rustyline fork patch
- [x] 策略层（z42）— `ReplEditing.KeyEdit` 的 `rbrace` / backspace-floor 分支
