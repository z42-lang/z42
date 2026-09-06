# Spec: 调用实参类型检查

## ADDED Requirements

### Requirement: 实参必须隐式可转到形参类型

调用一个签名已知的函数 / 方法 / 构造器时，每个位置实参的类型必须能**隐式转换**到对应形参类型，
判定门与赋值 / `return` / 变量初始化**完全一致**（`Conversion.Classify(from, to).ImplicitOk()`）。

#### Scenario: 引用类型不相容（自由函数）
- **WHEN** `class A {} class B {} void TakeA(A a) {}` 且调用 `TakeA(new B())`
- **THEN** 报 `E0402`，消息含 `cannot assign B to A (argument)`，span 指向实参 `new B()`
- **AND** 编译以非零码退出，**不写出 `.zbc` 产物**

#### Scenario: 基元 → 引用类型不相容
- **WHEN** `void TakeS(string s) {}` 且调用 `TakeS(42)`
- **THEN** 报 `E0402`

#### Scenario: 同一调用的多个不符实参逐条报
- **WHEN** 一次调用里有 N 个实参类型不符
- **THEN** 报出 **N 条**诊断（不在第一条短路）

#### Scenario: 实例方法路径
- **WHEN** `class C { void M(A a) {} }` 且调用 `c.M(new B())`
- **THEN** 报 `E0402`

#### Scenario: 静态方法路径
- **WHEN** `class C { static void S(A a) {} }` 且调用 `C.S(new B())`
- **THEN** 报 `E0402`

#### Scenario: 构造器路径
- **WHEN** `class C { C(A a) {} }` 且调用 `new C(new B())`
- **THEN** 报 `E0402`（今日只有 arity 检查 E0426，无类型检查）

### Requirement: 窄化实参需要显式 cast，常量在范围内除外

#### Scenario: 窄化实参缺 cast
- **WHEN** `void TakeB(byte b) {}`，`long v = 300;`，调用 `TakeB(v)`
- **THEN** 报 `E0439`（存在显式转换、缺 cast），**不是** `E0402`

#### Scenario: 常量在范围内放行
- **WHEN** `void TakeB(byte b) {}` 且调用 `TakeB(48)`
- **THEN** **无诊断**（与 `byte b = 48;` 同一条常量例外）

#### Scenario: 显式 cast 放行
- **WHEN** 调用 `TakeB((byte)v)`
- **THEN** **无诊断**

### Requirement: 上转 / 装箱 / 接口实现照常放行

#### Scenario: 派生类实参传基类形参
- **WHEN** `class D : A {}` 且调用 `TakeA(new D())`
- **THEN** **无诊断**

#### Scenario: 值类型传 object 形参
- **WHEN** `void TakeO(object o) {}` 且调用 `TakeO(42)`
- **THEN** **无诊断**（装箱）

#### Scenario: 实现类传接口形参
- **WHEN** `class L : IBag {}`、`void TakeI(IBag b) {}` 且调用 `TakeI(new L())`
- **THEN** **无诊断**

## MODIFIED Requirements

### Requirement: 跨包 imported 泛型签名保留型参身份（R1）

**Before:** `ImportedSymbolLoader` 读回跨包泛型方法签名时，方法级型参 `T` 变成名为 `"T"` 的普通
`Z42ClassType` —— 型参身份丢失，`Conversion` 的擦除放行不触发。

**After:** imported 方法级型参还原为 `Z42GenericParamType`，与同包声明一致。

#### Scenario: 跨包泛型方法调用
- **WHEN** 用户包调用 z42.core 的 `Array.Copy<T>(T[] source, T[] destination, int length)`，
  传 `byte[] a, byte[] b, 4`
- **THEN** **无诊断**（型参擦除放行）
- **AND** 同包声明的等价泛型方法行为不变

#### Scenario: 跨包泛型接口的实例化接收者
- **WHEN** `IBasicCollection<T>`（z42.core）以 `IBasicCollection<int>` 为接收者，调用 `AddOne(1)`
- **THEN** **无诊断**

### Requirement: 数组可赋给 `Array`（R2 / bug A）

**Before:** `X[]` → `Array` 判为无转换（`Conversion._classifyBuiltin` 只特判 `object`）。
今日在 var-decl 上下文即可复现：`Array boxed = x;`（`x: T[]`）报 `E0402 (var-decl)`。

**After:** 任意数组类型隐式可转到其基类 `Array`（`ImplicitRef`）。

#### Scenario: 数组传 Array 形参
- **WHEN** `void TakeArr(Array a) {}` 且调用 `TakeArr(new int[3])`
- **THEN** **无诊断**

#### Scenario: 数组赋给 Array 变量
- **WHEN** `Array a = new int[3];`
- **THEN** **无诊断**（回归 bug A）

### Requirement: enum 与其底层整数互转（R6 / bug D）

**Before:** enum 形参收整数实参报 `E0402`（`GCHandle.z42` 实测）。

**After:** enum ↔ 底层整数按 C# 规则（显式）/ z42 既定规则放行。

#### Scenario: 整数传 enum 形参
- **WHEN** `GCHandle` 内部把 `long` 传给 `GCHandleType` 形参
- **THEN** 按裁定规则放行或要求显式 cast，**不得**是"无转换"

### Requirement: lambda 实参按形参类型定型（R5）

**Before:** lambda 实参在重载决议**之前**被绑定，返回类型硬编码为 `Z42UnknownType`
（`ExprTyper.z42:136`）→ 语句体 lambda 得到 `Func<<unknown>>`，传给 `Action` 形参失败。

**After:** lambda 实参走既有的 target-typed 延迟绑定通道（同 target-typed `new`），
决议选定签名后按形参类型 `BindWithTarget` 回填。

#### Scenario: 语句体 lambda 传 Action 形参
- **WHEN** `Thread.Start(() => { doWork(); })`
- **THEN** **无诊断**，且 lambda 类型为 `Action`（不再是 `Func<<unknown>>`）

#### Scenario: 带返回值 lambda 传 Func 形参
- **WHEN** `void TakeF(Func<int> f) {}` 且调用 `TakeF(() => 1)`
- **THEN** **无诊断**，lambda 返回类型定型为 `int`（不再 `unknown`）

## 不检查的路径（残留洞，须在 book 登记）

### Requirement: 签名不可知的路径不报实参诊断

#### Scenario: 懒加载 stub 接收者 loose-bind
- **WHEN** 接收者类是懒加载空 stub 且主符号表无同名真类
- **THEN** **无实参诊断**（签名不可知，运行期经 DepIndex 解析）

#### Scenario: 错误 / 未知类型接收者
- **WHEN** 接收者类型为 `Z42ErrorType` / `Z42UnknownType`
- **THEN** **无实参诊断**（级联抑制）

## Pipeline Steps

受影响的 pipeline 阶段：
- [ ] Lexer —— 不涉及
- [ ] Parser / AST —— 不涉及
- [x] TypeChecker —— `OverloadBinder` / `MemberResolver` / `ConstructTyper` / `Conversion` / `ExprTyper` / `ImportedSymbolLoader`
- [ ] IR Codegen —— 不涉及（R5 会**间接**改善 lambda 的 IR 类型标注）
- [ ] VM interp —— 不涉及

## IR Mapping

无新增 IR 指令、无 zbc / zpkg 格式变更。

> R5 修复后 lambda 的返回类型从 `unknown` 变为真实类型，**会改变已发射 IR 的类型标注**
> （`-> unknown` → `-> void` / `-> i32`）→ 自举字节不动点须重新确认，golden `.zbc` 基线可能需要 regen。
