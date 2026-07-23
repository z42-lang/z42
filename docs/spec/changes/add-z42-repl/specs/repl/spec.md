# Spec: z42 交互式 REPL

## ADDED Requirements

### Requirement: 表达式即时求值

#### Scenario: 求值算术表达式
- **WHEN** 用户在 REPL 输入 `1 + 2`
- **THEN** 打印 `3`，会话状态不变

#### Scenario: 求值字符串/布尔
- **WHEN** 输入 `"a" + "b"` / `3 > 2`
- **THEN** 分别打印 `ab` / `true`

### Requirement: 变量跨行持久（Growing Transcript）

#### Scenario: 声明后引用
- **WHEN** 先输入 `var x = 5`，再输入 `var y = x * 2`，再输入 `x + y`
- **THEN** 第三行打印 `15`（`$ReplVars.x=5`、`$ReplVars.y=10` 均持久）

#### Scenario: 变量声明本身打印赋值结果
- **WHEN** 输入 `var x = 5`
- **THEN** 打印 `5`

### Requirement: 声明追加

#### Scenario: 函数声明后调用
- **WHEN** 输入 `int sq(int n) { return n*n; }`，再输入 `sq(4)`
- **THEN** 第二行打印 `16`

#### Scenario: using 追加
- **WHEN** 输入 `using Std.IO;`
- **THEN** 后续输入可用 `Std.IO` 下符号；`.usings` 列出该 using

### Requirement: 错误恢复不破坏会话

#### Scenario: 编译错误保留上一状态
- **WHEN** 已有 `var x = 5`，随后输入非法 `var y = ;`
- **THEN** 打印编译错误信息，`$ReplVars` 保持 `x=5`（`NextState = prevState`），随后 `x` 仍可求值为 `5`

#### Scenario: 运行时异常保留会话
- **WHEN** 输入会抛异常的表达式（如除零）
- **THEN** 打印 `RuntimeError: <message>`，会话状态保留，继续等待输入

### Requirement: 多行输入检测

#### Scenario: 未闭合括号续行
- **WHEN** 输入 `int sq(int n) {`（`{` 未闭合）
- **THEN** 显示续行提示符 `... ` 继续读取，直到括号平衡才整体求值

### Requirement: 单次求值模式

#### Scenario: -c 求值后退出
- **WHEN** 运行 `z42 repl -c "1 + 2"`
- **THEN** 输出 `3` 后进程退出（退出码 0），不进入交互循环

### Requirement: MVP 元指令

#### Scenario: .help 列指令
- **WHEN** 输入 `.help`
- **THEN** 按分组打印全部 MVP 指令

#### Scenario: .vars 列会话变量
- **WHEN** 已有 `var x = 5`，输入 `.vars`
- **THEN** 打印 `x : int = 5`

#### Scenario: .reset 清空会话
- **WHEN** 已有变量，输入 `.reset`，再输入之前的变量名
- **THEN** 报未定义（`$ReplVars` 归零）

#### Scenario: 未知元指令
- **WHEN** 输入 `.foo`
- **THEN** 打印 `unknown command '.foo'; try .help`，会话保留

#### Scenario: .exit / Ctrl-D 退出
- **WHEN** 输入 `.exit`（或 `.quit` 或 Ctrl-D）
- **THEN** REPL 退出

### Requirement: 行编辑（rustyline）

#### Scenario: 历史导航
- **WHEN** 求值若干行后按上方向键
- **THEN** 回填上一条输入，可编辑后重新提交

## IR Mapping
无新 IR 指令。新增两类 native builtin（`BuiltinInstr` 调用）：
- `__repl_readline` / `__repl_readblock` — rustyline 行输入。
- `__load_bytecode_in_memory` — 内存模块字节 → live VM 加载 + 返回句柄。

## Pipeline Steps
本 change 不改编译 pipeline（复用 `PackageCompile`）。受影响 pipeline 阶段：
- [ ] Lexer — 无
- [ ] Parser / AST — 无
- [ ] TypeChecker — 无
- [ ] IR Codegen — 无
- [x] VM interp — 新增 3 个 builtin
- [x] stdlib — 新库 z42.scripting
- [x] toolchain — z42.interactive 填充 + launcher 路由
