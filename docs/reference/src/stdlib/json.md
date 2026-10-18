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

[`src/libraries/z42.json/src/JsonBinder.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonBinder.z42#L10)


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

[`src/libraries/z42.json/src/JsonException.z42:5`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonException.z42#L5)


<a id="Std.Json.JsonException.Column"></a>
### int Column

[`src/libraries/z42.json/src/JsonException.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonException.z42#L6)


<a id="Std.Json.JsonException.ctor(string)"></a>
### JsonException(string message)

[`src/libraries/z42.json/src/JsonException.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonException.z42#L8)


<a id="Std.Json.JsonException.ctor(string,int,int)"></a>
### JsonException(string message, int line, int column)

[`src/libraries/z42.json/src/JsonException.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonException.z42#L13)


## JsonIgnoreAttribute

[`src/libraries/z42.json/src/JsonIgnoreAttribute.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonIgnoreAttribute.z42#L4)

标注成员在序列化 / 反序列化时被跳过。用法 `[JsonIgnore]`，可标注字段或属性。

```z42
public class JsonIgnoreAttribute : Attribute {
}
```

## JsonMember

[`src/libraries/z42.json/src/JsonMember.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonMember.z42#L8)

统一「可序列化成员」抽象：封装字段 (FieldInfo) 或属性 (PropertyInfo)，携带解析后的 JSON 键名、成员类型、读写能力；序列化 / 反序列化引擎按这层统一遍历，无需区分字段与属性。

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

[`src/libraries/z42.json/src/JsonMember.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonMember.z42#L14)

JSON 键名（[JsonProperty] 覆盖或成员名）。

<a id="Std.Json.JsonMember.MemberType"></a>
### Type MemberType

[`src/libraries/z42.json/src/JsonMember.z42:16`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonMember.z42#L16)

成员的静态类型。

<a id="Std.Json.JsonMember.CanRead"></a>
### bool CanRead

[`src/libraries/z42.json/src/JsonMember.z42:18`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonMember.z42#L18)

可读（序列化）。

<a id="Std.Json.JsonMember.CanWrite"></a>
### bool CanWrite

[`src/libraries/z42.json/src/JsonMember.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonMember.z42#L20)

可写（反序列化）。

<a id="Std.Json.JsonMember.Get(object)"></a>
### object Get(object target)

[`src/libraries/z42.json/src/JsonMember.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonMember.z42#L22)


<a id="Std.Json.JsonMember.Set(object,object)"></a>
### void Set(object target, object value)

[`src/libraries/z42.json/src/JsonMember.z42:27`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonMember.z42#L27)


<a id="Std.Json.JsonMember.OfField(FieldInfo,string)"></a>
### static JsonMember OfField(FieldInfo f, string key)

[`src/libraries/z42.json/src/JsonMember.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonMember.z42#L32)


<a id="Std.Json.JsonMember.OfProperty(PropertyInfo,string)"></a>
### static JsonMember OfProperty(PropertyInfo p, string key)

[`src/libraries/z42.json/src/JsonMember.z42:42`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonMember.z42#L42)


## JsonMembers

[`src/libraries/z42.json/src/JsonMember.z42:52`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonMember.z42#L52)

成员集合构建：从 Type 收集 public 实例字段 + public 属性，解析键名 / 忽略。

```z42
public static class JsonMembers {
    static JsonMember[] For(Type t)
}
```

成员：[`For`](#Std.Json.JsonMembers.For(Type))

<a id="Std.Json.JsonMembers.For(Type)"></a>
### static JsonMember[] For(Type t)

[`src/libraries/z42.json/src/JsonMember.z42:54`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonMember.z42#L54)

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

[`src/libraries/z42.json/src/JsonParser.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonParser.z42#L25)


<a id="Std.Json.JsonParser.Relaxed(string)"></a>
### static JsonParser Relaxed(string src)

[`src/libraries/z42.json/src/JsonParser.z42:35`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonParser.z42#L35)

构建宽松模式解析器：接受 JSON5 风格的 `//` / `/* */` 注释与数组 / 对象尾逗号。

<a id="Std.Json.JsonParser.ParseDocument()"></a>
### JsonValue ParseDocument()

[`src/libraries/z42.json/src/JsonParser.z42:41`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonParser.z42#L41)


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

[`src/libraries/z42.json/src/JsonPropertyAttribute.z42:5`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonPropertyAttribute.z42#L5)

显式指定成员的 JSON 键名，serde 引擎按名映射时以此覆盖默认成员名。 用法 `[JsonProperty("json_name")]`，可标注字段或 auto-property。

```z42
public class JsonPropertyAttribute : Attribute {
    string Name
    JsonPropertyAttribute(string name)
}
```

成员：[`Name`](#Std.Json.JsonPropertyAttribute.Name) · [`JsonPropertyAttribute`](#Std.Json.JsonPropertyAttribute.ctor(string))

<a id="Std.Json.JsonPropertyAttribute.Name"></a>
### string Name

[`src/libraries/z42.json/src/JsonPropertyAttribute.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonPropertyAttribute.z42#L7)

JSON 中使用的键名。

<a id="Std.Json.JsonPropertyAttribute.ctor(string)"></a>
### JsonPropertyAttribute(string name)

[`src/libraries/z42.json/src/JsonPropertyAttribute.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonPropertyAttribute.z42#L9)


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

[`src/libraries/z42.json/src/JsonSerializer.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonSerializer.z42#L8)


<a id="Std.Json.JsonSerializer.SerializePretty(object)"></a>
### static string SerializePretty(object value)

[`src/libraries/z42.json/src/JsonSerializer.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonSerializer.z42#L12)


<a id="Std.Json.JsonSerializer.Deserialize<T>(string)"></a>
### static T Deserialize<T>(string json)

[`src/libraries/z42.json/src/JsonSerializer.z42:16`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonSerializer.z42#L16)


<a id="Std.Json.JsonSerializer.Deserialize(Type,string)"></a>
### static object Deserialize(Type type, string json)

[`src/libraries/z42.json/src/JsonSerializer.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonSerializer.z42#L20)


## JsonValue

[`src/libraries/z42.json/src/JsonValue.z42:28`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L28)

值树的唯一节点类型与包的入口点：一个 `JsonValue` 属于 Null / Bool / Long / Double / String / Array / Object 七种 kind 之一。

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

[`src/libraries/z42.json/src/JsonValue.z42:62`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L62)

严格按 RFC 8259 解析 JSON 文本为根值；语法错误抛带行 / 列的 `JsonException`， 空输入按 RFC 8259 视为非法。

<a id="Std.Json.JsonValue.ParseRelaxed(string)"></a>
### static JsonValue ParseRelaxed(string text)

[`src/libraries/z42.json/src/JsonValue.z42:69`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L69)

宽松（JSON5 风格）解析：额外接受 `//` 行注释、`/* */` 块注释与数组 / 对象尾逗号， 适合工具配置（`tsconfig.json`、`.vscode/settings.json`、Docker `compose.json`）。

<a id="Std.Json.JsonValue.Stringify(JsonValue)"></a>
### static string Stringify(JsonValue v)

[`src/libraries/z42.json/src/JsonValue.z42:75`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L75)

序列化为紧凑 JSON（无空白）。

<a id="Std.Json.JsonValue.StringifyPretty(JsonValue)"></a>
### static string StringifyPretty(JsonValue v)

[`src/libraries/z42.json/src/JsonValue.z42:81`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L81)

序列化为缩进 JSON（2 空格缩进 + 换行）。

<a id="Std.Json.JsonValue.ParseStream(Stream)"></a>
### static JsonValue ParseStream(Stream source)

[`src/libraries/z42.json/src/JsonValue.z42:96`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L96)

从 UTF-8 字节流解析 JSON；读至 EOF，不关闭传入的流。

<a id="Std.Json.JsonValue.ParseRelaxedStream(Stream)"></a>
### static JsonValue ParseRelaxedStream(Stream source)

[`src/libraries/z42.json/src/JsonValue.z42:104`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L104)

从 UTF-8 字节流宽松解析 JSON，宽松项与 `ParseRelaxed(string)` 相同。

<a id="Std.Json.JsonValue.WriteTo(Stream,JsonValue)"></a>
### static void WriteTo(Stream dest, JsonValue v)

[`src/libraries/z42.json/src/JsonValue.z42:112`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L112)

以 UTF-8 字节写出紧凑 JSON；不关闭 `dest`（生命周期归调用方）。缩进输出用 `WriteToPretty`。

<a id="Std.Json.JsonValue.WriteToPretty(Stream,JsonValue)"></a>
### static void WriteToPretty(Stream dest, JsonValue v)

[`src/libraries/z42.json/src/JsonValue.z42:119`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L119)

以 UTF-8 字节写出缩进 JSON；生命周期语义见 `WriteTo`。

<a id="Std.Json.JsonValue.OfNull()"></a>
### static JsonValue OfNull()

[`src/libraries/z42.json/src/JsonValue.z42:127`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L127)


<a id="Std.Json.JsonValue.OfBool(bool)"></a>
### static JsonValue OfBool(bool b)

[`src/libraries/z42.json/src/JsonValue.z42:128`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L128)


<a id="Std.Json.JsonValue.OfLong(long)"></a>
### static JsonValue OfLong(long n)

[`src/libraries/z42.json/src/JsonValue.z42:129`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L129)


<a id="Std.Json.JsonValue.OfDouble(double)"></a>
### static JsonValue OfDouble(double d)

[`src/libraries/z42.json/src/JsonValue.z42:130`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L130)


<a id="Std.Json.JsonValue.OfString(string)"></a>
### static JsonValue OfString(string s)

[`src/libraries/z42.json/src/JsonValue.z42:131`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L131)


<a id="Std.Json.JsonValue.OfArray()"></a>
### static JsonValue OfArray()

[`src/libraries/z42.json/src/JsonValue.z42:132`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L132)


<a id="Std.Json.JsonValue.OfObject()"></a>
### static JsonValue OfObject()

[`src/libraries/z42.json/src/JsonValue.z42:139`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L139)


<a id="Std.Json.JsonValue.IsNull()"></a>
### bool IsNull()

[`src/libraries/z42.json/src/JsonValue.z42:151`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L151)


<a id="Std.Json.JsonValue.IsBool()"></a>
### bool IsBool()

[`src/libraries/z42.json/src/JsonValue.z42:152`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L152)


<a id="Std.Json.JsonValue.IsLong()"></a>
### bool IsLong()

[`src/libraries/z42.json/src/JsonValue.z42:153`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L153)


<a id="Std.Json.JsonValue.IsDouble()"></a>
### bool IsDouble()

[`src/libraries/z42.json/src/JsonValue.z42:154`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L154)


<a id="Std.Json.JsonValue.IsString()"></a>
### bool IsString()

[`src/libraries/z42.json/src/JsonValue.z42:155`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L155)


<a id="Std.Json.JsonValue.IsArray()"></a>
### bool IsArray()

[`src/libraries/z42.json/src/JsonValue.z42:156`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L156)


<a id="Std.Json.JsonValue.IsObject()"></a>
### bool IsObject()

[`src/libraries/z42.json/src/JsonValue.z42:157`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L157)


<a id="Std.Json.JsonValue.KindName()"></a>
### string KindName()

[`src/libraries/z42.json/src/JsonValue.z42:159`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L159)


<a id="Std.Json.JsonValue.AsBool()"></a>
### bool AsBool()

[`src/libraries/z42.json/src/JsonValue.z42:173`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L173)


<a id="Std.Json.JsonValue.AsLong()"></a>
### long AsLong()

[`src/libraries/z42.json/src/JsonValue.z42:177`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L177)


<a id="Std.Json.JsonValue.AsDouble()"></a>
### double AsDouble()

[`src/libraries/z42.json/src/JsonValue.z42:181`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L181)


<a id="Std.Json.JsonValue.AsString()"></a>
### string AsString()

[`src/libraries/z42.json/src/JsonValue.z42:186`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L186)


<a id="Std.Json.JsonValue.ContainsKey(string)"></a>
### bool ContainsKey(string key)

[`src/libraries/z42.json/src/JsonValue.z42:193`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L193)


<a id="Std.Json.JsonValue.Get(string)"></a>
### JsonValue Get(string key)

[`src/libraries/z42.json/src/JsonValue.z42:198`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L198)


<a id="Std.Json.JsonValue.TryGet(string,JsonValue)"></a>
### bool TryGet(string key, ref JsonValue value)

[`src/libraries/z42.json/src/JsonValue.z42:236`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L236)

取即检查：命中则写入 `value` 并返回 true；未命中返回 false 且**不动** `value`。 本值不是 object 时返回 false 而**不抛** —— 与 `ContainsKey` 一致。

<a id="Std.Json.JsonValue.TryGetValue<T>(string,T)"></a>
### bool TryGetValue<T>(string key, ref T value)

[`src/libraries/z42.json/src/JsonValue.z42:252`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L252)

取即检查 + 取标量：命中则按 `T` 取值写入 `value` 并返回 true；未命中返回 false 且 **不动** `value`（「有就覆盖默认值」一行写完）。

⚠️ 命中但**类型不符照抛** `JsonException`，与 `AsString()` / `AsLong()` 一字不差 ——
「键不在」与「值类型错」是两种失败，把后者吞成 false 会让数据里的类型错**静默退回默认值**。

支持的 `T`：`string` / `long` / `double` / `bool` / `JsonValue`（原样取子树）。
其它 `T` 抛异常：那是**调用方写错了代码**，返回 false 会把它伪装成「键不在」。

<a id="Std.Json.JsonValue.Set(string,JsonValue)"></a>
### void Set(string key, JsonValue v)

[`src/libraries/z42.json/src/JsonValue.z42:265`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L265)


<a id="Std.Json.JsonValue.Keys()"></a>
### string[] Keys()

[`src/libraries/z42.json/src/JsonValue.z42:299`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L299)


<a id="Std.Json.JsonValue.Count()"></a>
### int Count()

[`src/libraries/z42.json/src/JsonValue.z42:312`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L312)


<a id="Std.Json.JsonValue.Length()"></a>
### int Length()

[`src/libraries/z42.json/src/JsonValue.z42:320`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L320)


<a id="Std.Json.JsonValue.At(int)"></a>
### JsonValue At(int i)

[`src/libraries/z42.json/src/JsonValue.z42:327`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L327)


<a id="Std.Json.JsonValue.Add(JsonValue)"></a>
### void Add(JsonValue v)

[`src/libraries/z42.json/src/JsonValue.z42:337`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonValue.z42#L337)


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

[`src/libraries/z42.json/src/JsonWriter.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonWriter.z42#L15)


<a id="Std.Json.JsonWriter.Write(JsonValue)"></a>
### string Write(JsonValue v)

[`src/libraries/z42.json/src/JsonWriter.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.json/src/JsonWriter.z42#L22)


