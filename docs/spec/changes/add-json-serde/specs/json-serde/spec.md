# Spec: JSON serde（对象 ↔ JSON）

## ADDED Requirements

### Requirement: 对象序列化 Serialize(object) → JSON

#### Scenario: 基元字段序列化
- **WHEN** 一个对象含 `int` / `long` / `double` / `bool` / `string` 公开成员
- **THEN** `JsonSerializer.Serialize(o)` 产出 compact JSON，键=成员名，值=对应 JSON 基元（整数→number、
  浮点→number、bool→true/false、string→带引号串）

#### Scenario: 嵌套对象与数组
- **WHEN** 对象含另一个对象成员、或 `T[]` 数组成员
- **THEN** 递归序列化为嵌套 JSON object / array

#### Scenario: null 成员
- **WHEN** 引用类型成员为 null
- **THEN** 序列化为 JSON `null`

#### Scenario: 字段与属性都参与
- **WHEN** 对象既有公开字段又有公开可读属性（`{ get; }` / `{ get; set; }`）
- **THEN** 两者都出现在 JSON 输出中；键名=成员名

#### Scenario: pretty 输出
- **WHEN** 调用 `SerializePretty(o)`
- **THEN** 产出 2 空格缩进 + 换行的 JSON

### Requirement: 泛型反序列化 Deserialize<T>(string) → T

#### Scenario: 基元 + 嵌套 + 数组反序列化
- **WHEN** `Deserialize<T>(json)`，T 含基元 / 嵌套对象 / `E[]` 成员
- **THEN** 返回一个 T 实例，各成员按目标类型正确填充（JSON number 落到 `int` / `long` / `double`
  按目标宽度 coerce）

#### Scenario: 无参构造 + 成员写入
- **WHEN** T 有无参构造函数
- **THEN** 经 `Activator.CreateInstance` 建实例，对 JSON 中每个匹配键按名 `SetValue` 到可写成员

#### Scenario: 无无参构造 → 构造参数绑定（record / 只读 auto-prop）
- **WHEN** T 无无参构造，只有带参构造
- **THEN** 选参数最多的构造函数，按**参数名**从 JSON object 取值（递归反序列化到参数类型）后
  `ConstructorInfo.Invoke`；构造后再把剩余可写成员从 JSON 补齐

#### Scenario: 缺键 → 默认值
- **WHEN** JSON 缺少某成员对应键
- **THEN** 该成员保持类型默认值（不报错）；构造参数缺失时用参数 `DefaultValue`（可选参数）或类型默认

#### Scenario: 多余键 → 忽略
- **WHEN** JSON 含 T 中不存在的键
- **THEN** 忽略该键，不报错

#### Scenario: 类型不匹配 → 抛异常
- **WHEN** JSON 值类型与目标成员类型不兼容（如字符串 → int 字段）
- **THEN** 传播 `Std.JsonException`

#### Scenario: 非泛型入口
- **WHEN** 调用 `Deserialize(typeof(T), json)`
- **THEN** 行为等同 `Deserialize<T>(json)`（泛型薄壳即转发到此）

### Requirement: JSON 特性 [JsonProperty] / [JsonIgnore]

#### Scenario: 字段改名
- **WHEN** 公开字段标注 `[JsonProperty("custom_key")]`
- **THEN** 序列化用 `custom_key` 作键；反序列化从 `custom_key` 读

#### Scenario: 属性改名（含计算属性）
- **WHEN** 属性（`{ get; set; }` 或计算 `{ get { ... } }`）标注 `[JsonProperty("k")]`
- **THEN** 该属性的 attribute 可经 `PropertyInfo.GetCustomAttributes` 读到，键名解析为 `k`

#### Scenario: 忽略成员
- **WHEN** 字段/属性标注 `[JsonIgnore]`
- **THEN** 序列化与反序列化都跳过该成员

### Requirement: PropertyInfo 特性反射（背后字段载体，**无格式 bump**）

#### Scenario: 属性 attribute 往返（挂 `__prop_X` 背后字段）
- **WHEN** 某 auto-property 带 attribute，编译为 zpkg（**格式版本不变**）后由 VM 加载
- **THEN** `typeof(T).GetProperties()` 得到的 `PropertyInfo.GetCustomAttributes()` 返回该属性的 attribute
  实例（经背后字段 `__prop_<Name>` 的 `field_attributes`）
- **限制**：计算属性（有 getter 方法体、无背后字段）不承载 attribute（M2 已知限制）

### Requirement: System.Array 反射建/读写

#### Scenario: 反射建数组并填充
- **WHEN** 只以运行期 `Type elemType` 已知元素类型
- **THEN** `Array.CreateInstance(elemType, n)` 建长度 n 的 `elemType[]`；`Array.SetValue(arr,i,v)` /
  `Array.GetValue(arr,i)` / `Array.GetLength(arr)` 读写；`arr.GetType().GetElementType()` == elemType

## MODIFIED Requirements

**无格式版本变更**（zbc/zpkg minor 不动）。属性 attr 复用既有 `field_attributes` 载体，挂合成的
`__prop_<Name>` 背后字段。

## IR Mapping

- 属性上的 store-meta attr → **合成背后字段 `__prop_<Name>` 的 `.Attrs`**（既有 `field_attributes`
  编码，无新段）。
- `AttributeSynth._processMembers` 补 `PropertyDecl` 分支（合成工厂 + 记 `FactoryFunc`）；
  `ClassDescBuilder` 建 `__prop_X` 时把属性 attr-refs 写入其 `Attrs`。

## Pipeline Steps

- [ ] Lexer —— 无
- [ ] Parser / AST —— 无（PropertyDecl 已带 AttributedDecl 包裹）
- [x] Semantics —— AttributeSynth 为 PropertyDecl 的 store-meta attr 合成工厂
- [x] IR Codegen —— ClassDescBuilder 把属性 attr 挂 `__prop_X` 背后字段（既有 field_attributes 格式）
- [x] VM interp —— `__property_custom_attributes` 剥 get_/set_ → 查 `__prop_<Name>` field_attributes
