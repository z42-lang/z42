# Spec: REPL 输入完整性判定（parser 权威）

## ADDED Requirements

### Requirement: parser 报告 `IncompleteAtEof` 可恢复不完整信号

parser 在解析过程中，当**期待某个 token 但当前 token 已是 EOF** 时，除照常报诊断外，还在诊断袋（`DiagnosticBag`）上置 `IncompleteAtEof = true`，表示"不是写错了，而是没写完"。

#### Scenario: 类型声明缺正文
- **WHEN** parse `class B`（无 `{`，token 流以 EOF 结束）
- **THEN** `DiagnosticBag.IncompleteAtEof == true`
- **AND** `class B {`（`{` 已出现但未闭合）也 `IncompleteAtEof == true`（缺 `}` 于 EOF）

#### Scenario: 自由函数缺正文
- **WHEN** parse `void foo()`（参数括号配平，但无 `{` 也无 `;`，以 EOF 结束）
- **THEN** `IncompleteAtEof == true`（`_expectSemi` 在 EOF 处触发）

#### Scenario: 表达式缺操作数（悬挂运算符）
- **WHEN** parse 表达式语句 `1 +`（二元运算符后无右操作数，以 EOF 结束）
- **THEN** `IncompleteAtEof == true`（`ExprParser` 缺操作数 fallthrough 命中 EOF）

#### Scenario: 未闭合括号 / 语句块
- **WHEN** parse `f(1,`（缺参数与 `)`）或 `if (x > 0) {`（缺 `}`），以 EOF 结束
- **THEN** `IncompleteAtEof == true`

#### Scenario: 完整输入不置位
- **WHEN** parse 完整的 `class B {}` / `1 + 2` / `void foo() {}` / `void foo();`
- **THEN** `IncompleteAtEof == false`

#### Scenario: 真语法错误不误判为不完整
- **WHEN** parse `class 1`（class 后是非法 token，**首个错误发生在该 token 而非 EOF**；即使解析恢复后走到 EOF 也不算）
- **THEN** `IncompleteAtEof == false`（有诊断报错，但不是"没写完"——交给求值路径正常报错）
- **判据**：置位要求「当前 token 为 EOF **且** 此前无真语法错」（`!HasErrors()`），即首错就在 EOF

## MODIFIED Requirements

### Requirement: REPL 续读判定改由 parser 权威决定

**Before:** native `__repl_readblock` 用 `bracket_depth`（括号净深度 > 0）判续读；`class B` 等无括号但不完整的输入被判"完整"，直接送编译报错。

**After:** 续读判定由脚本层 `Std.Scripting.Completeness.IsIncomplete(src)` 决定——对**裸输入原文** parse（不包裹、不执行），返回 parser 的 `IncompleteAtEof`。native 只负责逐行读取与视觉缩进。

## ADDED Requirements

### Requirement: `Completeness.IsIncomplete` 探针

`Std.Scripting.Completeness.IsIncomplete(string src) -> bool`：判断累积输入是否"还没写完、需要续读"。

#### Scenario: 声明输入走 CompilationUnit 入口
- **WHEN** `src` 经 `Classifier.Classify` 判为 `IsDecl`（如 `class B` / `void foo()`）
- **THEN** 用 `Parser.ParseCompilationUnit()` parse 裸 `src`，返回其 `IncompleteAtEof`

#### Scenario: 表达式/语句输入走 Statement 入口
- **WHEN** `src` 非 `IsDecl`（如 `1 +` / `if (x) {` / `x = 5`）
- **THEN** 用 `Parser.ParseStatement()` parse 裸 `src`，返回其 `IncompleteAtEof`

#### Scenario: 表达式无分号视作完整（关键）
- **WHEN** `src` 是无分号的表达式/语句（`42` / `1 + 2` / `x = 5` / `foo()`），经 `ParseStatement` 缺 `;` 于 EOF
- **THEN** `IsIncomplete == false`——表达式/语句入口只看 `IncompleteAtEof`，忽略 `IncompleteSemiAtEof`（仅缺 `;`）
- **AND** 反例：声明入口 `void foo()` 缺 `;` → `IsIncomplete == true`（`ParseCompilationUnit` 取 `IncompleteAtEof || IncompleteSemiAtEof`）
- **动机**：若缺 `;` 也当续读，`42` 会无限吃后续输入、从不求值（实测回归）

#### Scenario: 不执行、不加载依赖
- **WHEN** 调用 `IsIncomplete`
- **THEN** 只 parse（不做语义分析 / codegen / 加载引用 / 执行），且不改变 `ScriptState`

### Requirement: REPL 多行累积循环（脚本层驱动）

#### Scenario: 不完整则续读、提示符切换、自动缩进
- **WHEN** 累积 buf 经 `IsIncomplete` 判为 true
- **THEN** 用续行提示符 `... ` 读下一行，并按 buf 的括号深度预填缩进（`ReadLineIndented`）
- **AND** 读到的行追加到 buf（`buf = buf + "\n" + line`）后重判

#### Scenario: 完整则求值
- **WHEN** buf 经 `IsIncomplete` 判为 false
- **THEN** 调 `Script.Eval(state, buf)`，打印结果/错误，清空 buf，回到主提示符 `>>> `

#### Scenario: 续行途中 Ctrl-C 放弃缓冲
- **WHEN** 多行续读途中用户按 Ctrl-C（Interrupted）
- **THEN** 丢弃当前 buf，回到主提示符 `>>> `（不退出 REPL、不求值残缺输入）

#### Scenario: Ctrl-D 退出
- **WHEN** 主提示符处（buf 为空）用户按 Ctrl-D（EOF）
- **THEN** 退出 REPL

### Requirement: `-c` 单次求值路径

#### Scenario: 一次性代码不完整视为语法错误
- **WHEN** `z42i -c "<code>"` 的 code 经 `IsIncomplete` 判为 true（无续读来源）
- **THEN** 当作语法错误处理并非零退出（不挂起等待输入）

## IR Mapping

无新 IR 指令 / 无 zbc·zpkg 格式变更。变更为 parser 诊断信号（`DiagnosticBag.IncompleteAtEof`，进程内、不序列化）。复用已定义未用的诊断码 `E0203 UnexpectedEof`。

## Pipeline Steps

受影响的 pipeline 阶段：
- [x] Lexer —— 不改（EOF token 已有）
- [x] Parser / AST —— 置 `IncompleteAtEof`（`_expect` / `ExprParser` 缺操作数 / `DeclParser` 名字位置）
- [ ] TypeChecker —— 不涉及
- [ ] IR Codegen —— 不涉及
- [ ] VM interp —— 不涉及（探针只 parse）
