# Spec: const 关键字（编译期常量）

## ADDED Requirements

### Requirement: const 静态常量字段声明

`const <type> <name> = <const-expr>;` 作为类成员声明一个编译期常量字段：隐式 static、无实例存储、
每处引用替换为常量值。

#### Scenario: 字面量初始化的 const 字段
- **WHEN** `class C { const int Max = 100; } … C.Max` 被引用
- **THEN** 引用处 emit `ConstI64Instr 100`（而非 `StaticGetInstr "C.Max"`），无该字段的实例/静态存储

#### Scenario: const 字段引用另一已定义 const 字段
- **WHEN** `class C { const int A = 10; const int B = A * 2; }`
- **THEN** `B` 求值为 20；`C.B` 引用 emit `ConstI64Instr 20`

#### Scenario: 各原始类型 const 字段
- **WHEN** const 字段类型分别为 `int` / `bool` / `char` / `double` / `string`
- **THEN** 引用分别 emit `ConstI64Instr` / `ConstBoolInstr` / `ConstCharInstr` / `ConstF64Instr` / `ConstStrInstr`

### Requirement: const 局部常量声明

方法体内 `const <type> <name> = <const-expr>;` 声明一个块作用域内可见的编译期常量。

#### Scenario: 局部 const 引用替换
- **WHEN** `const int n = 8; … use n`
- **THEN** const 声明不发射存储 / 赋值；`n` 的每处引用 emit `ConstI64Instr 8`

#### Scenario: 局部 const 参与常量折叠
- **WHEN** `const int n = 8; int y = n * 4;`
- **THEN** `n * 4` 经字面量替换 + ConstFold 折成 `ConstI64Instr 32`

### Requirement: 常量传播喂 ConstFold

const 引用替换为字面量后，现有 ConstFold pass 对其后续算术/比较继续折叠。

#### Scenario: const 折进循环边界
- **WHEN** `const int N = 3; for (int i=0;i<N;i=i+1) …`
- **THEN** `i < N` 中 `N` 替换为 `ConstI64Instr 3`；比较由 ConstFold 依已知常量参与折叠

### Requirement: 常量条件死分支消除（dead-branch pass）

当 `br.cond` 的条件寄存器由 `ConstBoolInstr` 产出（常量 bool 条件）时，折成无条件 `br` 到命中分支；
`ExcCount==0` 的函数进一步移除因此不可达的块。

#### Scenario: 恒假条件整块消除
- **WHEN** `const bool Debug = false; if (Debug) { heavy(); }`（函数无异常表）
- **THEN** `br.cond r_false, then, end` 折成 `br end`；`then` 块（含 `heavy()`）不可达 → 移除

#### Scenario: 恒真条件保留 then、消除 else
- **WHEN** `const bool On = true; if (On) { a(); } else { b(); }`（无异常表）
- **THEN** 折成 `br then`；`else`（含 `b()`）不可达 → 移除

#### Scenario: 有异常表的函数只折不移块（CFG 铁律）
- **WHEN** 同上但函数 `ExcCount>0`（含 try/catch）
- **THEN** `br.cond` 仍折成 `br`；**不移除任何块**（异常隐式边不在终结子 CFG，移块不安全）

#### Scenario: 非常量条件不动
- **WHEN** `br.cond` 的条件寄存器不是 `ConstBoolInstr` 产出
- **THEN** dead-branch pass 不改该终结子

### Requirement: const 强制诊断

#### Scenario: 非常量初始化器
- **WHEN** `const int x = foo();`（初始化器非编译期常量）
- **THEN** 报诊断 E04xx「const 初始化器必须是编译期常量」

#### Scenario: const 缺初始化器
- **WHEN** `const int x;`
- **THEN** 报诊断 E04xx「const 声明必须有初始化器」

#### Scenario: 对 const 赋值
- **WHEN** 对 const 字段 / 局部 const 目标赋值 `C.Max = 5;` 或 `n = 3;`
- **THEN** 报诊断 E04xx「不可对 const 赋值」（复用 readonly 赋值检查邻近路径，避免双诊断）

#### Scenario: 引用未定义 / 非 const 符号作常量表达式
- **WHEN** `const int Y = Z * 2;` 其中 `Z` 非 const（普通字段 / 变量 / 未定义）
- **THEN** 报诊断 E04xx「常量表达式只能引用已定义的 const」

## IR Mapping

| 语法 | IR |
|------|----|
| const 字段 / 局部声明 | **无 IR**（不 emit 字段 / 局部存储；仅登记 ConstVal） |
| const 引用（int/bool/char/float/string/null） | `ConstI64Instr` / `ConstBoolInstr` / `ConstCharInstr` / `ConstF64Instr` / `ConstStrInstr` / `ConstNullInstr` |
| `br.cond <const-bool>, A, B`（dead-branch） | → `br <taken>`（+ 可达性移除不可达块，ExcCount==0） |

**无 zbc / zpkg 格式 bump**：const 在 codegen 阶段全部替换为既有字面量指令，序列化字节即字面量；
dead-branch 是 ZbcWriter 前的纯 IR 变换。（同 readonly #124 的"编译期优化提示"性质。）

## Pipeline Steps

- [x] Lexer（`const` 关键字）
- [x] Parser / AST（字段修饰符 `Mods`；`VarDeclStmt.IsConst`）
- [x] TypeChecker（ConstEval 求值 + 强制诊断 + const 符号登记）
- [x] IR Codegen（引用 → 字面量替换；const 声明不 emit 存储）
- [x] 优化管线（现有 ConstFold 接管折叠；新增 dead-branch pass）
- [x] VM interp（无新指令；字面量 + br 已支持）
