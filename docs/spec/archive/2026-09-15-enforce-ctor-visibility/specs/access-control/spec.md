# Spec: 构造器可见性

## MODIFIED Requirements

### Requirement: 构造器调用受可见性约束（与字段 / 方法同一判据）

#### Scenario: private 构造器在类外不可调用
- **WHEN** `class C { C(int a) { } }`（无修饰符 = private）、`class U { void M() { var c = new C(1); } }`
- **THEN** E0404，文本 `cannot access private constructor \`C\` of \`C\``

#### Scenario: private 构造器在本类内可调用（静态工厂）
- **WHEN** `class C { private C() { } public static C Make() { return new C(); } }`
- **THEN** 无诊断

#### Scenario: protected 构造器可被派生类的初始化子句调用，不可在外部 new
- **WHEN** `class B { protected B(int x) { } } class D : B { public D() : base(1) { } }`、外部 `new B(1)`
- **THEN** `: base(1)` 无诊断；`new B(1)` 报 E0404

#### Scenario: private 基类构造器不可被派生类调用
- **WHEN** `class B { B(int x) { } } class D : B { public D() : base(1) { } }`
- **THEN** E0404

#### Scenario: `: this(..)` 调本类 private 构造器
- **WHEN** `class C { C(int a) { } public C() : this(1) { } }`
- **THEN** 无诊断

#### Scenario: 主构造器是 public
- **WHEN** `class P(int X);` / `[Record] struct R(int A);`，类外 `new P(1)`、元组 `(1, 2)`
- **THEN** 无诊断

#### Scenario: 跨包 internal / private 构造器
- **WHEN** 依赖包 `public class W { internal W(int x) { } public W() { } }`，主包 `new W(1)` 与 `new W()`
- **THEN** `new W(1)` 报 E0404（from another package）；`new W()` 无诊断

#### Scenario: target-typed new 与对象初始化器同样检查
- **WHEN** `C c = new(1);`、`new C(1) { F = 2 }`，`C(int)` 为 private
- **THEN** 均报 E0404
