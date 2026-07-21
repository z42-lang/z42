# Spec: 泛型反射三件套（G1–G3 目标行为）

> G0 规划锁定的目标行为；G1–G3 各 change 实现时按对应场景验证。

## ADDED Requirements

### Requirement: MakeGenericType（G1）

#### Scenario: 定义型 + 实参 → 构造型
- **WHEN** `Type def = typeof(List<>)`（open generic），`Type made = def.MakeGenericType(typeof(int))`
- **THEN** `made` 等价 `typeof(List<int>)`：`made.GetGenericArguments()[0] == typeof(int)`，`made.IsGenericTypeDefinition == false`

#### Scenario: 实参数不符抛异常
- **WHEN** 定义型有 2 个类型参数，只传 1 个 arg
- **THEN** 抛 `Std.Exception`（arg 数不匹配）

#### Scenario: 约束违背抛异常（Q3 安全要求）
- **WHEN** 定义 `class Repo<T> where T : IEntity`，`typeof(Repo<>).MakeGenericType(typeof(int))`（`int` 不实现 `IEntity`）
- **THEN** 抛 `Std.Exception`（信息含类型参数名 + 违背的约束）——反射不得造出违反约束的构造型

#### Scenario: 约束满足正常构造
- **WHEN** `typeof(Repo<>).MakeGenericType(typeof(User))`（`User : IEntity`）
- **THEN** 正常返回 `Repo<User>` 构造型（约束 `IEntity` 满足）

### Requirement: 构造泛型 CreateInstance（G1）

#### Scenario: 构造型造实例携带 type_args
- **WHEN** `Type made = typeof(Box<>).MakeGenericType(typeof(int))`，`object o = Activator.CreateInstance(made)`
- **THEN** `o.GetType().GetGenericArguments()[0] == typeof(int)`（实例 type_args 被灌入，反射信息不丢）

### Requirement: 参数化 CreateInstance（G2）

#### Scenario: 带参 ctor 构造
- **WHEN** `Activator.CreateInstance(typeof(Point), boxedArgs)`（Point 有 `(int,int)` ctor）
- **THEN** 经 ctor 重载决议构造，字段按参数初始化

### Requirement: 泛型方法 Invoke（G2）

#### Scenario: MakeGenericMethod + Invoke
- **WHEN** 泛型方法 `T Identity<T>(T x)`，`mi.MakeGenericMethod(typeof(int)).Invoke(obj, [42])`
- **THEN** 返回 `42`，方法体内 `typeof(T) == typeof(int)`

### Requirement: CreateInstance\<T\>（G3）

#### Scenario: 泛型糖
- **WHEN** `T v = Activator.CreateInstance<T>()`（T 方法级类型参数，运行期具体）
- **THEN** 等价 `(T) Activator.CreateInstance(typeof(T))`

### Requirement: Deserialize\<T\> 端到端（G3，喂 L 流招牌）

#### Scenario: 泛型 serde 主路径
- **WHEN** `List<Point> pts = Json.Deserialize<List<Point>>(json)`
- **THEN** 经 MakeGenericType + CreateInstance 自动绑定构造，元素字段正确填充

## IR / 格式

- G1 / G2(参数化 CreateInstance)：**纯 runtime，无格式 bump**（复用 make_constructed_type / activator）。
- G2(泛型方法 Invoke)：**可能触 IR**（方法级 type_args 供给通道，Q2）——届时若需新指令/帧槽，走 lang/ir 完整流程 + 可能格式 bump。
