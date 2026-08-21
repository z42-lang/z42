# Spec: 构造函数反射 + 方法反射层级（MethodBase / ConstructorInfo）

## ADDED Requirements

### Requirement: 反射类型层级对齐 C#

`MethodInfo` 与 `ConstructorInfo` 共享 `MethodBase` 基类，镜像 C#
`MemberInfo → MethodBase → {MethodInfo, ConstructorInfo}`。

#### Scenario: MethodInfo 是 MethodBase
- **WHEN** 取到任意方法的 `MethodInfo`
- **THEN** 它可赋值给 `MethodBase` 变量（`is`/`as` 关系成立），共享成员 `Name`/`IsStatic`/`GetParameters()` 可用

#### Scenario: ConstructorInfo 是 MethodBase 但不是 MethodInfo
- **WHEN** 取到构造函数的 `ConstructorInfo`
- **THEN** 它是 `MethodBase`（共享成员可用），但**不是** `MethodInfo`（两者是 MethodBase 的兄弟子类）

### Requirement: Type.GetConstructors 枚举构造函数

`GetConstructors()` 返回该类型声明的所有构造函数（按 ctor 命名约定 `<ClassFQ>.<ClassSimpleName>[$N]` 识别）。

#### Scenario: 枚举带参构造函数
- **WHEN** 类 `Point { public Point(int x, int y) {...} }`，调用 `typeof(Point).GetConstructors()`
- **THEN** 返回长度 ≥ 1 的 `ConstructorInfo[]`，其中一个的 `GetParameters().Length == 2`

#### Scenario: 无显式构造函数的类
- **WHEN** 类无显式构造函数（编译器合成默认无参 ctor），调用 `GetConstructors()`
- **THEN** 返回包含该无参 ctor 的 `ConstructorInfo[]`（`GetParameters().Length == 0`）

#### Scenario: 多个重载构造函数
- **WHEN** 类有 `C()` 与 `C(int)` 两个构造函数，调用 `GetConstructors()`
- **THEN** 返回长度 2 的 `ConstructorInfo[]`，两者 `GetParameters().Length` 分别为 0 与 1

### Requirement: ConstructorInfo.Invoke 带参构造实例

`ConstructorInfo.Invoke(object[] args)` 分配新实例，以其为 receiver 运行该构造函数（传入 args），返回构造好的对象。

#### Scenario: 带参构造返回初始化后的实例
- **WHEN** `Point(int x, int y)` 的 `ConstructorInfo` 经 `Invoke(new object[]{ 3, 4 })`
- **THEN** 返回的 `Point` 对象 `.x == 3`、`.y == 4`（构造函数体已执行），`.GetType().FullName` 为 Point 的 FQN

#### Scenario: 无参构造
- **WHEN** 无参 ctor 的 `ConstructorInfo` 经 `Invoke(new object[]{})` 或 `Invoke(null)`
- **THEN** 返回默认构造的实例，构造函数体已执行

#### Scenario: arity 不符抛异常
- **WHEN** 单参 ctor `Point(int)` 经 `Invoke(new object[]{ 1, 2 })`（给了 2 个）
- **THEN** 抛出可 catch 的 `Std.Exception`（消息含期望/实际参数数）

#### Scenario: 构造函数体内 throw 保留原类型
- **WHEN** ctor 体内 `throw new MyException(...)`，经 `ConstructorInfo.Invoke` 调用
- **THEN** 异常以原类型传播，调用方可 `try/catch (MyException)` 捕获（沿用 pending-thrown 机制）

## MODIFIED Requirements

### Requirement: Activator 与 ConstructorInfo 分工

**Before:** 反射建实例仅 `Activator.CreateInstance(Type)`——无参、且**不运行任何构造函数**（只分配 + 字段零初始化）。

**After:** `Activator.CreateInstance(Type)` 行为不变（无参、快路径）；新增 `ConstructorInfo.Invoke(args)`
提供**运行构造函数体**的带参构造。两者并存：Activator 用于无参快建，ConstructorInfo 用于需跑 ctor 逻辑 / 带参的场景。

## IR Mapping

- 无新 opcode。ConstructorInfo.Invoke 复用 ctor 函数（已在方法表，命名 `<ClassFQ>.<ClassSimpleName>[$N]`）
  + 分配逻辑（同 `__activator_create`）。
- 构造函数枚举无额外格式字段（靠命名约定）；本变更的格式 bump（zbc 1.37/zpkg 0.42）仅为**泛型方法**类型形参元数据。

## Pipeline Steps

- [ ] Lexer / Parser / TypeChecker — 无
- [ ] IR Codegen / 格式 — 无（ctor 反射不改格式）
- [ ] VM interp — reflection.rs：`__type_constructors` 枚举 + `__ctor_invoke` 带参构造
