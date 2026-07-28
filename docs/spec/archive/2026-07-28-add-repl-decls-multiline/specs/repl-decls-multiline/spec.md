# Spec: REPL 多行输入 + 声明累积

## ADDED Requirements

### Requirement: 多行括号平衡输入

REPL 读入一个「括号平衡」的多行块而非单行；未闭合 `()[]{}` 时以续行提示符继续读，直到平衡或 EOF。

#### Scenario: 单行输入即时返回
- **WHEN** 用户输入 `1 + 1`（无未闭合括号）
- **THEN** 立即作为一个输入块交给 `Script.Eval`，求值打印 `2`

#### Scenario: 多行声明续读
- **WHEN** 用户输入首行 `int add(int a, int b) {`（未闭合 `{`）
- **THEN** REPL 显示续行提示 `... ` 继续读，直到用户输入 `}` 使括号平衡；整块 `int add(int a, int b) {\n  return a + b;\n}` 作为单个输入交给 `Script.Eval`

#### Scenario: 元指令不受影响
- **WHEN** 用户输入 `.help`（单行、无括号）
- **THEN** 立即返回并按元指令处理（不进入续读）

#### Scenario: EOF 退出
- **WHEN** 首行即 Ctrl-D（EOF）
- **THEN** `ReadBlock` 返回 null，REPL 退出（行为与原单行一致）

### Requirement: 自由函数声明累积

顶层自由函数声明被识别、编入本轮命名空间 `Repl.R{N}`、加载进会话，后续轮可裸调。

#### Scenario: 声明后裸调
- **WHEN** 第 N 轮输入 `int square(int x) { return x * x; }`，随后第 M 轮输入 `square(7)`
- **THEN** 第 N 轮不打印（声明无返回值、会话推进）；第 M 轮打印 `49`（裸调 `square` 经 `using Repl.R{N}` 跨包解析到声明 ns）

#### Scenario: 后声明引用先声明
- **WHEN** 先声明 `int dbl(int x) { return x + x; }`，再声明 `int quad(int x) { return dbl(dbl(x)); }`，随后 `quad(3)`
- **THEN** 打印 `12`（`quad` 体内裸调 `dbl` 经其所在 `Repl.R{N}` 的 `using` 解析）

#### Scenario: void 自由函数
- **WHEN** 声明 `void greet() { Console.WriteLine("hi"); }`（需先 `using Std.IO;`），随后 `greet()`
- **THEN** 声明轮不打印；调用轮打印 `hi`

### Requirement: 类型声明累积

顶层类型声明（`class` / `struct` / `record` / `interface` / `enum`）被识别、编入 `Repl.R{N}`，后续轮可实例化 / 使用。

#### Scenario: 类声明后实例化
- **WHEN** 声明 `class Point { public int x; public int y; }`，随后 `var p = new Point();`，随后 `p.x = 5; p.x`
- **THEN** 类声明轮不打印；后续轮正常构造并访问字段，最后打印 `5`

#### Scenario: enum 声明后使用
- **WHEN** 声明 `enum Color { Red, Green, Blue }`，随后 `Color.Green`
- **THEN** 声明轮不打印；使用轮求值成功

### Requirement: 重定义报错

同名函数/类型在同一会话中重复声明 → 报错，会话不推进（不 supersede）。

#### Scenario: 函数重定义
- **WHEN** 已声明 `int f() { return 1; }`，再次声明 `int f() { return 2; }`
- **THEN** 返回 `Success=false`，错误信息指明 `f` 已声明；会话状态不变（`f` 仍为原定义，后续 `f()` 返回 `1`）

#### Scenario: 类型重定义
- **WHEN** 已声明 `class Box {}`，再次声明 `class Box { public int v; }`
- **THEN** 返回 `Success=false`，错误信息指明 `Box` 已声明；会话不推进

### Requirement: 声明与既有输入类不冲突

声明累积不破坏既有 var carry-forward / 表达式 / 语句 / using 路径。

#### Scenario: 声明与变量共存
- **WHEN** `var n = 10;` 后声明 `int inc(int x) { return x + 1; }`，随后 `inc(n)`
- **THEN** 打印 `11`（`n` 经会话变量改写为 `Vars{K}.n`、`inc` 经声明 ns 裸调；两机制正交共存）

## MODIFIED Requirements

### Requirement: 输入分类

**Before:** `Script._classify` 只区分 var 声明（`var x=` / `T x=`）与「其余」（表达式/语句）。

**After:** 新增顶层声明识别（token 级，可选前导修饰符后）：
- 类型关键字 `class`/`struct`/`record`/`interface`/`enum` 开头 → 类型声明；声明名 = 关键字后标识符。
- `<type> <ident> (` 形（token2 为 `(`）→ 自由函数声明；声明名 = 该标识符。
- `<type> <ident> =` 形（token2 为 `=`）→ 仍为 var 声明（不变）。
- 其余 → 表达式/语句（不变）。

### Requirement: 跨包 enum 导入（一般能力）

任一包 `using` 另一包 → 后者的 `public` enum 类型 + 成员可解析（此前恒被排除）。enum 成员是 `long` 常量（既有语义不变）。

#### Scenario: 磁盘全量路径跨包 enum
- **WHEN** 包 A 声明 `enum Color { Red, Green, Blue }`，包 B `using A;` 后写 `long x = Color.Green;`
- **THEN** 编译通过，`x == 1`（此前 `undefined: Color`）

#### Scenario: enum 成员仍为 long
- **WHEN** 跨包访问 `Color.Blue`
- **THEN** 其类型为 `long`（值 2）；`Color c = Color.Blue`（enum 类型变量）仍非法，与同包 enum 语义一致

## Pipeline Steps

不加 IR 指令 / 不改 zbc·zpkg 格式（enum 元数据已在 zbc TYPE 段）。受影响：
- [ ] Lexer —— 仅**复用** `Z42.Syntax.Lexer` 做 token 级分类（不改 Lexer）
- [ ] Parser / AST —— 不改
- [ ] TypeChecker —— 不改（消费侧 `ImportedSymbolLoader`/`SymbolCollector` 已就绪）
- [ ] 依赖扫描 / 导入元数据 —— `z42c.pipeline/DepScan.ExtendWithPackage`（world-extension）+ `z42.ir/TsigReconcile._rebuildModule`（enum 导出）
- [ ] IR Codegen —— 不改
- [ ] VM interp —— 不改（`__repl_readblock` builtin 已存在）
