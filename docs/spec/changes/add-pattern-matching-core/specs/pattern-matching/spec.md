# Spec: Pattern Matching Core (A1)

统一模式文法接入三位点（`switch` 语句、`switch` 表达式、`is` 表达式）。本 spec 定义 A1 覆盖的模式形态与
可观察行为。绑定用 Rust 裸标识符（无 `var`）；常量/枚举用限定名或字面量。

## ADDED Requirements

### Requirement: 通配模式 `_`

`_` 匹配任意值、不引入绑定。

#### Scenario: 通配总是匹配
- **GIVEN** `switch (x) { case _: r = 1; }`（x 任意值）
- **THEN** 命中该 case，`r == 1`

#### Scenario: 通配不绑定
- **GIVEN** case 体内引用 `_`
- **THEN** 编译错误（`_` 非绑定名）

### Requirement: 常量模式（byte-identical 保持现状）

字面量（int/float/string/char/bool/null）与限定枚举/常量名（`Color.Red`）按值相等匹配。

#### Scenario: 字面量匹配
- **GIVEN** `x switch { 1 => "a", 2 => "b", _ => "z" }`，`x == 2`
- **THEN** 结果为 `"b"`

#### Scenario: null 常量匹配
- **GIVEN** `obj switch { null => "nil", _ => "some" }`，`obj == null`
- **THEN** 结果为 `"nil"`

#### Scenario: 限定枚举名匹配
- **GIVEN** `c switch { Color.Red => 1, Color.Blue => 2, _ => 0 }`，`c == Color.Blue`
- **THEN** 结果为 `2`

#### Scenario: 现有常量 switch 发码不变
- **GIVEN** z42c/stdlib 源中现有的常量 `switch`
- **THEN** 新 z42c 对其发码与旧 z42c **逐字节相同**（自举不动点 gen1==gen2）

### Requirement: 类型模式 `T` / `T x`

`T` 测试运行时类型为 `T`；`T x` 额外把值以 `T` 绑定到 `x`（延续现有 `is` 语义）。

#### Scenario: 类型测试
- **GIVEN** `shape switch { Circle => "c", Square => "s", _ => "?" }`，`shape` 运行时为 `Square`
- **THEN** 结果为 `"s"`

#### Scenario: 类型 + 绑定
- **GIVEN** `case Circle c:`，`shape` 为 `Circle`
- **THEN** 命中，`c` 在 body 内以 `Circle` 类型可见

### Requirement: 位置模式（record 解构）

`T(p0, p1, ...)`：`T` 必须是 record；先测 `IsInstance(subj, T)`，再对**主构造器声明序**的第 i 个字段递归匹配
子模式 `pi`。子模式数须等于 record 字段数。

#### Scenario: record 位置解构 + 绑定
- **GIVEN** `[Record] class Point(int X, int Y)`；`p == new Point(3, 4)`；`case Point(x, y):`
- **THEN** 命中，`x == 3`、`y == 4`

#### Scenario: 位置模式内嵌常量
- **GIVEN** `p == new Point(0, 7)`；`case Point(0, y):`
- **THEN** 命中，`y == 7`

#### Scenario: 位置模式常量不匹配则落下一 arm
- **GIVEN** `p == new Point(5, 7)`；`Point(0, y) => ... , Point(x, y) => "other"`
- **THEN** 结果为 `"other"`（第一 arm 因 `X != 0` 失败）

#### Scenario: 非 record 用位置模式报错
- **GIVEN** 普通 `class Foo`（非 `[Record]`）用 `case Foo(a, b):`
- **THEN** 编译错误（位置解构仅限 record）

#### Scenario: arity 不符报错
- **GIVEN** `Point` 有 2 字段，`case Point(a):`
- **THEN** 编译错误（子模式数 ≠ 字段数）

### Requirement: 属性模式 `T { F: p }` / `{ F: p }`

按字段名匹配零个或多个字段；类型部分可省（`{F:p}` 仅约束字段、不测类型）。

#### Scenario: 属性模式按名匹配
- **GIVEN** `p == new Point(0, 9)`；`case Point { X: 0, Y: y }:`
- **THEN** 命中，`y == 9`

#### Scenario: 省略类型的属性模式
- **GIVEN** `case { X: 0 }:`，subject 有字段 `X == 0`
- **THEN** 命中

