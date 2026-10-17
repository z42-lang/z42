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

`src/libraries/z42.json/src/JsonBinder.z42:9`

反序列化反射核：按目标 Type 递归把 JSON 绑定为对象，故 JSON 的整数 / 浮点能正确落到 int / long / double 目标成员。

构造模型：有无参构造器（或无构造器）→ Activator + 按名赋值；否则选参数最多的构造器按
参数名匹配 JSON 键调用，再补齐剩余可写成员。类型覆盖基元 + 嵌套对象 + 定长数组 T[]。

```z42
public static class JsonBinder {
    static object FromJson(Type t, JsonValue v)
}
```

### static object FromJson(Type t, JsonValue v)

`src/libraries/z42.json/src/JsonBinder.z42:10`


## JsonException

`src/libraries/z42.json/src/JsonException.z42:4`

JSON 解析 / 序列化错误；携带 1-based 行 / 列号（0 = 不可用，如 stringify 阶段错误）。

```z42
public class JsonException : Exception {
    int Line
    int Column
    JsonException(string message)
    JsonException(string message, int line, int column)
}
```

### int Line

`src/libraries/z42.json/src/JsonException.z42:5`


### int Column

`src/libraries/z42.json/src/JsonException.z42:6`


### JsonException(string message)

`src/libraries/z42.json/src/JsonException.z42:8`


### JsonException(string message, int line, int column)

`src/libraries/z42.json/src/JsonException.z42:13`


## JsonIgnoreAttribute

`src/libraries/z42.json/src/JsonIgnoreAttribute.z42:4`

标注成员在序列化 / 反序列化时被跳过。用法 `[JsonIgnore]`，可标注字段或属性。

```z42
public class JsonIgnoreAttribute : Attribute {
}
```

## JsonMember

`src/libraries/z42.json/src/JsonMember.z42:8`

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

### string JsonKey

`src/libraries/z42.json/src/JsonMember.z42:14`

JSON 键名（[JsonProperty] 覆盖或成员名）。

### Type MemberType

`src/libraries/z42.json/src/JsonMember.z42:16`

成员的静态类型。

### bool CanRead

`src/libraries/z42.json/src/JsonMember.z42:18`

可读（序列化）。

### bool CanWrite

`src/libraries/z42.json/src/JsonMember.z42:20`

可写（反序列化）。

### object Get(object target)

`src/libraries/z42.json/src/JsonMember.z42:22`


### void Set(object target, object value)

`src/libraries/z42.json/src/JsonMember.z42:27`


### static JsonMember OfField(FieldInfo f, string key)

`src/libraries/z42.json/src/JsonMember.z42:32`


### static JsonMember OfProperty(PropertyInfo p, string key)

`src/libraries/z42.json/src/JsonMember.z42:42`


## JsonMembers

`src/libraries/z42.json/src/JsonMember.z42:52`

成员集合构建：从 Type 收集 public 实例字段 + public 属性，解析键名 / 忽略。

```z42
public static class JsonMembers {
    static JsonMember[] For(Type t)
}
```

### static JsonMember[] For(Type t)

`src/libraries/z42.json/src/JsonMember.z42:54`

收集 t 的可序列化成员（public 非 static 字段 + 属性；[JsonIgnore] 排除）。

## JsonParser

`src/libraries/z42.json/src/JsonParser.z42:5`

递归下降的 RFC 8259 JSON 解析器。

```z42
public sealed class JsonParser {
    JsonParser(string src)
    static JsonParser Relaxed(string src)
    JsonValue ParseDocument()
}
```

### JsonParser(string src)

`src/libraries/z42.json/src/JsonParser.z42:25`


### static JsonParser Relaxed(string src)

`src/libraries/z42.json/src/JsonParser.z42:35`

构建宽松模式解析器：接受 JSON5 风格的 `//` / `/* */` 注释与数组 / 对象尾逗号。

### JsonValue ParseDocument()

`src/libraries/z42.json/src/JsonParser.z42:41`


## JsonPath

`src/libraries/z42.json/src/JsonPath.z42:14`

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

### static JsonValue? Select(JsonValue root, string path)

`src/libraries/z42.json/src/JsonPath.z42:17`

按 `path` 遍历 `root`，返回路径上的值；路径走不通（键缺失、下标越界、kind 不符）返回 null。

## JsonPropertyAttribute

`src/libraries/z42.json/src/JsonPropertyAttribute.z42:5`

显式指定成员的 JSON 键名，serde 引擎按名映射时以此覆盖默认成员名。 用法 `[JsonProperty("json_name")]`，可标注字段或 auto-property。

```z42
public class JsonPropertyAttribute : Attribute {
    string Name
    JsonPropertyAttribute(string name)
}
```

### string Name

`src/libraries/z42.json/src/JsonPropertyAttribute.z42:7`

JSON 中使用的键名。

### JsonPropertyAttribute(string name)

`src/libraries/z42.json/src/JsonPropertyAttribute.z42:9`


## JsonSerializer

`src/libraries/z42.json/src/JsonSerializer.z42:7`

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

### static string Serialize(object value)

`src/libraries/z42.json/src/JsonSerializer.z42:8`


### static string SerializePretty(object value)

`src/libraries/z42.json/src/JsonSerializer.z42:12`


### static T Deserialize<T>(string json)

`src/libraries/z42.json/src/JsonSerializer.z42:16`


### static object Deserialize(Type type, string json)

`src/libraries/z42.json/src/JsonSerializer.z42:20`


## JsonValue

`src/libraries/z42.json/src/JsonValue.z42:28`

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

### static JsonValue Parse(string text)

`src/libraries/z42.json/src/JsonValue.z42:62`

