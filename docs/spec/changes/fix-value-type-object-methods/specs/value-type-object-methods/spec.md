# Spec: 值类型 + Type 对象 Object 方法 / 数组全路径名

## ADDED / MODIFIED Requirements

### Requirement: struct 实例支持 Object 继承方法

struct 实例可调用 `GetType()` / `ToString()` / `GetHashCode()` / `Equals(object)`，语义对齐 C#。

#### Scenario: struct GetType
- **WHEN** `struct A { public int X; } A a = new A(); a.GetType()`
- **THEN** 返回 `typeof(A)`：`FullName == "Demo.A"`、`IsValueType == true`、`a.GetType() ` 等价 `typeof(A)`

#### Scenario: struct ToString 默认
- **WHEN** 非 record 的 struct `a.ToString()`
- **THEN** 返回短类型名（C# ValueType 默认），不崩

#### Scenario: struct GetHashCode / Equals
- **WHEN** `a.GetHashCode()` / `a.Equals(a)`
- **THEN** 分别返回稳定 hash / bool（值相等），不崩

#### Scenario: record struct 自声明方法不被破坏
- **WHEN** `[Record] struct` 的 `ToString()` / `Equals()`（编译器合成）
- **THEN** 仍走其合成实现（record 格式 / 逐字段相等），不被装箱协议改写

### Requirement: enum 实例 GetType 返回枚举类型

#### Scenario: enum GetType
- **WHEN** `enum E { Red, Green } E.Red.GetType()`
- **THEN** 返回 `typeof(E)`：`FullName == "Demo.E"`、`IsEnum == true`（**非** `Std.Int32`）

### Requirement: Type 对象的 GetType 返回 Std.Type

#### Scenario: GetType on a Type object
- **WHEN** `typeof(A).GetType()`（receiver 是 `Std.Type`）
- **THEN** 返回 `typeof(Std.Type)`（非 null）；`typeof(A).GetType().FullName == "Std.Type"`（不再 `FieldGet on Null`）

### Requirement: 数组类型 FullName / Name 全路径

**Before:** `typeof(int[]).FullName == "Std.Array"`、`Name == "Array"`（丢元素类型）。
**After:** `FullName == "Std.Int32[]"`、`Name == "Int32[]"`。

#### Scenario: 基元数组
- **WHEN** `typeof(int[]).FullName` / `.Name`
- **THEN** `"Std.Int32[]"` / `"Int32[]"`

#### Scenario: 嵌套数组
- **WHEN** `typeof(int[][]).FullName`
- **THEN** `"Std.Int32[][]"`（递归）

#### Scenario: 用户类型数组
- **WHEN** `typeof(A[]).FullName`（A 为用户 struct/class）
- **THEN** `"Demo.A[]"`

#### Scenario: GetElementType 不变
- **WHEN** `typeof(int[]).GetElementType()`
- **THEN** 仍返回 `typeof(int)`（`Std.Int32`），行为不变

## Pipeline Steps
- [ ] TypeChecker / Binder（值类型 Object 方法解析——若需，见 design 决策 3）
- [ ] IR Codegen（CallEmitter：GetType 折叠 typeof / 装箱 + VCall）
- [ ] VM interp + JIT（数组名 build_type_ex；装箱-struct 协议已存在）
- [x] 无新 zbc/zpkg 格式变更（除非决策 3 选 B——届时停下问 User）

## IR Mapping
- 值类型 `GetType()` → 复用 `Typeof` opcode（无新指令）。
- struct 其余 Object 方法 → `__box_struct` builtin + `VCall`（均已存在）。
- 数组名 → runtime 反射构造（无 IR/格式变更）。
