# Spec: REPL 缩进感知行编辑

缩进一级 = `indent_size`（在 `read_one_line` 设为 4，匹配脚本层续行缩进）。「当前逻辑行光标前」= 从当前逻辑行行首（最后一个 `\n` 之后，或 pos 之前无 `\n` 时即 line 起始）到 `pos` 的子串；令 `col` = 该子串长度。z42 `KeyEdit` 仅当该前缀**全为空格**时才介入。

> **实现约束（见 design.md D1/D6）**：rustyline 对自定义绑定返回的可重复命令执行 `redo(Some(n))`，n=数字前缀（默认 1），会覆盖 movement 计数——故「删/插变量个空格」无法 redo-免疫地实现。唯一 redo-免疫的成级缩进原语是 `Cmd::Indent`/`Dedent(WholeLine)`（按 `indent_size` 定量）。因此缩进量固定为**一级**，非「网格吸附」。

## ADDED Requirements

### Requirement: 退格删一级缩进

#### Scenario: 光标前全为空格
- **WHEN** 当前行光标前全是空格且 `col > 0`
- **THEN** `KeyEdit` 返回 `"dedent"` → `Cmd::Dedent(WholeLine)` 去掉一级缩进（行首最多 `indent_size` 个空格）

#### Scenario: 光标前有非空白字符 / 行首
- **WHEN** 当前行光标前含任何非空格字符，或 `col == 0`
- **THEN** `KeyEdit` 返回 `""` → 默认单字符退格

### Requirement: Tab 加一级缩进

#### Scenario: 纯空白前缀按 Tab
- **WHEN** 当前行光标前为空或全为空格（无词可补全）
- **THEN** `KeyEdit` 返回 `"indent"` → `Cmd::Indent(WholeLine)` 加一级缩进

#### Scenario: 有词前缀按 Tab
- **WHEN** 当前行光标前以标识符字符（字母/数字/`_` 等非空格）结尾
- **THEN** `KeyEdit` 返回 `""` → 默认补全（落回 rustyline 补全绑定）

### Requirement: 策略在 z42、Rust 只做适配

#### Scenario: 决策逻辑落脚本层
- **WHEN** 受控键（Backspace / Tab）被按下
- **THEN** Rust handler 取 `line`/`pos`，回调 z42 `KeyEdit(key, line, pos)`，按返回的动作串构造 rustyline `Cmd`；所有触发条件在 z42 `KeyEdit` 内，Rust 不含判断

#### Scenario: 动作串协议
- **WHEN** `KeyEdit` 返回动作串
- **THEN** Rust 按下表翻译（仅 redo-免疫命令）：
  - `""` → 该键默认行为（`None`）
  - `"indent"` → `Cmd::Indent(Movement::WholeLine)`
  - `"dedent"` → `Cmd::Dedent(Movement::WholeLine)`

### Requirement: 平台与回退

#### Scenario: 非 rustyline 路径不受影响
- **WHEN** 运行于 wasm32 或无 tty（plain stdin 回退）
- **THEN** 不绑定任何缩进键位，行为与本变更前一致；`__repl_set_key_editor` builtin 仍解析（仅存 FQN，不影响）

## Pipeline Steps

不涉及编译 pipeline（Lexer/Parser/…）。仅 REPL 行编辑运行时行为。

## IR Mapping

无。不新增 IR 指令、不改 zbc/zpkg 格式。
