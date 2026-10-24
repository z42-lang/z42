<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.json/ -->

# z42.json —— JSON 读写与对象序列化。

> 包路径 `src/libraries/z42.json/`；命名空间 `Std.Json`

两层 API，按需取用：DOM 层（[`JsonValue`]）把 JSON 文本解析成值树，逐键 / 逐下标访问，
也可手工搭树再输出；serde 层（[`JsonSerializer`]）经反射在用户对象与 JSON 文本之间直转，
不用写 DOM 代码。

整份文档一次性读进内存：没有增量 / 流式 reader，超大 JSON 需自行分块。解析失败一律
fail-fast，抛第一个错误，不聚合错误列表。

## 不支持

- 增量 / 流式解析：`ParseStream` 只是「读到 EOF 再整体解析」，不是逐 token 的流式 reader。
- 注释保留：relaxed 模式解析掉的注释不进值树，也不会被 `Stringify` 写回。
- JSON Schema 校验、自定义命名策略、自定义转换器。
- `Stringify` 的两个非法输出（读进来能原样写回去不成立的场景）：
  - `NaN` / `Infinity` 写出为 `NaN` / `inf` / `-inf`——不是合法 JSON，连 `ParseRelaxed`
    也读不回来（relaxed 只认 `NaN` / `Infinity` 拼写）。要 round-trip 得自己先检出这些值。
  - 整值 double（`42.0`）写出为 `42`，再解析回来 kind 变 Long。
- 数组下标 / 键缺失不返回默认值：`At` / `Get` 越界或缺键一律抛异常，需要容错就先
  `ContainsKey` / `Length`，或改用 `JsonPath.Select`（走不通返回 `null`）。

## JsonBinder

