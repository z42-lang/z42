# Spec: 泛型方法反射式调用（MakeGenericMethod + Invoke）

## ADDED Requirements

### Requirement: 方法级泛型元数据可反射

METHOD 段携带方法自身的类型形参（区别于类的类型形参），使反射能识别泛型方法并列出其类型形参。

#### Scenario: 识别泛型方法定义
- **WHEN** 对含 `public static T Identity<T>(T x)` 的类型调用 `GetMethods()`，取到 `Identity` 的 `MethodInfo`
- **THEN** `mi.IsGenericMethod == true` 且 `mi.IsGenericMethodDefinition == true`

#### Scenario: 非泛型方法不误判
- **WHEN** 取到非泛型方法（如 `int Add(int a, int b)`）的 `MethodInfo`
- **THEN** `mi.IsGenericMethod == false`、`mi.IsGenericMethodDefinition == false`，且 `GetGenericArguments()` 返回空数组

#### Scenario: 定义态列出类型形参
- **WHEN** 对泛型方法定义 `Map<K, V>(...)` 的 `MethodInfo` 调用 `GetGenericArguments()`
- **THEN** 返回长度为 2 的 `Std.Type[]`，其名字依次为 `"K"`、`"V"`（类型形参占位）

### Requirement: MakeGenericMethod 绑定类型实参

`MakeGenericMethod(params Std.Type[])` 返回构造后的 `MethodInfo`（C# 语义：同为 `MethodInfo`，无独立子类型）。

#### Scenario: 绑定成功返回构造态 MethodInfo
- **WHEN** 对 `Identity<T>` 的定义态 `MethodInfo` 调用 `MakeGenericMethod(typeof(Std.Int32))`
- **THEN** 返回一个 `MethodInfo`，其 `IsGenericMethod == true`、`IsGenericMethodDefinition == false`，
  且 `GetGenericArguments()` 返回 `[typeof(Std.Int32)]`（已绑定的实参，非占位）

#### Scenario: arity 不符抛异常
- **WHEN** 对单形参泛型方法 `Identity<T>` 调用 `MakeGenericMethod(typeof(int), typeof(string))`（给了 2 个）
- **THEN** 抛出可 catch 的 `Std.Exception`（消息含期望/实际 arity），不产生构造态 MethodInfo

#### Scenario: 对非泛型方法调用 MakeGenericMethod 抛异常
- **WHEN** 对非泛型方法的 `MethodInfo` 调用 `MakeGenericMethod(typeof(int))`
- **THEN** 抛出可 catch 的 `Std.Exception`

### Requirement: 构造态 MethodInfo 反射式 Invoke（静态）

构造态 MethodInfo 的 `Invoke` 把绑定的类型实参线程进 callee 帧，方法体的 `typeof(T)`/`new T()`/`default(T)`
按实参具化，结果与直接调用 `Foo<Arg>()` 逐点一致。

#### Scenario: 静态泛型方法反射调用返回值正确
- **WHEN** `Identity<T>(T x) => x` 经 `MakeGenericMethod(typeof(int)).Invoke(null, new object[]{ 42 })`
- **THEN** 返回 `42`

#### Scenario: 方法体 typeof(T) 反射与直接一致
- **WHEN** 方法 `TypeNameOf<T>() => typeof(T).FullName` 分别经①直接调用 `TypeNameOf<Std.String>()`
  与②`MakeGenericMethod(typeof(Std.String)).Invoke(null, empty)`
- **THEN** 两者返回相同字符串（`"Std.String"`）

#### Scenario: 方法体 default(T) 反射具化
- **WHEN** `DefaultOf<T>() => default(T)` 经 `MakeGenericMethod(typeof(Std.Int32)).Invoke(null, empty)`
- **THEN** 返回 `0`（值类型零值）；对 `MakeGenericMethod(typeof(Std.String))` 返回 `null`（引用类型）

#### Scenario: 方法体 new T() 反射具化
- **WHEN** `Create<T>() => new T()`（`T` 有无参构造）经 `MakeGenericMethod(typeof(SomeClass)).Invoke(null, empty)`
- **THEN** 返回的对象 `.GetType().FullName == "SomeClass 的 FQN"`

### Requirement: 构造态 MethodInfo 反射式 Invoke（实例）

实例泛型方法的反射调用以 `obj` 为 receiver（reg0），类型实参线程与 receiver 正交。

#### Scenario: 实例泛型方法反射调用
- **WHEN** 实例方法 `Box<T>(T v) => ...`（在对象 `o` 上）经
  `mi.MakeGenericMethod(typeof(int)).Invoke(o, new object[]{ 7 })`
- **THEN** 以 `o` 为 receiver 执行，类型实参 `int` 在方法体内可 `typeof(T)` 得到，返回值正确

#### Scenario: 反射调用中的 throw 保留原类型
- **WHEN** 泛型方法体内 `throw new MyException(...)`，经构造态 `Invoke` 调用
- **THEN** 异常以原类型传播，调用方可 `try/catch (MyException)` 捕获（沿用非泛型 Invoke 的 pending-thrown 机制）

## MODIFIED Requirements

### Requirement: MethodInfo.Invoke 支持泛型线程

**Before:** `MethodInfo.Invoke(obj, args)` 仅走非泛型 `exec_function`，忽略任何类型实参。

**After:** `Invoke` 检测 receiver MethodInfo 的隐藏 `__typeArgs`；非空时把类型实参转为类型名线程进 callee
`frame.method_type_args`（复用 M1 物化路径）。非泛型 / 定义态（无 `__typeArgs`）行为**逐字节不变**。

## IR Mapping

- 无新 opcode。复用 M1 的 `MethodTypeArg(0xB2)`/`MethodDefault(0xB3)`（方法体物化）与 `frame.method_type_args`
  帧槽。反射式调用只是 `frame.method_type_args` 的**另一个填充来源**（native 填，非 `CallGeneric` 指令填）。
- **新增格式**：zbc METHOD 段 + zpkg TSIG 方法记录携带方法类型形参（TpCount + 名字）。zbc 1.37 / zpkg 0.42。

## Pipeline Steps

受影响阶段：
- [ ] Lexer — 无
- [ ] Parser / AST — 无（方法类型形参已由 M1 解析，本变更只补元数据 emit）
- [ ] TypeChecker — 无
- [ ] IR Codegen / zbc·zpkg 格式 — METHOD 段写方法类型形参（ZbcWriter/ZbcReader/TsigReconcile/ExportedMethodZ）
- [ ] VM interp — reflection.rs：make_generic native + invoke 线程 typeArgs + MethodInfo 构造填元数据；zbc_reader 读方法类型形参
