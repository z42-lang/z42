# Spec: enum 作为独立类型

## MODIFIED Requirements

### Requirement: enum 成员引用的静态类型是其 enum 类型

**Before:** `E.Member` 绑成 `BoundLitInt`，静态类型 `long`（`MemberResolver.z42:36-46`，enum-as-int 模型）。

**After:** 静态类型为 `E`。运行期表示不变（仍 i64）。

#### Scenario: enum 变量声明（**今天编不过，本变更的核心兑现**）
- **WHEN** `public enum Color { Red, Blue }` 且 `Color c = Color.Blue;`
- **THEN** **无诊断**
- **AND** 今天的行为是 `E0402: cannot assign long to Color (var-decl)`

#### Scenario: enum 传 enum 形参
- **WHEN** `void TakeC(Color c) {}` 且调用 `TakeC(Color.Blue)`
- **THEN** **无诊断**（今天报 `E0402 ... (argument)`）

#### Scenario: enum 字段 / 返回值
- **WHEN** `class S { Color c; }`、`Color pick() { return Color.Red; }`
- **THEN** **无诊断**

#### Scenario: 跨包 enum
- **WHEN** z42c 调 z42.core 的 `GCHandle.Alloc(target, GCHandleType.Weak)`
- **THEN** **无诊断**
- **AND** `add-argument-type-check` 在 `OverloadBinder._checkOneArg` 留的 `_isEnumSide` 跳过**已被摘除**

### Requirement: enum ↔ 底层整数为**显式**转换

**Before:** enum 成员即 `long`，与整数自由互换（`Direction.North == 0` 成立）。

**After:** 新增 `ConvKind.ExplicitEnum` —— **不在** `ImplicitOk` 白名单、**在** `Exists()` 内
⇒ 隐式上下文报 **E0439**（"an explicit conversion exists (are you missing a cast?)"）。

#### Scenario: enum → 整数缺 cast
- **WHEN** `long n = Color.Red;`
- **THEN** 报 **E0439**（不是 E0402）

#### Scenario: 整数 → enum 缺 cast
- **WHEN** `Color c = 0;`
- **THEN** 报 **E0439**

#### Scenario: 显式 cast 双向放行
- **WHEN** `long n = (long)Color.Red;` / `Color c = (Color)0;`
- **THEN** **无诊断**

#### Scenario: enum 与整数直接比较需 cast
- **WHEN** `Direction.North == 0`
- **THEN** 报 **E0439**
- **AND** `(long)Direction.North == 0` **无诊断**
- **AND** 这会让 `src/tests/types/enum.z42:43-44` 变红 —— **必须按新语义改写，不得保留旧断言**

### Requirement: enum 的比较运算

#### Scenario: 同 enum 相等比较
- **WHEN** `Direction.South == Direction.North`
- **THEN** **无诊断**，结果 `false`

#### Scenario: enum 关系比较（关系模式依赖）
- **WHEN** `examples/patterns.z42:65` 的 `>= HttpStatus.BadRequest and < HttpStatus.ServerError`
- **THEN** **无诊断**（C# 亦允许 enum 关系比较）

#### Scenario: 不同 enum 类型比较
- **WHEN** `Color.Red == Direction.North`
- **THEN** 报诊断（类型不符）

### Requirement: switch / 模式匹配不回归

#### Scenario: enum switch 表达式
- **WHEN** `c switch { Color.Red => 1, Color.Green => 2, Color.Blue => 3 }`（`c: Color`）
- **THEN** **无诊断**，穷尽性检查结论与今天一致

#### Scenario: 穷尽性缺分支仍报
- **WHEN** 少一个 enum 分支且无 `_`
- **THEN** 仍报穷尽性诊断（`ExhaustCheck` 不回归）

### Requirement: 运行期表示不变

#### Scenario: enum 值仍是底层整数
- **WHEN** golden 用例打印 `(long)Status.NotFound`
- **THEN** 输出 `404`
- **AND** `.zbc` 里 enum 值仍以 i64 承载

#### Scenario: enum 反射身份
- **WHEN** `Color.Red.GetType()`
- **THEN** 折叠回 `typeof(Color)`（沿用既有 `EnumTypeName` 机制，此时类型已知可简化）

## Pipeline Steps

- [ ] Lexer —— 不涉及
- [ ] Parser / AST —— 不涉及
- [x] TypeChecker —— `MemberResolver` / `SymbolTable` / `Conversion` / `Z42Type` / `BinaryTypeTable` / `PatternBinder` / `OverloadBinder`
- [x] IR Codegen —— `ExprEmitter`：enum 静态类型仍发 i64；装箱按 i64
- [ ] VM interp —— 不涉及（表示不变）

## IR Mapping

**无新增 IR 指令、无 zbc / zpkg 格式变更**——enum 值的 wire 表示与今天逐位相同。

> ⚠️ 但**签名键**须实证：`OverloadResolver.TypeKey` 走 `Canon(t.Name())`，enum 名不变 ⇒ 键预期不变。
> 只能靠自举字节对账确认（design D1），不得靠推理。