[`src/libraries/z42.json/src/JsonBinder.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonBinder.z42#L9)

反序列化反射核：按目标 Type 递归把 JSON 绑定为对象，故 JSON 的整数 / 浮点能正确落到 int / long / double 目标成员。

构造模型：有无参构造器（或无构造器）→ Activator + 按名赋值；否则选参数最多的构造器按
参数名匹配 JSON 键调用，再补齐剩余可写成员。类型覆盖基元 + 嵌套对象 + 定长数组 T[]。

```z42
public static class JsonBinder {
    static object FromJson(Type t, JsonValue v)
}
```

成员：[`FromJson`](#Std.Json.JsonBinder.FromJson(Type,JsonValue))

<a id="Std.Json.JsonBinder.FromJson(Type,JsonValue)"></a>
### static object FromJson(Type t, JsonValue v)

[`src/libraries/z42.json/src/JsonBinder.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonBinder.z42#L14)

把 `v` 递归绑定为 `t` 的实例并返回；`v` 为 JSON null 时返回 null。

基元 / string 直接转换；数组、List、Dictionary（键须为 string，否则抛
JsonException）逐元素递归；其余对象按类注释的构造模型创建。

## JsonException

[`src/libraries/z42.json/src/JsonException.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonException.z42#L4)

JSON 解析 / 序列化错误；携带 1-based 行 / 列号（0 = 不可用，如 stringify 阶段错误）。

```z42
public class JsonException : Exception {
    int Line
    int Column
    JsonException(string message)
    JsonException(string message, int line, int column)
}
```

成员：[`Line`](#Std.Json.JsonException.Line) · [`Column`](#Std.Json.JsonException.Column) · [`JsonException`](#Std.Json.JsonException.ctor(string)) · [`JsonException`](#Std.Json.JsonException.ctor(string,int,int))

<a id="Std.Json.JsonException.Line"></a>
### int Line

[`src/libraries/z42.json/src/JsonException.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonException.z42#L6)

1-based 行号；0 = 不可用（如 stringify 阶段错误）。

<a id="Std.Json.JsonException.Column"></a>
### int Column

[`src/libraries/z42.json/src/JsonException.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonException.z42#L8)

1-based 列号；0 = 不可用。

<a id="Std.Json.JsonException.ctor(string)"></a>
### JsonException(string message)

[`src/libraries/z42.json/src/JsonException.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonException.z42#L11)

无位置版本：Line / Column 置 0。

<a id="Std.Json.JsonException.ctor(string,int,int)"></a>
### JsonException(string message, int line, int column)

[`src/libraries/z42.json/src/JsonException.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonException.z42#L17)

带位置版本：`line` / `column` 按 1-based 原样保存。

## JsonIgnoreAttribute

[`src/libraries/z42.json/src/JsonIgnoreAttribute.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonIgnoreAttribute.z42#L4)

标注成员在序列化 / 反序列化时被跳过。用法 `[JsonIgnore]`，可标注字段或属性。

```z42
public class JsonIgnoreAttribute : Attribute {
}
```

## JsonMember

[`src/libraries/z42.json/src/JsonMember.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonMember.z42#L10)

统一「可序列化成员」抽象：封装字段 (FieldInfo) 或属性 (PropertyInfo)。

携带解析后的 JSON 键名、成员类型、读写能力；序列化 / 反序列化引擎按这层统一
遍历，无需区分字段与属性。

键名解析：`[JsonProperty("k")]` 覆盖默认成员名；`[JsonIgnore]` 排除成员。

```z42
public class JsonMember {
    string JsonKey
    Type MemberType
    bool CanRead
    bool CanWrite
    object Get(object target)
    void Set(object target, object value)
    static JsonMember OfField(FieldInfo f, string key)
    static JsonMember OfProperty(PropertyInfo p, string key)
}
```

成员：[`JsonKey`](#Std.Json.JsonMember.JsonKey) · [`MemberType`](#Std.Json.JsonMember.MemberType) · [`CanRead`](#Std.Json.JsonMember.CanRead) · [`CanWrite`](#Std.Json.JsonMember.CanWrite) · [`Get`](#Std.Json.JsonMember.Get(object)) · [`Set`](#Std.Json.JsonMember.Set(object,object)) · [`OfField`](#Std.Json.JsonMember.OfField(FieldInfo,string)) · [`OfProperty`](#Std.Json.JsonMember.OfProperty(PropertyInfo,string))

<a id="Std.Json.JsonMember.JsonKey"></a>
### string JsonKey

[`src/libraries/z42.json/src/JsonMember.z42:16`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonMember.z42#L16)

JSON 键名（[JsonProperty] 覆盖或成员名）。

<a id="Std.Json.JsonMember.MemberType"></a>
### Type MemberType

[`src/libraries/z42.json/src/JsonMember.z42:18`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonMember.z42#L18)

成员的静态类型。

<a id="Std.Json.JsonMember.CanRead"></a>
### bool CanRead

[`src/libraries/z42.json/src/JsonMember.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonMember.z42#L20)

可读（序列化）。

<a id="Std.Json.JsonMember.CanWrite"></a>
### bool CanWrite

[`src/libraries/z42.json/src/JsonMember.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonMember.z42#L22)

可写（反序列化）。

<a id="Std.Json.JsonMember.Get(object)"></a>
### object Get(object target)

[`src/libraries/z42.json/src/JsonMember.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonMember.z42#L25)

读 `target` 上该成员的当前值（按封装的是字段还是属性走对应反射）。

<a id="Std.Json.JsonMember.Set(object,object)"></a>
### void Set(object target, object value)

[`src/libraries/z42.json/src/JsonMember.z42:31`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonMember.z42#L31)

把 `value` 写入 `target` 上该成员（按封装的是字段还是属性走对应反射）。

<a id="Std.Json.JsonMember.OfField(FieldInfo,string)"></a>
### static JsonMember OfField(FieldInfo f, string key)

[`src/libraries/z42.json/src/JsonMember.z42:37`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonMember.z42#L37)

封装字段 `f` 为可序列化成员；字段恒可读写，`key` 为解析后的 JSON 键名。

<a id="Std.Json.JsonMember.OfProperty(PropertyInfo,string)"></a>
### static JsonMember OfProperty(PropertyInfo p, string key)

[`src/libraries/z42.json/src/JsonMember.z42:48`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonMember.z42#L48)

封装属性 `p` 为可序列化成员；可读 / 可写随属性自身，`key` 为解析后的 JSON 键名。

## JsonMembers

[`src/libraries/z42.json/src/JsonMember.z42:58`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonMember.z42#L58)

成员集合构建：从 Type 收集 public 实例字段 + public 属性，解析键名 / 忽略。

```z42
public static class JsonMembers {
    static JsonMember[] For(Type t)
}
```

成员：[`For`](#Std.Json.JsonMembers.For(Type))

<a id="Std.Json.JsonMembers.For(Type)"></a>
### static JsonMember[] For(Type t)

[`src/libraries/z42.json/src/JsonMember.z42:60`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonMember.z42#L60)

收集 t 的可序列化成员（public 非 static 字段 + 属性；[JsonIgnore] 排除）。

## JsonParser

[`src/libraries/z42.json/src/JsonParser.z42:5`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonParser.z42#L5)

递归下降的 RFC 8259 JSON 解析器。

```z42
public sealed class JsonParser {
    JsonParser(string src)
    static JsonParser Relaxed(string src)
    JsonValue ParseDocument()
}
```

成员：[`JsonParser`](#Std.Json.JsonParser.ctor(string)) · [`Relaxed`](#Std.Json.JsonParser.Relaxed(string)) · [`ParseDocument`](#Std.Json.JsonParser.ParseDocument())

<a id="Std.Json.JsonParser.ctor(string)"></a>
### JsonParser(string src)

[`src/libraries/z42.json/src/JsonParser.z42:26`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonParser.z42#L26)

以 `src` 为输入构建严格模式（RFC 8259）解析器；宽松语法见 Relaxed。

<a id="Std.Json.JsonParser.Relaxed(string)"></a>
### static JsonParser Relaxed(string src)

[`src/libraries/z42.json/src/JsonParser.z42:36`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonParser.z42#L36)

构建宽松模式解析器：接受 JSON5 风格的 `//` / `/* */` 注释与数组 / 对象尾逗号。

<a id="Std.Json.JsonParser.ParseDocument()"></a>
### JsonValue ParseDocument()

[`src/libraries/z42.json/src/JsonParser.z42:45`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonParser.z42#L45)

解析整个文档：恰好一个根值 + 可选前后空白，返回根 JsonValue。

抛 `JsonException`：空输入、根值后有多余字符或任何语法错误（带行 / 列）。

## JsonPath

[`src/libraries/z42.json/src/JsonPath.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonPath.z42#L14)

单值 JSONPath 子集：按路径在 JSON 值树中取单个值。

支持的路径语法：`$` 根锚点（可省略）、`.name` 对象键、`["name"]` / `['name']` 对象键、
`[n]` 数组下标（非负十进制），段间空格 / 制表符容忍。

不支持：`..` 递归下降、`*` 通配（显式抛异常，不静默返回 null）、`?(...)` 过滤、
`[start:end]` 切片、函数调用。

路径任一步走不通（键缺失、下标越界、kind 不符）返回 `null`；仅路径语法本身非法才抛
`JsonException`。

```z42
public static class JsonPath {
    static JsonValue? Select(JsonValue root, string path)
}
```

成员：[`Select`](#Std.Json.JsonPath.Select(JsonValue,string))

<a id="Std.Json.JsonPath.Select(JsonValue,string)"></a>
### static JsonValue? Select(JsonValue root, string path)

[`src/libraries/z42.json/src/JsonPath.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonPath.z42#L17)

按 `path` 遍历 `root`，返回路径上的值；路径走不通（键缺失、下标越界、kind 不符）返回 null。

## JsonPropertyAttribute

[`src/libraries/z42.json/src/JsonPropertyAttribute.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonPropertyAttribute.z42#L6)

显式指定成员的 JSON 键名，serde 引擎按名映射时以此覆盖默认成员名。

用法 `[JsonProperty("json_name")]`，可标注字段或 auto-property。

```z42
public class JsonPropertyAttribute : Attribute {
    string Name
    JsonPropertyAttribute(string name)
}
```

成员：[`Name`](#Std.Json.JsonPropertyAttribute.Name) · [`JsonPropertyAttribute`](#Std.Json.JsonPropertyAttribute.ctor(string))

<a id="Std.Json.JsonPropertyAttribute.Name"></a>
### string Name

[`src/libraries/z42.json/src/JsonPropertyAttribute.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonPropertyAttribute.z42#L8)

JSON 中使用的键名。

<a id="Std.Json.JsonPropertyAttribute.ctor(string)"></a>
### JsonPropertyAttribute(string name)

[`src/libraries/z42.json/src/JsonPropertyAttribute.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonPropertyAttribute.z42#L11)

以覆盖默认成员名的 JSON 键名构造特性。

## JsonSerializer

[`src/libraries/z42.json/src/JsonSerializer.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonSerializer.z42#L7)

对象 ↔ JSON 序列化：经反射在用户对象与 JSON 文本之间直转。

序列化按对象**运行期类型**递归（多态元素随其真实类型）；反序列化按**目标 Type** 递归
（JsonBinder）。成员统一由 JsonMembers 提供（字段 + 属性）；[JsonProperty]/[JsonIgnore] 生效。

```z42
public static class JsonSerializer {
    static string Serialize(object value)
    static string SerializePretty(object value)
    static T Deserialize<T>(string json)
    static object Deserialize(Type type, string json)
}
```

成员：[`Serialize`](#Std.Json.JsonSerializer.Serialize(object)) · [`SerializePretty`](#Std.Json.JsonSerializer.SerializePretty(object)) · [`Deserialize`](#Std.Json.JsonSerializer.Deserialize%3CT%3E(string)) · [`Deserialize`](#Std.Json.JsonSerializer.Deserialize(Type,string))

<a id="Std.Json.JsonSerializer.Serialize(object)"></a>
### static string Serialize(object value)

[`src/libraries/z42.json/src/JsonSerializer.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonSerializer.z42#L9)

把 `value` 序列化为紧凑 JSON 文本（无换行缩进）；null 序列化为 "null"。

<a id="Std.Json.JsonSerializer.SerializePretty(object)"></a>
### static string SerializePretty(object value)

[`src/libraries/z42.json/src/JsonSerializer.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonSerializer.z42#L14)

同 Serialize，但输出按两层缩进换行的易读格式。

<a id="Std.Json.JsonSerializer.Deserialize<T>(string)"></a>
### static T Deserialize<T>(string json)

[`src/libraries/z42.json/src/JsonSerializer.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonSerializer.z42#L19)

解析 `json` 并按 `T` 绑定返回；JSON null / 语法错误按 JsonBinder / JsonParser 语义抛出。

<a id="Std.Json.JsonSerializer.Deserialize(Type,string)"></a>
### static object Deserialize(Type type, string json)

[`src/libraries/z42.json/src/JsonSerializer.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonSerializer.z42#L24)

同泛型版 Deserialize，目标类型运行期给定（多态反序列化入口）。

## JsonValue

[`src/libraries/z42.json/src/JsonValue.z42:29`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L29)

值树的唯一节点类型与包的入口点：一个 `JsonValue` 属于七种 kind 之一。

七种 kind：Null / Bool / Long / Double / String / Array / Object。

```z42
public class JsonValue {
    static JsonValue Parse(string text)
    static JsonValue ParseRelaxed(string text)
    static string Stringify(JsonValue v)
    static string StringifyPretty(JsonValue v)
    static JsonValue ParseStream(Stream source)
    static JsonValue ParseRelaxedStream(Stream source)
    static void WriteTo(Stream dest, JsonValue v)
    static void WriteToPretty(Stream dest, JsonValue v)
    static JsonValue OfNull()
    static JsonValue OfBool(bool b)
    static JsonValue OfLong(long n)
    static JsonValue OfDouble(double d)
    static JsonValue OfString(string s)
    static JsonValue OfArray()
    static JsonValue OfObject()
    bool IsNull()
    bool IsBool()
    bool IsLong()
    bool IsDouble()
    bool IsString()
    bool IsArray()
    bool IsObject()
    string KindName()
    bool AsBool()
    long AsLong()
    double AsDouble()
    string AsString()
    bool ContainsKey(string key)
    JsonValue Get(string key)
    bool TryGet(string key, ref JsonValue value)
    bool TryGetValue<T>(string key, ref T value)
    void Set(string key, JsonValue v)
    string[] Keys()
    int Count()
    int Length()
    JsonValue At(int i)
    void Add(JsonValue v)
}
```

成员：[`Parse`](#Std.Json.JsonValue.Parse(string)) · [`ParseRelaxed`](#Std.Json.JsonValue.ParseRelaxed(string)) · [`Stringify`](#Std.Json.JsonValue.Stringify(JsonValue)) · [`StringifyPretty`](#Std.Json.JsonValue.StringifyPretty(JsonValue)) · [`ParseStream`](#Std.Json.JsonValue.ParseStream(Stream)) · [`ParseRelaxedStream`](#Std.Json.JsonValue.ParseRelaxedStream(Stream)) · [`WriteTo`](#Std.Json.JsonValue.WriteTo(Stream,JsonValue)) · [`WriteToPretty`](#Std.Json.JsonValue.WriteToPretty(Stream,JsonValue)) · [`OfNull`](#Std.Json.JsonValue.OfNull()) · [`OfBool`](#Std.Json.JsonValue.OfBool(bool)) · [`OfLong`](#Std.Json.JsonValue.OfLong(long)) · [`OfDouble`](#Std.Json.JsonValue.OfDouble(double)) · [`OfString`](#Std.Json.JsonValue.OfString(string)) · [`OfArray`](#Std.Json.JsonValue.OfArray()) · [`OfObject`](#Std.Json.JsonValue.OfObject()) · [`IsNull`](#Std.Json.JsonValue.IsNull()) · [`IsBool`](#Std.Json.JsonValue.IsBool()) · [`IsLong`](#Std.Json.JsonValue.IsLong()) · [`IsDouble`](#Std.Json.JsonValue.IsDouble()) · [`IsString`](#Std.Json.JsonValue.IsString()) · [`IsArray`](#Std.Json.JsonValue.IsArray()) · [`IsObject`](#Std.Json.JsonValue.IsObject()) · [`KindName`](#Std.Json.JsonValue.KindName()) · [`AsBool`](#Std.Json.JsonValue.AsBool()) · [`AsLong`](#Std.Json.JsonValue.AsLong()) · [`AsDouble`](#Std.Json.JsonValue.AsDouble()) · [`AsString`](#Std.Json.JsonValue.AsString()) · [`ContainsKey`](#Std.Json.JsonValue.ContainsKey(string)) · [`Get`](#Std.Json.JsonValue.Get(string)) · [`TryGet`](#Std.Json.JsonValue.TryGet(string,JsonValue)) · [`TryGetValue`](#Std.Json.JsonValue.TryGetValue%3CT%3E(string,T)) · [`Set`](#Std.Json.JsonValue.Set(string,JsonValue)) · [`Keys`](#Std.Json.JsonValue.Keys()) · [`Count`](#Std.Json.JsonValue.Count()) · [`Length`](#Std.Json.JsonValue.Length()) · [`At`](#Std.Json.JsonValue.At(int)) · [`Add`](#Std.Json.JsonValue.Add(JsonValue))

<a id="Std.Json.JsonValue.Parse(string)"></a>
### static JsonValue Parse(string text)

[`src/libraries/z42.json/src/JsonValue.z42:63`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L63)

严格按 RFC 8259 解析 JSON 文本为根值；语法错误抛带行 / 列的 `JsonException`， 空输入按 RFC 8259 视为非法。

<a id="Std.Json.JsonValue.ParseRelaxed(string)"></a>
### static JsonValue ParseRelaxed(string text)

[`src/libraries/z42.json/src/JsonValue.z42:72`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L72)

宽松（JSON5 风格）解析：额外接受注释与尾逗号，适合工具配置。

额外接受：`//` 行注释、`/* */` 块注释、数组 / 对象尾逗号。
典型场景：`tsconfig.json`、`.vscode/settings.json`、Docker `compose.json`。

<a id="Std.Json.JsonValue.Stringify(JsonValue)"></a>
### static string Stringify(JsonValue v)

[`src/libraries/z42.json/src/JsonValue.z42:78`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L78)

序列化为紧凑 JSON（无空白）。

<a id="Std.Json.JsonValue.StringifyPretty(JsonValue)"></a>
### static string StringifyPretty(JsonValue v)

[`src/libraries/z42.json/src/JsonValue.z42:84`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L84)

序列化为缩进 JSON（2 空格缩进 + 换行）。

<a id="Std.Json.JsonValue.ParseStream(Stream)"></a>
### static JsonValue ParseStream(Stream source)

[`src/libraries/z42.json/src/JsonValue.z42:99`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L99)

从 UTF-8 字节流解析 JSON；读至 EOF，不关闭传入的流。

<a id="Std.Json.JsonValue.ParseRelaxedStream(Stream)"></a>
### static JsonValue ParseRelaxedStream(Stream source)

[`src/libraries/z42.json/src/JsonValue.z42:107`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L107)

从 UTF-8 字节流宽松解析 JSON，宽松项与 `ParseRelaxed(string)` 相同。

<a id="Std.Json.JsonValue.WriteTo(Stream,JsonValue)"></a>
### static void WriteTo(Stream dest, JsonValue v)

[`src/libraries/z42.json/src/JsonValue.z42:115`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L115)

以 UTF-8 字节写出紧凑 JSON；不关闭 `dest`（生命周期归调用方）。缩进输出用 `WriteToPretty`。

<a id="Std.Json.JsonValue.WriteToPretty(Stream,JsonValue)"></a>
### static void WriteToPretty(Stream dest, JsonValue v)

[`src/libraries/z42.json/src/JsonValue.z42:122`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L122)

以 UTF-8 字节写出缩进 JSON；生命周期语义见 `WriteTo`。

<a id="Std.Json.JsonValue.OfNull()"></a>
### static JsonValue OfNull()

[`src/libraries/z42.json/src/JsonValue.z42:131`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L131)

构造一个 Null 值。

<a id="Std.Json.JsonValue.OfBool(bool)"></a>
### static JsonValue OfBool(bool b)

[`src/libraries/z42.json/src/JsonValue.z42:133`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L133)

构造一个值为 `b` 的 Bool 值。

<a id="Std.Json.JsonValue.OfLong(long)"></a>
### static JsonValue OfLong(long n)

[`src/libraries/z42.json/src/JsonValue.z42:135`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L135)

构造一个值为 `n` 的 Long 值。

<a id="Std.Json.JsonValue.OfDouble(double)"></a>
### static JsonValue OfDouble(double d)

[`src/libraries/z42.json/src/JsonValue.z42:137`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L137)

构造一个值为 `d` 的 Double 值。

<a id="Std.Json.JsonValue.OfString(string)"></a>
### static JsonValue OfString(string s)

[`src/libraries/z42.json/src/JsonValue.z42:139`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L139)

构造一个值为 `s` 的 String 值。

<a id="Std.Json.JsonValue.OfArray()"></a>
### static JsonValue OfArray()

[`src/libraries/z42.json/src/JsonValue.z42:141`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L141)

构造一个空 Array 值（配合 `Add` 逐项追加）。

<a id="Std.Json.JsonValue.OfObject()"></a>
### static JsonValue OfObject()

[`src/libraries/z42.json/src/JsonValue.z42:149`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L149)

构造一个空 Object 值（配合 `Set` 逐键填充）。

<a id="Std.Json.JsonValue.IsNull()"></a>
### bool IsNull()

[`src/libraries/z42.json/src/JsonValue.z42:162`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L162)

本值是否为 Null。

<a id="Std.Json.JsonValue.IsBool()"></a>
### bool IsBool()

[`src/libraries/z42.json/src/JsonValue.z42:164`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L164)

本值是否为 Bool。

<a id="Std.Json.JsonValue.IsLong()"></a>
### bool IsLong()

[`src/libraries/z42.json/src/JsonValue.z42:166`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L166)

本值是否为 Long。

<a id="Std.Json.JsonValue.IsDouble()"></a>
### bool IsDouble()

[`src/libraries/z42.json/src/JsonValue.z42:168`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L168)

本值是否为 Double。

<a id="Std.Json.JsonValue.IsString()"></a>
### bool IsString()

[`src/libraries/z42.json/src/JsonValue.z42:170`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L170)

本值是否为 String。

<a id="Std.Json.JsonValue.IsArray()"></a>
### bool IsArray()

[`src/libraries/z42.json/src/JsonValue.z42:172`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L172)

本值是否为 Array。

<a id="Std.Json.JsonValue.IsObject()"></a>
### bool IsObject()

[`src/libraries/z42.json/src/JsonValue.z42:174`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L174)

本值是否为 Object。

<a id="Std.Json.JsonValue.KindName()"></a>
### string KindName()

[`src/libraries/z42.json/src/JsonValue.z42:177`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L177)

kind 的小写名称（`null`…`object`，未知 kind 给 `unknown`），多用于错误消息。

<a id="Std.Json.JsonValue.AsBool()"></a>
### bool AsBool()

[`src/libraries/z42.json/src/JsonValue.z42:192`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L192)

取 bool 值；kind 非 Bool 时抛 `JsonException`。

<a id="Std.Json.JsonValue.AsLong()"></a>
### long AsLong()

[`src/libraries/z42.json/src/JsonValue.z42:197`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L197)

取 long 值；kind 非 Long 时抛 `JsonException`。

<a id="Std.Json.JsonValue.AsDouble()"></a>
### double AsDouble()

[`src/libraries/z42.json/src/JsonValue.z42:202`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L202)

取 double 值；Long 自动升格为 double，其余非 Double kind 抛 `JsonException`。

<a id="Std.Json.JsonValue.AsString()"></a>
### string AsString()

[`src/libraries/z42.json/src/JsonValue.z42:208`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L208)

取 string 值；kind 非 String 时抛 `JsonException`。

<a id="Std.Json.JsonValue.ContainsKey(string)"></a>
### bool ContainsKey(string key)

[`src/libraries/z42.json/src/JsonValue.z42:216`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L216)

object 是否含指定键；非 object kind 一律返回 false（不抛）。

<a id="Std.Json.JsonValue.Get(string)"></a>
### JsonValue Get(string key)

[`src/libraries/z42.json/src/JsonValue.z42:222`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L222)

按键取子值；非 object 或键不存在时抛 `JsonException`。

<a id="Std.Json.JsonValue.TryGet(string,JsonValue)"></a>
### bool TryGet(string key, ref JsonValue value)

[`src/libraries/z42.json/src/JsonValue.z42:261`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L261)

取即检查：命中则写入 `value` 并返回 true；未命中返回 false 且**不动** `value`。

本值不是 object 时返回 false 而**不抛** —— 与 `ContainsKey` 一致。

<a id="Std.Json.JsonValue.TryGetValue<T>(string,T)"></a>
### bool TryGetValue<T>(string key, ref T value)

[`src/libraries/z42.json/src/JsonValue.z42:279`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L279)

取即检查 + 取标量：命中则按 `T` 取值写入 `value` 并返回 true；未命中返回 false 且 **不动** `value`。

「有就覆盖默认值」一行写完。

⚠️ 命中但**类型不符照抛** `JsonException`，与 `AsString()` / `AsLong()` 一字不差 ——
「键不在」与「值类型错」是两种失败，把后者吞成 false 会让数据里的类型错**静默退回默认值**。

支持的 `T`：`string` / `long` / `double` / `bool` / `JsonValue`（原样取子树）。
其它 `T` 抛异常：那是**调用方写错了代码**，返回 false 会把它伪装成「键不在」。

<a id="Std.Json.JsonValue.Set(string,JsonValue)"></a>
### void Set(string key, JsonValue v)

[`src/libraries/z42.json/src/JsonValue.z42:293`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L293)

写入键值：已有键原地覆盖（顺序不变），新键追加到末尾；非 object 抛 `JsonException`。

<a id="Std.Json.JsonValue.Keys()"></a>
### string[] Keys()

[`src/libraries/z42.json/src/JsonValue.z42:328`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L328)

对象的全部键，按插入顺序；非 object kind 抛 `JsonException`。

<a id="Std.Json.JsonValue.Count()"></a>
### int Count()

[`src/libraries/z42.json/src/JsonValue.z42:342`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L342)

元素个数：Array 为元素数，Object 为键数；其余 kind 抛 `JsonException`。

<a id="Std.Json.JsonValue.Length()"></a>
### int Length()

[`src/libraries/z42.json/src/JsonValue.z42:351`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L351)

数组长度；非 array kind 抛 `JsonException`。

<a id="Std.Json.JsonValue.At(int)"></a>
### JsonValue At(int i)

[`src/libraries/z42.json/src/JsonValue.z42:359`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L359)

按下标（0 起）取元素；非数组或下标越界抛 `JsonException`。

<a id="Std.Json.JsonValue.Add(JsonValue)"></a>
### void Add(JsonValue v)

[`src/libraries/z42.json/src/JsonValue.z42:370`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L370)

追加一个元素到数组末尾；非 array kind 抛 `JsonException`。

## JsonWriter

[`src/libraries/z42.json/src/JsonWriter.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonWriter.z42#L4)

JSON 序列化器（紧凑或缩进）。

```z42
public sealed class JsonWriter {
    JsonWriter(bool pretty)
    string Write(JsonValue v)
}
```

成员：[`JsonWriter`](#Std.Json.JsonWriter.ctor(bool)) · [`Write`](#Std.Json.JsonWriter.Write(JsonValue))

<a id="Std.Json.JsonWriter.ctor(bool)"></a>
### JsonWriter(bool pretty)

[`src/libraries/z42.json/src/JsonWriter.z42:16`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonWriter.z42#L16)

`pretty=true` 输出换行 + 两空格缩进，否则单行紧凑格式。

<a id="Std.Json.JsonWriter.Write(JsonValue)"></a>
### string Write(JsonValue v)

[`src/libraries/z42.json/src/JsonWriter.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonWriter.z42#L24)

把 `v` 写成 JSON 文本并返回（一次性拼接全部片段）。

