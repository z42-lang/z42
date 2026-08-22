# Spec: REPL 整块多行编辑

## ADDED Requirements

### Requirement: 整块多行缓冲读取

一次 `Repl.ReadLine` 调用返回**一整条**（可能跨多物理行的）完整语句，而非单个物理行。
未写完时，编辑发生在同一个可编辑缓冲内，方向键可跨行导航。

#### Scenario: 单行完整语句立即提交
- **WHEN** 用户在主提示符输入 `1 + 1` 后按 Enter
- **THEN** `Completeness.IsIncomplete("1 + 1")` 为 false → 该 readline 提交，返回 `"1 + 1"`，求值输出 `2`

#### Scenario: 多行语句在缓冲内续读、写完才提交
- **WHEN** 用户输入 `class C {` 按 Enter（此时整块 = `class C {`，`IsIncomplete` 为 true）
- **THEN** 不提交；缓冲插入 `\n` + 一级缩进（`ContinuationIndent` = 4 空格），光标落缩进后，提示符转 `...`
- **WHEN** 用户继续输入 `int x;` 与 `}` 各按 Enter，直到整块 = `class C {\n    int x;\n}`（`IsIncomplete` 为 false）
- **THEN** 在闭合行按 Enter 提交整块，`Repl.ReadLine` 一次性返回完整的 `class C {\n    int x;\n}`

#### Scenario: 提交时机由整个缓冲的完整性决定（非物理行）
- **WHEN** 缓冲为 `foo(\n    1,\n    2` 且用户按 Enter
- **THEN** 整块 `IsIncomplete` 为 true（`(` 未闭合）→ 继续插行、不提交
- **WHEN** 用户补 `)` 使整块 = `foo(\n    1,\n    2)` 后按 Enter
- **THEN** `IsIncomplete` 为 false → 提交

### Requirement: 回车键的完整性判定与自动缩进

按下 Enter 时，把**整个缓冲**交 z42 判定：完整 → 接受（提交）；不完整 → 插入换行 + 续行缩进。
判定逻辑在 z42（`Completeness`），Rust 侧只翻译动作、不做语法决策。

#### Scenario: 不完整 → 插入换行并按未闭合层数缩进
- **WHEN** 整块 = `if (x) {`，用户按 Enter
- **THEN** z42 回车策略返回「newline:<indent>」，Rust 译为 `Cmd::Insert(1, "\n    ")`（1 级未闭合 → 4 空格），光标落缩进后

#### Scenario: 完整 → 接受提交
- **WHEN** 整块 = `if (x) { y(); }`，光标在末尾，用户按 Enter
- **THEN** z42 回车策略返回「accept」，Rust 译为 `Cmd::AcceptLine`，提交整块

#### Scenario: 缩进按未闭合括号层数（复用 ContinuationIndent）
- **WHEN** 整块 = `foo(\n    bar(` 用户按 Enter（2 级未闭合）
- **THEN** 插入 `\n` + 8 空格（2 × 4）

### Requirement: 粘贴与跨行编辑

多行文本作为一个缓冲进入，可回改任意行；不因粘贴产生重复缩进，不在粘贴中途误提交。

#### Scenario: 粘贴多行块后回改上面的行
- **WHEN** 用户粘贴 `class C {\n    int x;\n    int y;\n}`（bracketed paste，整体入单缓冲）
- **THEN** 缓冲为该 4 行、可编辑；用户方向键上移到 `int x;` 行改成 `int z;`，再到末尾按 Enter → 提交改后的整块

#### Scenario: 粘贴不触发逐行自动缩进叠加
- **WHEN** 粘贴的文本自身已带缩进
- **THEN** 不再叠加自动缩进（bracketed paste 内的换行不过回车策略），保持粘贴原样

### Requirement: 元指令与中断在整块模型下语义不回退

#### Scenario: `.` 元指令仍单行短路
- **WHEN** 用户在主提示符输入 `.help` 按 Enter
- **THEN** `.help` 单行完整（`IsIncomplete(".help")` 为 false 或首行 `.` 前缀短路）→ 立即作为元指令处理，不进入多行

#### Scenario: 多行编辑中 Ctrl-C 放弃整块回主提示符
- **WHEN** 用户在多行缓冲编辑中按 Ctrl-C
- **THEN** 该 readline 被中断，`Repl.ReadLine` 返回 null，主循环丢弃当前输入、回 `>>>` 主提示符（与旧「续读中 Ctrl-C 弃缓冲」语义等价）

#### Scenario: 主提示符 Ctrl-D 退出
- **WHEN** 缓冲为空时按 Ctrl-D
- **THEN** `Repl.ReadLine` 返回 null → 主循环退出 REPL

## MODIFIED Requirements

### Requirement: Repl.ReadLine 读取粒度

**Before:** `Repl.ReadLine(prompt, initial)` 读**一个物理行**，`initial` 预填续行缩进；多行累积与完整性
判定在 `interactive_main.z42` 脚本层用 `buf` + `IsIncomplete` 循环驱动。

**After:** `Repl.ReadLine(prompt)` 读**一整条完整语句**（内部可跨多物理行，rustyline 缓冲驱动）；完整性
判定下沉到回车键重入回调（每次 Enter 对整块判 `IsIncomplete`）；脚本层不再累积 `buf`、不再传 `initial`。

## Pipeline Steps

受影响（本 change 不碰编译 pipeline，仅交互运行时 + 脚本层）：
- [ ] VM interp（corelib：repl.rs / repl_editing.rs 重入回调 + 键绑定）
- [ ] stdlib 脚本层（ReplEditing.z42 回车策略 / Repl.z42 ReadLine 语义 / interactive_main.z42 循环塌缩）
- [ ] 测试（parse_action 单测 + 回车策略 golden + PTY e2e）

## IR Mapping

无。本 change 不新增 IR 指令 / 不改 zbc·zpkg 格式（纯交互运行时 + 脚本层行为）。
