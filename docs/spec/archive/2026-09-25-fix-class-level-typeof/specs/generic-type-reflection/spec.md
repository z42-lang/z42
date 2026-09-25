# Spec: 类级泛型型参的运行期具化（`typeof`）

## MODIFIED Requirements

### Requirement: 类级 `typeof(T)` 在实例语境产出绑定的类型实参

**Before:** `class Box<T>` 的实例方法里 `typeof(T)` 产出一个名为 `"T"` 的占位 constructed
type —— `typeof(T).FullName == "T"`、`typeof(T).Name == "T"`、`typeof(T) == typeof(int)` 为
`false`（即便 `T` 就是 `int`）。

**After:** 同一处 `typeof(T)` 产出 `T` 在**该实例**上绑定的真实类型，与方法级 `typeof(U)`
同口径。

#### Scenario: 单型参、值类型实参
- **WHEN** `class Box<T> { string F() { return typeof(T).FullName; } }`，求值
  `new Box<int>().F()`
- **THEN** 结果为 `"Std.Int32"`

#### Scenario: 单型参、引用类型实参
- **WHEN** 同上类，求值 `new Box<string>().F()`
- **THEN** 结果为 `"Std.String"`

#### Scenario: 同一泛型类的不同实例化互不串味
- **WHEN** 同一进程内先求 `new Box<int>().F()` 再求 `new Box<string>().F()`
- **THEN** 分别为 `"Std.Int32"` 与 `"Std.String"`（type_args 是 per-instance，不是
  per-TypeDesc）

#### Scenario: 多型参按声明序对位
- **WHEN** `class Pair<A, B> { string F() { return typeof(A).FullName + "/" + typeof(B).FullName; } }`，
  求值 `new Pair<int, string>().F()`
- **THEN** 结果为 `"Std.Int32/Std.String"`

#### Scenario: 值相等可用（招牌症状消失）
- **WHEN** `class Box<T> { bool IsInt() { return typeof(T) == typeof(int); } }`，
  求值 `new Box<int>().IsInt()` 与 `new Box<string>().IsInt()`
- **THEN** 分别为 `true` 与 `false`

#### Scenario: `Name` 与 `FullName` 两个成员都对
- **WHEN** `typeof(T).Name` 与 `typeof(T).FullName`，`T = int`
- **THEN** 分别为 `"Int32"` 与 `"Std.Int32"`（与 `typeof(int)` 上取同名成员逐字一致）

#### Scenario: 数组型参
- **WHEN** `new Box<int[]>().F()`
- **THEN** 结果为 `"Std.Int32[]"`，与 `typeof(int[]).FullName` 逐字一致
  （实例存的 type_arg 串就是 `"Std.Int32[]"`，`make_type_from_name` 的 `[]` 分支递归解析）

#### Scenario: 嵌套泛型型参
- **WHEN** `new Box<Pair<int, string>>().F()`
- **THEN** 与 `typeof(Pair<int, string>).FullName` 逐字一致
  （`make_type_from_name` 的 constructed-generic 分支逐层重入）

## ADDED Requirements

### Requirement: 方法级型参遮蔽同名类级型参（就近优先）

#### Scenario: 方法级 `T` 遮蔽类级 `T`
- **WHEN** `class Box<T> { string G<T>() { return typeof(T).FullName; } }`，
  在 `Box<int>` 实例上求值 `G<string>()`
- **THEN** 结果为 `"Std.String"`（方法级实参），**不是** `"Std.Int32"`
- **AND** 这与 `default(T)` / `new T()` / `new T[n]` 的既有「就近优先」口径逐字一致
- 📌 **这一条本变更前就已经是对的**（实测 `"Std.String"`）——列为 spec 是为了钉住
  「补类级分支不能把已经对的遮蔽路径改坏」，是回归护栏而非新行为

### Requirement: 载体为空时优雅降级，绝不产出错误的类型

**判据**：降级结果必须是**显然不对的占位名**，不能是**看起来对的错类型**——后者是静默缺陷。

#### Scenario: 静态语境（无 `this`）
- **WHEN** `class Box<T> { static string S() { return typeof(T).FullName; } }`，求值 `Box.S()`
- **THEN** 结果仍为占位 `"T"`（与本变更前逐字相同），不崩、不抛
- **AND** 🔴 **不得**读到第一个实参的 type_args —— `class Box<T> { static string S(Box<int> o) { return typeof(T).FullName; } }`
  求值 `Box.S(new Box<int>())` **必须**得 `"T"`，不能得 `"Std.Int32"`
  （`default(T)` 在同一形态下今天就是错的，本变更不得把该隐患搬进 `typeof`）

#### Scenario: 继承基类的型参（载体本就为空）
- **WHEN** `class Derived : Box<int> { }`，在 `new Derived()` 上求 `Box<T>` 体内的
  `typeof(T).FullName`
- **THEN** 结果为占位 `"T"`（与本变更前逐字相同），不崩
- **AND** 同一实例上 `default(T)` 仍为 `null`（既有行为，第二刀修）

#### Scenario: 非泛型类里没有类级型参可标
- **WHEN** 非泛型类的方法里写 `typeof(int)` / `typeof(SomeClass)`
- **THEN** 发射的指令序列与本变更前**逐字节相同**（仍走 `TypeofInstr`）

## IR Mapping

**不新增 IR 指令，不 bump 任何格式版本。** 类级分支复用既有 `Builtin` opcode：

```
%idx = const.i32 <classParamIndex>
%t   = builtin __class_type_arg(%this, %idx)     ← %this = 局部槽 reg 0
```

| 形态 | 发射 | 载体 |
|---|---|---|
| `typeof(具体类型)` | `TypeofInstr`（不变） | 编译期名字 |
| 方法级 `typeof(U)` | `MethodTypeArgInsn`（不变） | `frame.method_type_args[i]` |
| **类级 `typeof(T)`（本变更）** | **`BuiltinInstr __class_type_arg`** | `regs[0].instance.type_args[i]` |
| 类级 `default(T)`（对照，不变） | `DefaultOfInstr` | `regs[0].instance.type_args[i]` |

⚠️ `BuiltinId = BUILTINS 表下标、烤进 zbc` ⇒ 新条目**只可表尾追加**。

## Pipeline Steps

受影响的 pipeline 阶段：

- [ ] Lexer —— 不涉及
- [ ] Parser / AST —— 不涉及（`TypeofExpr` 已有）
- [x] TypeChecker —— `_bindTypeofExpr` 补类级分支 + 实例语境判据
- [x] IR Codegen —— `_emitTypeof` 补类级分支（发既有 Builtin opcode）
- [x] VM interp —— 新 corelib builtin
- [x] JIT —— **零改动**（`Instruction::Builtin` 是按名 / BuiltinId 的通用派发）
