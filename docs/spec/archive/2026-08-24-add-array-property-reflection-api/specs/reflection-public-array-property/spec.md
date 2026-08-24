# Spec: 反射式数组 + 属性 attribute 公开反射 API

## ADDED Requirements

### Requirement: Std.Array 反射式数组 API（照搬 C# System.Array）

`Std.Array` 暴露 C# 风格反射 API：静态 `CreateInstance(Type,int)` 建数组，实例 `GetValue(int)` /
`SetValue(object value, int index)` 读写元素，长度用既有 `Length` 属性——供反射式 (de)serialization
在编译期不知元素类型时操作数组。

#### Scenario: 反射建数组并逐元素读写
- **WHEN** `Array arr = Array.CreateInstance(typeof(int), 3);` 再 `arr.SetValue(10, 0)`、`arr.SetValue(20, 1)`、
  `arr.SetValue(30, 2)`（C# 顺序：value 先、index 后）
- **THEN** `arr.Length == 3`，且 `arr.GetValue(0) == 10`、`arr.GetValue(1) == 20`、`arr.GetValue(2) == 30`

#### Scenario: 建引用类型数组
- **WHEN** `Array arr = Array.CreateInstance(typeof(string), 2);` 后 `arr.SetValue("a", 0)`
- **THEN** `arr.GetValue(0)` 返回 `"a"`，`arr.Length == 2`，未写入的槽为 `null`

#### Scenario: 对既有 T[] 经 object 反射读取（下转型）
- **WHEN** 有 `int[] xs = [1, 2, 3]`（编译期数组），以 `object o = xs`，再 `Array a = (Array)o;`
- **THEN** `a.Length == 3`、`a.GetValue(1) == 2`（`(Array)o` 下转型成立，公开 API 与编译期数组互通）

### Requirement: PropertyInfo 属性 attribute 反射

`Std.Reflection.PropertyInfo` 暴露 `GetCustomAttributes()` / `GetAttribute(Type)`，镜像 `FieldInfo`
的 attribute API，读取属性 backing field 上挂的 attribute 工厂并实例化（复用既有 field-attribute 格式，
无格式 bump）。

#### Scenario: 读取属性上的 attribute 实例
- **WHEN** 类 `C` 有属性 `[JsonProperty("n")] public string Name { get; set; }`，经 `typeof(C).GetProperties()`
  拿到该属性的 `PropertyInfo p`，调 `p.GetAttribute(typeof(JsonPropertyAttribute))`
- **THEN** 返回该 attribute 的活实例（非 null），可读其字段（如 name == "n"）

#### Scenario: 属性无该 attribute 返回 null
- **WHEN** 属性未标注目标 attribute 类型
- **THEN** `GetAttribute(t)` 返回 `null`；`GetCustomAttributes()` 返回不含该类型的数组（可为空数组）

#### Scenario: 计算属性（无 backing field）无 attribute
- **WHEN** 属性是计算属性（仅 `get` 表达式、无 backing field），调 `GetCustomAttributes()`
- **THEN** 返回空数组（不抛异常）

#### Scenario: GetCustomAttributes 返回全部标注实例
- **WHEN** 属性标注多个 attribute，调 `GetCustomAttributes()`
- **THEN** 返回数组含全部 attribute 活实例（与 `FieldInfo.GetCustomAttributes` 同语义；`GetAttribute`
  按运行期类型 `FullName` **精确匹配**取其一，非精确类型不命中——镜像 `FieldInfo.GetAttribute`）

## MODIFIED Requirements

### Requirement: z42.json serde 改用公开反射 API（去重）

**Before:** z42.json 经自有 `JsonReflect` 库的 5 个 `extern`（`__array_*` ×4 + `__property_custom_attributes`）
+ `PropAttr` 辅助访问反射数组与属性 attribute。

**After:** z42.json 的 `JsonBinder` / `JsonSerializer` / `JsonMember` 改用 `Std.Array` 的 C# 风格 API
（`CreateInstance` / 实例 `GetValue`/`SetValue` / `.Length`，序列化处 `(Array)o` 下转型）与
`PropertyInfo.GetAttribute`；`JsonReflect.z42` 删除这 5 个 extern 与 `PropAttr` 辅助（集合反射辅助保留）。
serde 行为不变（既有 `xtask test stdlib z42.json` 全绿）。

## IR Mapping

无新 IR 指令 / 无 zbc·zpkg 格式变更。新增方法 `[Native]` 绑既有 builtin：`__array_create` / `__array_get` /
`__array_set`（后者重排 arg 读取为 `(array,value,index)`）/ `__property_custom_attributes`；`__array_length`
删除（`.Length` 走 VM FieldGet 分支）。

## Pipeline Steps

- [ ] Lexer — 无
- [ ] Parser / AST — 无
- [ ] TypeChecker — 无（普通 extern 方法声明 + `(Array)o` 走既有 cast）
- [ ] IR Codegen — 无
- [ ] VM interp — `builtin_array_set` 重排 + 删 `builtin_array_length`（Decision 4；无格式变更）
