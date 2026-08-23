# Spec: 集合类型 serde（List<T> + Dictionary<string,V>）

## ADDED Requirements

### Requirement: List<T> 序列化 / 反序列化

#### Scenario: List<基元> 序列化为 JSON array
- **WHEN** 序列化含 `List<int>` 字段（元素 `[1,2,3]`）的对象
- **THEN** 该字段 → JSON array `[1,2,3]`（按 Count + indexer 顺序）

#### Scenario: List<嵌套对象> 序列化
- **WHEN** `List<Point>` 元素为对象
- **THEN** → JSON array，每元素按其类型递归序列化

#### Scenario: JSON array 反序列化为 List<T>
- **WHEN** 目标成员类型 `List<int>`，JSON `[1,2,3]`
- **THEN** 反射构造 `List<int>` + 逐元素 `FromJson(int, …)` 反射 `Add` → `Count==3`，元素有序

#### Scenario: 空 List 往返
- **WHEN** `List<int>` 为空 / JSON `[]`
- **THEN** 序列化 `[]`；反序列化得 `Count==0` 的 `List<int>`

### Requirement: Dictionary<string,V> 序列化 / 反序列化

#### Scenario: Dictionary<string,int> 序列化为 JSON object
- **WHEN** 序列化含 `Dictionary<string,int>`（`{"a":1,"b":2}`）的对象
- **THEN** → JSON object `{"a":1,"b":2}`（键 = 字符串键，值按 V 递归）

#### Scenario: JSON object 反序列化为 Dictionary<string,V>
- **WHEN** 目标成员类型 `Dictionary<string,int>`，JSON `{"a":1,"b":2}`
- **THEN** 反射构造 + 逐键 `FromJson(V, …)` 反射 indexer set → `Count==2`

#### Scenario: Dictionary<string,嵌套对象>
- **WHEN** V 为对象类型
- **THEN** 值按对象类型递归序列化 / 反序列化

### Requirement: 集合检测优先于对象分派

#### Scenario: 集合类型不被当普通对象序列化
- **WHEN** 成员/运行期类型 `FullName` 为 `Std.Collections.List` / `Std.Collections.Dictionary`
- **THEN** 走集合分支（array / object），**不**遍历其内部字段（items/capacity/Count 等）

## MODIFIED Requirements

`_toJson` / `FromJson` 的类型分派**新增** List/Dict 分支（在基元/数组之后、通用对象之前）。基元 / 数组 /
对象 / 属性行为不变。**无格式版本变更**（纯 z42.json 反射-only）。

## IR Mapping
- 无新 IR / opcode / 格式改动。复用既有反射 native（`GetGenericArguments` / `Activator.CreateInstance` /
  `MethodInfo.Invoke` / `GetMethods` / `GetFields`）。

## Pipeline Steps
- [ ] Lexer / Parser / TypeChecker / IR Codegen —— 无
- [x] stdlib（z42.json）—— `_toJson` / `FromJson` 集合分支 + `JsonReflect` 集合辅助
- [x] 测试 —— serialize.z42 / deserialize.z42 加集合用例（含空集合 + 嵌套 + 往返）
