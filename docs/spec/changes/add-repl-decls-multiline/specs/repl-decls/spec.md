# Spec: REPL 多行输入 + 顶层声明累积

## ADDED Requirements

### Requirement: 多行输入块

REPL 在一行未闭合括号（`()` / `[]` / `{}`）时继续读取续行，直到括号平衡，再作为单个输入求值。

#### Scenario: 跨行函数声明作为一个块读入
- **WHEN** 用户在 `>>>` 提示符下输入 `int add(int a, int b) {`（末尾 `{` 未闭合）
- **THEN** REPL 显示续行提示符 `... ` 并继续读取，直到用户补齐 `}`，随后把整个多行文本作为一次输入求值

#### Scenario: 单行输入不触发续读
- **WHEN** 用户输入 `1 + 2`（括号平衡）
- **THEN** REPL 立即求值该行，不进入续读

#### Scenario: 字符串/注释中的括号不计入平衡
- **WHEN** 用户输入 `Console.WriteLine("("）`（括号在字符串字面量内）
- **THEN** 该行视为平衡、立即求值（依赖 `__repl_readblock` 既有的字符串/注释感知平衡逻辑）

### Requirement: 顶层声明跨轮累积

用户在 REPL 中声明的自由函数与类型（`class` / `struct` / `record` / `enum` / `interface`），在后续轮次可被引用。

#### Scenario: 定义函数后调用
- **WHEN** 用户先输入 `int sq(int x) { return x * x; }`，随后输入 `sq(5)`
- **THEN** 第二轮求值结果为 `25`

#### Scenario: 定义类后实例化
- **WHEN** 用户先输入 `class Point { public int X; public Point(int x) { this.X = x; } }`，随后输入 `new Point(7).X`
- **THEN** 第二轮求值结果为 `7`

#### Scenario: 声明与既有变量 carry-forward 共存
- **WHEN** 会话中已有变量 `var n = 3`，其后定义 `int twice(int v) { return v * 2; }`，再输入 `twice(n)`
- **THEN** 结果为 `6`（声明累积不破坏变量 carry-forward，反之亦然）

### Requirement: 同名重定义报错且不破坏会话

MVP 不支持重定义；重名声明返回错误，会话状态不推进。

#### Scenario: 重名函数声明被拒绝
- **WHEN** 用户已定义 `int f() { return 1; }`，随后再输入 `int f() { return 2; }`
- **THEN** REPL 报错（提示符号 `f` 已定义、暂不支持重定义），且后续 `f()` 仍返回 `1`（原定义存活）

#### Scenario: 声明编译失败不推进会话
- **WHEN** 用户输入语法错误的声明 `class { }`（缺类名）
- **THEN** REPL 打印编译错误，会话变量/声明集不变，下一轮可正常继续

## MODIFIED Requirements

### Requirement: 输入分类

**Before:** `Script._classify` 只区分「变量声明（`var x =` / `T x =`）」与「表达式」；语句由 `_isStatement` 兜底。
**After:** 新增「顶层声明」类别——`class`/`struct`/`record`/`enum`/`interface` 关键字开头，或 `<type> <name> (`
自由函数形状（两标识符后紧跟 `(`，区别于 `<type> <name> =` 变量声明）。分类优先级：using → 顶层声明 →
变量声明 → 语句 → 表达式。

## IR Mapping

无新 IR 指令 / 无 zbc·zpkg 格式变更。声明经现有 `PackageCompile.Compile` 编译为普通 zpkg 成员，
经现成 `__load_bytecode_in_memory` 加载。

## Pipeline Steps

受影响阶段（均在 stdlib/toolchain 层，不改 VM/编译器源）：
- [x] 输入分类（`Script._classify`，token 级）
- [x] 每轮源组装（`Script.Eval` prelude + body）
- [x] 内存增量并入（`DepScan.ExtendWithPackage`，复用）
- [ ] Lexer / Parser / TypeChecker / IR Codegen / VM interp —— **不涉及**（复用现成）