### Requirement: 裸绑定模式（无 `var`）

单个裸标识符：解析命中类型名 → 类型模式（匹配任意该类型实例、不绑定）；否则 → 新绑定（匹配任意值并绑定）。

#### Scenario: 裸名作绑定
- **GIVEN** `x switch { n => n * 2 }`，`x == 5`（`n` 非类型名）
- **THEN** 结果为 `10`（`n` 绑定到 `5`）

#### Scenario: 裸名命中类型名作类型模式
- **GIVEN** `case Circle:`（`Circle` 是类型），subject 为 `Circle` 实例
- **THEN** 命中，无绑定引入

### Requirement: 嵌套模式

位置/属性模式的子模式可为任意模式，递归任意深度。

#### Scenario: 嵌套位置解构
- **GIVEN** `[Record] class Line(Point A, Point B)`；`l == new Line(new Point(1, 2), new Point(3, 4))`；`case Line(Point(x, _), _):`
- **THEN** 命中，`x == 1`

### Requirement: 守卫 `if`

模式匹配成功后评估布尔守卫；守卫为假则视为该 arm 不匹配、落下一 arm。守卫内可见该模式的绑定。

#### Scenario: 守卫为真命中
- **GIVEN** `case Point(x, y) if x == y:`，`p == new Point(2, 2)`
- **THEN** 命中

#### Scenario: 守卫为假落下一 arm
- **GIVEN** `Point(x, y) if x > y => "gt", Point(x, y) => "le"`，`p == new Point(1, 5)`
- **THEN** 结果为 `"le"`

### Requirement: 绑定作用域

模式绑定在对应 arm body / 守卫 / `is` 为真分支的 `TypeEnv` 可见；各 arm 独立作用域，互不泄漏。

#### Scenario: is-pattern 绑定在 true 分支可见
- **GIVEN** `if (p is Point(x, y)) { use(x, y); }`
- **THEN** `x`、`y` 在 then 分支可见；else/外层不可见

#### Scenario: arm 间绑定不泄漏
- **GIVEN** 两个 arm 各绑定同名 `n`
- **THEN** 各自 body 内 `n` 指本 arm 绑定，无冲突

## MODIFIED Requirements

### Requirement: `switch` case 模式

`switch` 的 `case` 由「仅常量表达式」扩为「完整模式 + 可选 `if` 守卫」；`default` 不变。常量 case 语义与发码
**保持现状不变**（byte-identical）。

### Requirement: `is` 表达式

`is` 由「类型 + 可选绑定」扩为「完整模式」。`x is T` / `x is T v` 语义与发码**保持现状不变**；`x is <richer
pattern>` 走新引擎，绑定在 true 分支可见。

## IR Mapping

| 模式 | 发射 IR（既有指令，无新增） |
|------|--------------------------|
| Wildcard / Binding | 无测试（恒真）；Binding 额外 `mov bindReg, subj` |
| Constant | `Eq(subj, const)`（**与现状一致**） |
| Type `T` / `T x` | `IsInstance(subj, T)`；bind 时命中后 `mov x, subj` |
| Positional `T(pi)` | `IsInstance(subj, T)` + 逐字段 `FieldGet [owner=T, field=fi] subj` + 子模式递归 + `BrCond` 短路 |
| Property `T{F:p}` | `IsInstance(subj, T)`（T 省则跳）+ `FieldGet [owner=T, field=F] subj` + 递归 |
| 守卫 | 匹配成功块内评估守卫 `BoundExpr` → `BrCond` body/next |

**约束**：字段读用**显式 owner 的直读 `FieldGet`**，禁 `as_cast subj→T` + `field_get`（jit 误编，见 design D4）。

## Pipeline Steps

1. **Parse**（`PatternParser._parsePattern`）：源 → `Pattern` AST（名字形状分流）。
2. **Bind**（`PatternBinder`）：`Pattern` → `BoundPattern`（类型解析、裸名歧义消解、record/arity 校验、绑定注册 `TypeEnv`）。
3. **Emit**（`PatternEmitter.Emit`）：`BoundPattern` → 既有 IR（递归 test+bind，短路 `BrCond`）。
4. **三位点外壳**：`StmtEmitter._emitSwitch` / `OperatorEmitter._emitSwitchExpr` / `TypeOpEmitter._emitIs` 编排 match→guard→body/result→end。