严格按 RFC 8259 解析 JSON 文本为根值；语法错误抛带行 / 列的 `JsonException`， 空输入按 RFC 8259 视为非法。

### static JsonValue ParseRelaxed(string text)

`src/libraries/z42.json/src/JsonValue.z42:69`

宽松（JSON5 风格）解析：额外接受 `//` 行注释、`/* */` 块注释与数组 / 对象尾逗号， 适合工具配置（`tsconfig.json`、`.vscode/settings.json`、Docker `compose.json`）。

### static string Stringify(JsonValue v)

`src/libraries/z42.json/src/JsonValue.z42:75`

序列化为紧凑 JSON（无空白）。

### static string StringifyPretty(JsonValue v)

`src/libraries/z42.json/src/JsonValue.z42:81`

序列化为缩进 JSON（2 空格缩进 + 换行）。

### static JsonValue ParseStream(Stream source)

`src/libraries/z42.json/src/JsonValue.z42:96`

从 UTF-8 字节流解析 JSON；读至 EOF，不关闭传入的流。

### static JsonValue ParseRelaxedStream(Stream source)

`src/libraries/z42.json/src/JsonValue.z42:104`

从 UTF-8 字节流宽松解析 JSON，宽松项与 `ParseRelaxed(string)` 相同。

### static void WriteTo(Stream dest, JsonValue v)

`src/libraries/z42.json/src/JsonValue.z42:112`

以 UTF-8 字节写出紧凑 JSON；不关闭 `dest`（生命周期归调用方）。缩进输出用 `WriteToPretty`。

### static void WriteToPretty(Stream dest, JsonValue v)

`src/libraries/z42.json/src/JsonValue.z42:119`

以 UTF-8 字节写出缩进 JSON；生命周期语义见 `WriteTo`。

### static JsonValue OfNull()

`src/libraries/z42.json/src/JsonValue.z42:127`


### static JsonValue OfBool(bool b)

`src/libraries/z42.json/src/JsonValue.z42:128`


### static JsonValue OfLong(long n)

`src/libraries/z42.json/src/JsonValue.z42:129`


### static JsonValue OfDouble(double d)

`src/libraries/z42.json/src/JsonValue.z42:130`


### static JsonValue OfString(string s)

`src/libraries/z42.json/src/JsonValue.z42:131`


### static JsonValue OfArray()

`src/libraries/z42.json/src/JsonValue.z42:132`


### static JsonValue OfObject()

`src/libraries/z42.json/src/JsonValue.z42:139`


### bool IsNull()

`src/libraries/z42.json/src/JsonValue.z42:151`


### bool IsBool()

`src/libraries/z42.json/src/JsonValue.z42:152`


### bool IsLong()

`src/libraries/z42.json/src/JsonValue.z42:153`


### bool IsDouble()

`src/libraries/z42.json/src/JsonValue.z42:154`


### bool IsString()

`src/libraries/z42.json/src/JsonValue.z42:155`


### bool IsArray()

`src/libraries/z42.json/src/JsonValue.z42:156`


### bool IsObject()

`src/libraries/z42.json/src/JsonValue.z42:157`


### string KindName()

`src/libraries/z42.json/src/JsonValue.z42:159`


### bool AsBool()

`src/libraries/z42.json/src/JsonValue.z42:173`


### long AsLong()

`src/libraries/z42.json/src/JsonValue.z42:177`


### double AsDouble()

`src/libraries/z42.json/src/JsonValue.z42:181`


### string AsString()

`src/libraries/z42.json/src/JsonValue.z42:186`


### bool ContainsKey(string key)

`src/libraries/z42.json/src/JsonValue.z42:193`


### JsonValue Get(string key)

`src/libraries/z42.json/src/JsonValue.z42:198`


### bool TryGet(string key, ref JsonValue value)

`src/libraries/z42.json/src/JsonValue.z42:236`

取即检查：命中则写入 `value` 并返回 true；未命中返回 false 且**不动** `value`。 本值不是 object 时返回 false 而**不抛** —— 与 `ContainsKey` 一致。

### bool TryGetValue<T>(string key, ref T value)

`src/libraries/z42.json/src/JsonValue.z42:252`

取即检查 + 取标量：命中则按 `T` 取值写入 `value` 并返回 true；未命中返回 false 且 **不动** `value`（「有就覆盖默认值」一行写完）。

⚠️ 命中但**类型不符照抛** `JsonException`，与 `AsString()` / `AsLong()` 一字不差 ——
「键不在」与「值类型错」是两种失败，把后者吞成 false 会让数据里的类型错**静默退回默认值**。

支持的 `T`：`string` / `long` / `double` / `bool` / `JsonValue`（原样取子树）。
其它 `T` 抛异常：那是**调用方写错了代码**，返回 false 会把它伪装成「键不在」。

### void Set(string key, JsonValue v)

`src/libraries/z42.json/src/JsonValue.z42:265`


### string[] Keys()

`src/libraries/z42.json/src/JsonValue.z42:299`


### int Count()

`src/libraries/z42.json/src/JsonValue.z42:312`


### int Length()

`src/libraries/z42.json/src/JsonValue.z42:320`


### JsonValue At(int i)

`src/libraries/z42.json/src/JsonValue.z42:327`


### void Add(JsonValue v)

`src/libraries/z42.json/src/JsonValue.z42:337`


## JsonWriter

`src/libraries/z42.json/src/JsonWriter.z42:4`

JSON 序列化器（紧凑或缩进）。

```z42
public sealed class JsonWriter {
    JsonWriter(bool pretty)
    string Write(JsonValue v)
}
```

### JsonWriter(bool pretty)

`src/libraries/z42.json/src/JsonWriter.z42:15`


### string Write(JsonValue v)

`src/libraries/z42.json/src/JsonWriter.z42:22`


