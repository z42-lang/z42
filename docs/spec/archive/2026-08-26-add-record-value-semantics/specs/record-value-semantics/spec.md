# Spec: Record Value Semantics

给 `[Record]` 类型（class & struct）合成 **值相等** + **记录式 ToString**，补齐 C# record 核心语义。

## ADDED Requirements

### Requirement: `[Record] class` 值相等

`[Record] class` 是引用类型，默认拿 `Std.Object` 的身份版 `Equals`/`GetHashCode`（按句柄）与
identity `==`。本变更为其合成 **member-wise 值相等**：逐字段比较，类型精确（type-exact）。

#### Scenario: 同值两实例值相等
- **WHEN** `[Record] class Point(int X, int Y)`，`var a = new Point(1, 2); var b = new Point(1, 2);`
- **THEN** `a.Equals(b)` == `true`，`a == b` == `true`，`a != b` == `false`

#### Scenario: 异值两实例不等
- **WHEN** `var a = new Point(1, 2); var b = new Point(1, 3);`
- **THEN** `a.Equals(b)` == `false`，`a == b` == `false`，`a != b` == `true`

#### Scenario: 同值同 hash
- **WHEN** `var a = new Point(1, 2); var b = new Point(1, 2);`
- **THEN** `a.GetHashCode() == b.GetHashCode()`

#### Scenario: 与 null 比较
- **WHEN** `Point a = new Point(1, 2);`
- **THEN** `a.Equals(null)` == `false`，`a == null` == `false`（不抛）

#### Scenario: 与非 record / 异类型对象比较
- **WHEN** `a.Equals("hello")`（other 非 `Point`）
- **THEN** == `false`（不抛）

#### Scenario: 类型精确（type-exact / EqualityContract）
- **WHEN** `[Record] class Base(int X)`、`[Record] class Derived(int X, int Y) : Base`，
  `Base b = new Derived(1, 2); Base c = new Base(1);`
- **THEN** `b.Equals(c)` == `false`、`b == c` == `false`（运行时类型不同 → 不等，即使基字段相同）

#### Scenario: 引用字段递归值比较
- **WHEN** `[Record] class Line(Point A, Point B)`（字段为 record 类型），两个同值 `Line`
- **THEN** `Equals` == `true`（引用字段用其自身 `Equals` 递归值比较，非句柄比较）

### Requirement: 参与相等的字段范围（对齐 C#）

C# record 合成的 `Equals` 比较 **全部实例字段**（含 private / 主构造器位置字段 / 块内声明字段）。

#### Scenario: 相等覆盖全部实例字段
- **WHEN** `[Record] class P(int X) { private int _tag; }`，两实例 `X` 相同但 `_tag` 不同
- **THEN** `Equals` == `false`（`_tag` 参与比较）

### Requirement: `[Record] struct` 值相等（已有，保持）

`[Record] struct` 是值类型，已由既有 blob-struct 合成路（`EmitSynthStructEquals`）获得逐叶子值相等。
本变更 **不改动** struct 的相等行为，仅补 ToString（见下）。

#### Scenario: struct record 值相等不回归
- **WHEN** `[Record] struct V(int X, int Y)`，两个同值实例
- **THEN** `Equals`/`==` 值相等，异值 `!=`（沿用现状）

### Requirement: 记录式 ToString（class & struct 统一）

为 `[Record]` 类型合成 `ToString()`，输出 C# record 精确格式 `TypeName { A = v, B = v }`。

#### Scenario: class record ToString 格式
- **WHEN** `[Record] class Point(int X, int Y)`，`new Point(1, 2).ToString()`
- **THEN** == `"Point { X = 1, Y = 2 }"`

#### Scenario: struct record ToString 格式（可观察变更）
- **WHEN** `[Record] struct V(int X, int Y)`，`new V(3, 4).ToString()`
- **THEN** == `"V { X = 3, Y = 4 }"`（此前为类型名 `"V"`——本变更改为记录格式）

#### Scenario: 单字段 record
- **WHEN** `[Record] class Wrap(string Name)`，`new Wrap("z42").ToString()`
- **THEN** == `"Wrap { Name = z42 }"`

#### Scenario: 无字段 record
- **WHEN** `[Record] class Empty()`，`new Empty().ToString()`
- **THEN** == `"Empty { }"`

### Requirement: ToString 字段范围 = public 成员（对齐 C#）

C# record 的 `ToString`（PrintMembers）只打印 **public** 属性/字段；private 字段不出现在 ToString
中（区别于相等比较的「全部字段」）。

#### Scenario: ToString 只含 public 字段
- **WHEN** `[Record] class P(int X) { private int _tag = 9; }`，`new P(1).ToString()`
- **THEN** == `"P { X = 1 }"`（`_tag` 不出现，但仍参与相等——见上）

### Requirement: 用户显式声明优先

用户若在 record 内显式声明 `Equals` / `GetHashCode` / `ToString` / `==`，合成让位，不覆盖用户版本。

#### Scenario: 显式 ToString 不被合成覆盖
- **WHEN** `[Record] class P(int X) { public override string ToString() { return "custom"; } }`
- **THEN** `new P(1).ToString()` == `"custom"`

## MODIFIED Requirements

### Requirement: `[Record]` 语义

**Before:** `[Record]` 仅为主构造器语法糖 + bit3 反射 flag，**不生成** `Equals`/`GetHashCode`/`ToString`/`==`
（值相等在 Deferred）。

**After:** `[Record]`（class & struct）额外合成 **member-wise 值相等**（`Equals`/`GetHashCode`/`==`/`!=`，
type-exact）与 **记录式 ToString**（`TypeName { A = v, B = v }`）。struct 相等沿用既有 blob 路，仅补 ToString。
`with` / 解构 / init-only 仍在 Deferred。

## IR Mapping

无新 IR 指令、无 zbc/zpkg 格式 bump。合成复用既有指令：

| 合成产物 | 复用指令 |
|---------|---------|
| class `Equals(object)` | `IsInstance` / `AsCast` / `FieldGet` / `Eq` / `Call`（引用字段递归 `.Equals`）/ `BrCond` / `ConstBool`（镜像 `EmitSynthEqualsResult`） |
| class `GetHashCode()` | `FieldGet` / `Call`（字段 `.GetHashCode`）/ 算术组合（`Mul`/`Add`/`BitXor`） |
| class `==` / `!=` | `OperatorEmitter` 拦截 record-class 操作数 → 发对 `Equals` 的调用（镜像 blob-struct `==` 拦截） |
| `ToString()`（两者） | `FieldGet` / `Call`（字段 `.ToString`）/ `StrConcat` 左折叠 |

## Pipeline Steps

受影响的 pipeline 阶段：
- [ ] Lexer — 无
- [ ] Parser / AST — 无
- [ ] TypeChecker — record-class `==`/`!=` 需允许值相等（视 OperatorEmitter 拦截落点确定；见 design）
- [x] IR Codegen — 新合成 pass（`RecordSynth`）+ IrGen 接线 + OperatorEmitter 拦截
- [x] VM interp — 视 struct-ToString 分派结论（见 design.md，Explore 确认后定）
- [ ] JIT / AOT — interp 全绿前不碰
