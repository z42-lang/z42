# Spec: 反射基元/泛型类型名

## MODIFIED Requirements

### Requirement: 基元类型的反射恒等（typeof ≡ GetType）

反射对基元类型（`int`/`long`/`double`/`bool`/`char`/`string`/`byte`/`short`/`uint`/`ulong`/
`sbyte`/`ushort`/`float`）无论经 `typeof(T)`、`obj.GetType()`、`FieldType`、`PropertyType`、
`ParameterType`、数组元素类型、泛型实参，均解析到**同一个真实 `Std.*` wrapper 类型句柄**。

**Before:** `typeof(int)` → handle-less 合成 Type：`Name="int"`、`FullName="int"`、
`IsValueType=false`、`GetMethods().Length=0`；与 `(5).GetType()`（真 `Std.Int32` 句柄）不一致。

**After:** `typeof(int)` → 真 `Std.Int32` 句柄，与 `(5).GetType()` 完全一致。

#### Scenario: typeof 基元名
- **WHEN** 求值 `typeof(int).FullName` 与 `typeof(int).Name`
- **THEN** 分别为 `"Std.Int32"` 与 `"Int32"`

#### Scenario: typeof 与 GetType 恒等
- **WHEN** `Type a = typeof(int); Type b = (5).GetType();`
- **THEN** `a.FullName == b.FullName`、`a.Name == b.Name`、`a.IsValueType == b.IsValueType == true`、
  `a.GetMethods().Length == b.GetMethods().Length`（均非 0）

#### Scenario: 基元是值类型
- **WHEN** 求值 `typeof(int).IsValueType`
- **THEN** `true`（回归 bug：此前为 `false`）

#### Scenario: 字段/属性/数组元素的基元类型名
- **WHEN** 反射一个 `int` 字段的 `FieldType`、`int` 属性的 `PropertyType`、`int[]` 的
  `GetElementType()`
- **THEN** 其 `.Name` 为 `"Int32"`、`.FullName` 为 `"Std.Int32"`

#### Scenario: 各基元覆盖
- **WHEN** 反射 `long`/`double`/`bool`/`char`/`string`
- **THEN** FullName 分别为 `"Std.Int64"`/`"Std.Double"`/`"Std.Boolean"`/`"Std.Char"`/`"Std.String"`，
  Name 为对应简名 `"Int64"`/`"Double"`/`"Boolean"`/`"Char"`/`"String"`

#### Scenario: z42.core 缺失兜底（退化）
- **WHEN** `Std.Int32` 未加载（理论环境异常）
- **THEN** 回落到 handle-less 合成 Type，`FullName` 用规范名（不 panic）

### Requirement: 构造型泛型的 FullName 含实参

构造型泛型（`typeof(Box<int>)` 或泛型实例 `.GetType()`）的 `FullName` 拼接实参的 FullName，
形如 `<基名>` + `<` + 逗号连接的实参 FullName + `>`；递归处理嵌套泛型。`Name` 仍为基简名。

**Before:** `typeof(List<int>).FullName == "Std.Collections.List"`（丢实参）。

**After:** `typeof(List<int>).FullName == "Std.Collections.List<Std.Int32>"`。

#### Scenario: 单实参泛型 FullName
- **WHEN** 求值 `typeof(List<int>).FullName`
- **THEN** `"Std.Collections.List<Std.Int32>"`

#### Scenario: 泛型实例与 typeof 一致
- **WHEN** `List<int> a = new(); a.GetType().FullName`
- **THEN** 等于 `typeof(List<int>).FullName` == `"Std.Collections.List<Std.Int32>"`

#### Scenario: 泛型 Name 不含实参
- **WHEN** 求值 `typeof(List<int>).Name`
- **THEN** `"List"`（基简名，不含 `<…>` / arity backtick）

#### Scenario: 多实参与嵌套泛型
- **WHEN** 求值 `typeof(Dictionary<string, int>).FullName`（多实参）与嵌套 `typeof(List<List<int>>).FullName`
- **THEN** 分别为 `"Std.Collections.Dictionary<Std.String,Std.Int32>"` 与
  `"Std.Collections.List<Std.Collections.List<Std.Int32>>"`（逗号无空格，递归展开）

#### Scenario: 非构造型不受影响
- **WHEN** 求值非泛型 `typeof(int).FullName` 或开放定义（无实参路径）
- **THEN** 不追加 `<…>`

## Pipeline Steps

受影响阶段（纯 VM 运行时反射；无编译器/格式改动）：
- [ ] VM 运行时反射（`corelib/reflection/type_object.rs`）
- [x] 无 Lexer / Parser / TypeChecker / IR Codegen / zbc·zpkg 格式改动

## IR Mapping

无新增 IR 指令 / opcode / zbc·zpkg 格式变更——`__fullName` / `__name` 是运行期构造的对象槽，非序列化元数据。
