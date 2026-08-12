# Spec: 用户自定义类型转换（User-Defined Conversions）

## ADDED Requirements

### Requirement: 声明 conversion operator

#### Scenario: 声明 implicit conversion
- **WHEN** 类体内写 `public static implicit operator Target(Source s) { ... }`
- **THEN** 解析为一个静态方法，方法名 `op_Implicit`，单参类型 `Source`，返回类型 `Target`，方法体照常绑定

#### Scenario: 声明 explicit conversion
- **WHEN** 类体内写 `public static explicit operator Target(Source s) { ... }`
- **THEN** 解析为静态方法名 `op_Explicit`，其余同上

#### Scenario: implicit / explicit 作为关键字
- **WHEN** 源码把 `implicit` 或 `explicit` 用作普通标识符（变量名/方法名）
- **THEN** 报语法错误（二者是保留关键字）

### Requirement: 隐式用户转换（赋值 / return / var-decl / 传参）

#### Scenario: 隐式转换在赋值处触发
- **WHEN** 存在 `implicit operator Target(Source)`，且 `Target t = sourceValue;`（sourceValue 静态类型 Source）
- **THEN** 编译通过，运行期 `t` 为 `op_Implicit(sourceValue)` 的结果（lower 成静态 `BoundCall`）

#### Scenario: 隐式转换在 return 处触发
- **WHEN** 函数返回类型 Target，`return sourceValue;`（Source 有 implicit→Target）
- **THEN** 编译通过，返回值为 `op_Implicit(sourceValue)`

#### Scenario: explicit-only 不在隐式上下文触发
- **WHEN** 只有 `explicit operator Target(Source)`（无 implicit），写 `Target t = sourceValue;`
- **THEN** 报 E0439「存在显式转换，是否漏了 cast？」（不隐式放行）

### Requirement: 显式用户转换 `(T)x`

#### Scenario: `(T)x` 触发 explicit conversion
- **WHEN** 存在 `explicit operator Target(Source)`，写 `Target t = (Target)sourceValue;`
- **THEN** 编译通过，`t` = `op_Explicit(sourceValue)`

#### Scenario: `(T)x` 也可触发 implicit conversion
- **WHEN** 存在 `implicit operator Target(Source)`，写 `(Target)sourceValue`
- **THEN** 编译通过，走 `op_Implicit`（显式 cast 接受 implicit 与 explicit 两类）

#### Scenario: `(UserType)identifier` 解析为 cast
- **WHEN** 写 `(Foo)bar`（Foo 是用户类型名，bar 是标识符/成员/调用表达式）
- **THEN** 解析为 `CastExpr(Foo, bar)`，而非分组表达式

### Requirement: RegKey 按 (源, 目标) 唯一

#### Scenario: 同源不同目标的转换不撞键
- **WHEN** 一个类同时声明 `implicit operator int(Foo)` 与 `implicit operator string(Foo)`
- **THEN** 两者注册为**不同** RegKey（键含返回类型），各自可独立解析与发射，互不覆盖

### Requirement: ② 声明期冲突检测

#### Scenario: 重复的 (源→目标) 转换
- **WHEN** 一个类声明两个 `implicit operator Target(Source)`（同源同目标）
- **THEN** 在**声明处**报 E0440「conversion operator 'Source'→'Target' 已声明」

#### Scenario: implicit 与 explicit 同 (源→目标)
- **WHEN** 一个类同时声明 `implicit operator Target(Source)` 与 `explicit operator Target(Source)`
- **THEN** 在声明处报 E0440「不能同时声明 implicit 与 explicit 'Source'→'Target'」

### Requirement: ③ 走中间类型诊断

#### Scenario: A→C 不存在但 A→B→C 存在
- **WHEN** `(C)aValue` 或隐式转换 A→C 失败，且存在 A→B 与 B→C 两个用户转换
- **THEN** 错误信息追加提示「可经中间类型 'B' 转换：写 `(C)(B)x`」

## MODIFIED Requirements

### Requirement: `(UserType)x` 解析

**Before:** 用户类的 `(C)x` 故意不解析为 cast（要求写 `x as C`，避免与分组表达式歧义）。
**After:** `(UserType)operand` 解析为 `CastExpr`（operand 为标识符/字面量/成员/调用等无歧义起始）；
`(UserType)(paren)` 与 `(UserType) - x` 仍按分组/二元处理（歧义，用 `as` 或临时变量绕）。

### Requirement: `Conversion.Classify` 用户转换回退

**Before:** `UserImplicit`/`UserExplicit` 两种类预留但 `Classify` 永不返回它们（PR1/PR2 恒走内建分支）。
**After:** 内建分支得出 `None` 时回退查 from/to 类型上的 op_Implicit/op_Explicit（精确 (源,目标) 匹配），
命中返回 `ConvResult{UserImplicit|UserExplicit, Method}`；无内建转换存在时才查（内建优先，镜像 C#）。

## IR Mapping

- 用户转换**不引入新 IR 指令 / 不 bump zbc·zpkg 格式**。
- lowering：`op_Implicit`/`op_Explicit` 调用 → 既有 `BoundCall("static", ...)` → Call opcode（同运算符重载 op_Add 脱糖）。
- RegKey：转换运算符 = `MangleKey(name, paramTypes, 1) + "$to$" + Canon(retType)`（保证 (源,目标) 唯一）。

## Pipeline Steps

- [x] Lexer（`implicit`/`explicit` 关键字）
- [x] Parser / AST（conversion operator 成员解析 + `(T)x` 消歧）
- [x] TypeChecker（Classify 用户支 + 声明期冲突检测 + 中间类型诊断）
- [x] IR Codegen（lower 成 BoundCall；RegKey 消歧）
- [x] VM interp（复用既有 Call；无 VM 改动）
