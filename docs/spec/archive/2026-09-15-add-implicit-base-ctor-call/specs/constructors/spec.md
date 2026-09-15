# Spec: 实例构造器与基类构造器

## MODIFIED Requirements

### Requirement: 实例构造器调用基类构造器（隐式或显式）

#### Scenario: 显式 ctor 无初始化子句，基类有无参 ctor
- **WHEN** `class W { int w; W() { w = 5; } }`、`class D5 : W { D5() { } }`，`new D5()`
- **THEN** `w == 5`

#### Scenario: 派生类无 ctor，基类有无参 ctor（经继承）
- **WHEN** `class D3 : W { }`，`new D3()`
- **THEN** `w == 5`

#### Scenario: 基类只有字段初始化器，派生类有显式 ctor
- **WHEN** `class B { int b = 7; }`、`class D : B { D(int x) { } }`，`new D(3)`
- **THEN** `b == 7`

#### Scenario: 执行顺序
- **WHEN** 派生类有初始化器 `f = Mark(1)`、基类 ctor `Mark(2)`、派生 ctor 体 `Mark(3)`，均无显式子句
- **THEN** 顺序 `123`；多层继承时自顶向下各层依次「本层初始化器 → 上一层」…… 与 C# 一致

#### Scenario: `: this(..)` 不隐式调用基类
- **WHEN** `D(int x) : this() { }`、`D() { }`，基类 ctor 有副作用
- **THEN** 基类 ctor 只执行一次

#### Scenario: 派生类写了 ctor，基类没有可零实参调用的 ctor
- **WHEN** `class B { B(int x) { } }` 且 `class D : B { D() { } }`
- **THEN** 编译错误（新专用码），指向 `D` 的 ctor，文本说明隐式 `base()` 找不到无参构造器、应写 `: base(...)`

## ADDED Requirements

### Requirement: 没写实例构造器的类继承基类的实例构造器

#### Scenario: 继承带参构造器
- **WHEN** `class B { public int v; public B(int x) { v = x; } }`、`class D : B { }`
- **THEN** `new D(3).v == 3`；`new D()` 报 E0426（基类没有无参 ctor，也就没有可继承的）

#### Scenario: 继承全部重载（含默认值与 params）
- **WHEN** 基类有 `B()`、`B(string s, int n = 2)`、`B(params int[] xs)`，派生类无 ctor
- **THEN** `new D()`、`new D("a")`、`new D("a", 5)`、`new D(1, 2, 3)` 都选中对应的继承构造器，结果与直接 `new B(..)` 一致

#### Scenario: 派生类初始化器照常执行
- **WHEN** `class D : B { public int d = Mark(1); }`、`B(int x) { Mark(2); }`
- **THEN** `new D(0)` 顺序 `12`

#### Scenario: 传递继承
- **WHEN** `class A { A(int x) {..} }`、`class B : A { }`、`class C : B { }`
- **THEN** `new C(1)` 调到 `A(int)`

#### Scenario: 写了任何构造器就不再继承
- **WHEN** `class B { B(int x) {..} B(string s) {..} }`、`class D : B { D(int x) : base(x) { } }`
- **THEN** `new D("s")` 报 E0426

#### Scenario: private 基类构造器不继承
- **WHEN** 基类有 `private B(int x)` 与 `public B()`
- **THEN** 派生类只继承 `D()`

#### Scenario: 跨包继承
- **WHEN** 基类在依赖包、派生类在主包且无 ctor；或派生类在中间包、主包 `new` 它
- **THEN** 两种形态均可用继承来的构造器（继承构造器随派生类所在包的 TSIG 导出）

#### Scenario: 泛型基类
- **WHEN** `class Box<T> { public T v; public Box(T x) { v = x; } }`、`class IntBox : Box<int> { }`
- **THEN** `new IntBox(5).v == 5`，形参类型按基类实参代换为 `int`

#### Scenario: 典型用法
- **WHEN** `class MyErr : Exception { }`
- **THEN** `throw new MyErr("bad")` 可编译，`Message == "bad"`

#### Scenario: 基类无 ctor 且无初始化器
- **WHEN** `class B { int b; }`、`class D : B { D() { } }`
- **THEN** 不产生基类 ctor 调用，`new D().b == 0`（无字节变化）

#### Scenario: 同包跨文件
- **WHEN** 基类与派生类在同一包的不同文件，基类只有字段初始化器、派生类无 ctor
- **THEN** 初始化器生效

#### Scenario: 跨包
- **WHEN** 基类在依赖包（只有字段初始化器 / 或有无参 ctor），派生类在主包（有 / 无显式 ctor）
- **THEN** 基类初始化器 / ctor 生效（interp / JIT 相同）

#### Scenario: `where T : new()` 与 E0426 不受合成 ctor 影响
- **WHEN** 类只有合成默认构造器
- **THEN** 满足 `new()`，`new C()` 不报 E0426（与改动前「无显式 ctor = 可默认构造」一致）
